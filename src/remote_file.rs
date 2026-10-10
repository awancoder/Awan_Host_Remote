//! Bounded file operations on a separate SSH exec channel, without a PTY.
use anyhow::{Context, Result, bail};
use russh::{Channel, ChannelMsg, client};

pub const MAX_BYTES: usize = 1024 * 1024;

pub enum Request {
    List { path: String, query: String, show_hidden: bool },
    Read { path: String },
    Save { path: String, revision: String, text: String },
}

pub enum Reply {
    Directory(crate::remote_directory::Directory),
    Read { path: String, revision: String, text: String },
    Saved { revision: String },
}

fn quote(value: &str) -> String { format!("'{}'", value.replace('\'', "'\"'\"'")) }

impl Request {
    pub fn validate(&self) -> Result<()> {
        if let Self::List { path, query, .. } = self {
            return crate::remote_directory::validate(path, query);
        }
        let path = match self { Self::Read { path } | Self::Save { path, .. } => path, Self::List { .. } => unreachable!() };
        if !path.starts_with('/') || path.len() > 4096 || path.chars().any(char::is_control) {
            bail!("Gunakan path absolut Linux tanpa karakter kontrol, misalnya /home/ubuntu/app/config.toml.");
        }
        if let Self::Save { revision, text, .. } = self {
            if revision.len() != 64 || !revision.bytes().all(|c| c.is_ascii_hexdigit()) { bail!("Revisi file tidak valid. Buka ulang file."); }
            if text.len() > MAX_BYTES || text.contains('\0') { bail!("Editor mendukung teks UTF-8 tanpa NUL, maksimal 1 MiB."); }
        }
        Ok(())
    }
}

pub async fn execute(channel: &mut Channel<client::Msg>, request: Request) -> Result<Reply> {
    request.validate()?;
    let (path, revision, data) = match request {
        Request::List { path, query, show_hidden } => return crate::remote_directory::read(channel, path, query, show_hidden).await.map(Reply::Directory),
        Request::Read { path } => (path, String::new(), None),
        Request::Save { path, revision, text } => (path, revision, Some(text.into_bytes())),
    };
    let saving = data.is_some();
    let command = format!("python3 -c {} {} {} {}", quote(SCRIPT), quote(&path),
        quote(if saving { "save" } else { "read" }), quote(&revision));
    channel.exec(true, command).await?;
    // The remote script consumes stdin before producing a result. No file content
    // is interpolated into the command or sent to the interactive shell.
    if let Some(data) = data { channel.data(&data[..]).await?; }
    channel.eof().await?;
    let output = receive(channel).await?;
    let output = String::from_utf8(output).context("File bukan teks UTF-8")?;
    let (header, text) = output.split_once('\n').context("Respons file tidak lengkap")?;
    let revision = header.strip_prefix("AWAN_FILE_V1 ").context("Respons file tidak dikenali")?;
    if revision.len() != 64 || !revision.bytes().all(|c| c.is_ascii_hexdigit()) || text.contains('\0') || text.len() > MAX_BYTES {
        bail!("Respons file tidak valid");
    }
    if saving {
        if !text.is_empty() { bail!("Respons penyimpanan tidak valid"); }
        Ok(Reply::Saved { revision: revision.into() })
    } else {
        Ok(Reply::Read { path, revision: revision.into(), text: text.into() })
    }
}

pub(crate) async fn receive(channel: &mut Channel<client::Msg>) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut status = None;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => {
                if output.len() + data.len() > MAX_BYTES + 256 { bail!("Output file melebihi batas editor."); }
                output.extend_from_slice(&data);
            }
            ChannelMsg::ExtendedData { data, .. } => {
                let remaining = 8192usize.saturating_sub(errors.len());
                errors.extend_from_slice(&data[..data.len().min(remaining)]);
            }
            ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
            ChannelMsg::Failure | ChannelMsg::ExitSignal { .. } => bail!("Server menolak atau menghentikan operasi file."),
            ChannelMsg::Close => break,
            _ => {},
        }
    }
    if status != Some(0) {
        bail!("Operasi file gagal ({status:?}): {}", String::from_utf8_lossy(&errors));
    }
    Ok(output)
}

// Python 3 must already exist on the Linux server. Writes use a temporary file
// in the same directory and preserve ownership, permissions, and xattrs.
const SCRIPT: &str = r#"import os, sys, stat, hashlib, tempfile
LIMIT = 1024 * 1024
path, action, expected = sys.argv[1:]
temporary = None
def digest(data):
    return hashlib.sha256(data).hexdigest()
def stamp(metadata):
    return (metadata.st_dev, metadata.st_ino, metadata.st_size, metadata.st_mtime_ns,
            metadata.st_ctime_ns, metadata.st_mode, metadata.st_uid, metadata.st_gid, metadata.st_nlink)
def snapshot():
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as source:
        before = os.fstat(source.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
            raise ValueError('Hanya file biasa tanpa symlink/hardlink yang didukung.')
        if before.st_size > LIMIT:
            raise ValueError('File melebihi batas 1 MiB.')
        data = source.read(LIMIT + 1)
        after = os.fstat(source.fileno())
        if len(data) > LIMIT or stamp(before) != stamp(after):
            raise ValueError('File berubah saat dibaca atau terlalu besar. Buka ulang file.')
        data.decode('utf-8')
        if b'\0' in data:
            raise ValueError('File biner tidak didukung.')
        return data, after
try:
    original, metadata = snapshot()
    if action == 'read':
        sys.stdout.buffer.write(('AWAN_FILE_V1 ' + digest(original) + '\n').encode() + original)
    else:
        if digest(original) != expected:
            raise ValueError('Konflik: file di server sudah berubah. Salin draft dan buka ulang file.')
        if not os.access(path, os.W_OK):
            raise PermissionError('Akun SSH tidak memiliki izin tulis file.')
        content = sys.stdin.buffer.read(LIMIT + 1)
        if len(content) > LIMIT or b'\0' in content:
            raise ValueError('Konten tidak valid atau melebihi batas 1 MiB.')
        content.decode('utf-8')
        fd, temporary = tempfile.mkstemp(prefix='.awan-edit-', dir=os.path.dirname(path))
        with os.fdopen(fd, 'wb') as target:
            target.write(content)
            target.flush()
            current = os.fstat(target.fileno())
            if (current.st_uid, current.st_gid) != (metadata.st_uid, metadata.st_gid):
                os.fchown(target.fileno(), metadata.st_uid, metadata.st_gid)
            os.fchmod(target.fileno(), stat.S_IMODE(metadata.st_mode))
            for name in os.listxattr(path, follow_symlinks=False):
                os.setxattr(temporary, name, os.getxattr(path, name, follow_symlinks=False))
            os.fsync(target.fileno())
        latest, latest_metadata = snapshot()
        if digest(latest) != expected or stamp(latest_metadata) != stamp(metadata):
            raise ValueError('Konflik: file berubah sebelum disimpan. Buka ulang file.')
        os.replace(temporary, path)
        temporary = None
        sys.stdout.write('AWAN_FILE_V1 ' + digest(content) + '\n')
except Exception as error:
    print('[Awan] ' + str(error), file=sys.stderr)
    sys.exit(1)
finally:
    if temporary is not None:
        os.unlink(temporary)
"#;
