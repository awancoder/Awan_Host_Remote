//! File-manager operations (create/delete/rename/move/copy/upload) on their
//! own SSH exec channel, independent of the text editor's read/save channel
//! (`remote_file.rs`) so a rename or an in-flight upload cannot block, or be
//! blocked by, opening/saving the currently edited file.
use anyhow::{Context, Result, bail};
use russh::{Channel, client};

/// General files (images, archives, ...) are not bounded by the text
/// editor's 1 MiB limit, but still need a hard cap to keep one SSH channel
/// and the in-memory buffer bounded.
pub const UPLOAD_MAX_BYTES: usize = 25 * 1024 * 1024;

pub enum Request {
    CreateFile { path: String },
    CreateFolder { path: String },
    Delete { path: String, recursive: bool },
    Rename { from: String, to: String },
    Copy { from: String, to: String, recursive: bool },
    Upload { path: String, data: Vec<u8> },
    /// `mode` holds the 9 rwx permission bits. With `recursive` on a folder,
    /// subfolders get `mode` and files get it without execute unless they
    /// were already executable (like `chmod -R a=rwX`).
    Chmod { path: String, mode: u32, recursive: bool },
}

pub enum Reply {
    Chmoded { path: String, mode: u32, recursive: bool },
    Created { path: String },
    Deleted { path: String },
    Renamed { from: String, to: String },
    Copied { to: String },
    Uploaded { path: String },
}

fn quote(value: &str) -> String { format!("'{}'", value.replace('\'', "'\"'\"'")) }

fn validate_path(path: &str) -> Result<()> {
    if !path.starts_with('/') || path.len() > 4096 || path.chars().any(char::is_control) {
        bail!("Gunakan path absolut Linux tanpa karakter kontrol.");
    }
    Ok(())
}

impl Request {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::CreateFile { path } | Self::CreateFolder { path } => validate_path(path)?,
            Self::Delete { path, .. } => validate_path(path)?,
            Self::Rename { from, to } | Self::Copy { from, to, .. } => { validate_path(from)?; validate_path(to)?; }
            Self::Upload { path, data } => {
                validate_path(path)?;
                if data.len() > UPLOAD_MAX_BYTES { bail!("File melebihi batas unggah {} MiB.", UPLOAD_MAX_BYTES / 1024 / 1024); }
            }
            Self::Chmod { path, mode, .. } => {
                validate_path(path)?;
                if *mode > 0o777 { bail!("Mode izin tidak valid."); }
            }
        }
        Ok(())
    }
}

pub async fn execute(channel: &mut Channel<client::Msg>, request: Request) -> Result<Reply> {
    request.validate()?;
    let (action, args, payload, reply) = match request {
        Request::CreateFile { path } => ("touch", vec![path.clone()], None, Reply::Created { path }),
        Request::CreateFolder { path } => ("mkdir", vec![path.clone()], None, Reply::Created { path }),
        Request::Delete { path, recursive } => {
            let flag = if recursive { "1" } else { "0" }.to_owned();
            ("rm", vec![path.clone(), flag], None, Reply::Deleted { path })
        }
        Request::Rename { from, to } => ("rename", vec![from.clone(), to.clone()], None, Reply::Renamed { from, to }),
        Request::Copy { from, to, recursive } => {
            let flag = if recursive { "1" } else { "0" }.to_owned();
            ("copy", vec![from.clone(), to.clone(), flag], None, Reply::Copied { to })
        }
        Request::Upload { path, data } => ("upload", vec![path.clone()], Some(data), Reply::Uploaded { path }),
        Request::Chmod { path, mode, recursive } => {
            let flag = if recursive { "1" } else { "0" }.to_owned();
            ("chmod", vec![path.clone(), format!("{mode:o}"), flag], None, Reply::Chmoded { path, mode, recursive })
        }
    };
    let mut command = format!("python3 -c {} {}", quote(SCRIPT), quote(action));
    for arg in &args {
        command.push(' ');
        command.push_str(&quote(arg));
    }
    channel.exec(true, command).await?;
    // Only `upload` reads stdin; the script never touches it for the other
    // actions, so sending EOF right away for those is harmless.
    if let Some(data) = payload { channel.data(&data[..]).await?; }
    channel.eof().await?;
    let output = crate::remote_file::receive(channel).await?;
    let output = String::from_utf8(output).context("Respons operasi file tidak valid")?;
    if output.trim() != "AWAN_FS_V1 OK" { bail!("Respons operasi file tidak dikenali"); }
    Ok(reply)
}

// Python 3 must already exist on the Linux server (same requirement as
// remote_file.rs/remote_directory.rs). Every action fails loudly instead of
// silently overwriting or partially applying: create/upload refuse to
// replace an existing path, rename/copy refuse an existing destination.
const SCRIPT: &str = r#"import os, sys, shutil, stat

LIMIT = 25 * 1024 * 1024
args = sys.argv[1:]

def fail(message):
    print('[Awan] ' + message, file=sys.stderr)
    sys.exit(1)

if not args:
    fail('Perintah operasi file tidak lengkap.')
action, rest = args[0], args[1:]

try:
    if action == 'touch':
        (path,) = rest
        try:
            os.close(os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o644))
        except FileExistsError:
            fail('Sudah ada file atau folder dengan nama itu.')
        except FileNotFoundError:
            fail('Folder induk tidak ditemukan.')
        except PermissionError:
            fail('Tidak ada izin membuat file di folder ini.')
    elif action == 'mkdir':
        (path,) = rest
        try:
            os.makedirs(path, exist_ok=False)
        except FileExistsError:
            fail('Sudah ada file atau folder dengan nama itu.')
        except FileNotFoundError:
            fail('Folder induk tidak ditemukan.')
        except PermissionError:
            fail('Tidak ada izin membuat folder di sini.')
    elif action == 'rm':
        path, recursive = rest
        try:
            info = os.lstat(path)
            if recursive == '1' and stat.S_ISDIR(info.st_mode):
                shutil.rmtree(path)
            else:
                os.remove(path)
        except FileNotFoundError:
            fail('File atau folder tidak ditemukan.')
        except IsADirectoryError:
            fail('Ini folder; hapus sebagai folder, bukan file.')
        except PermissionError:
            fail('Tidak ada izin menghapus.')
    elif action == 'rename':
        source, destination = rest
        if os.path.lexists(destination):
            fail('Tujuan sudah ada.')
        try:
            os.rename(source, destination)
        except FileNotFoundError:
            fail('Sumber tidak ditemukan.')
        except PermissionError:
            fail('Tidak ada izin mengubah nama atau memindahkan.')
    elif action == 'copy':
        source, destination, recursive = rest
        if os.path.lexists(destination):
            fail('Tujuan sudah ada.')
        try:
            info = os.lstat(source)
            if recursive == '1' and stat.S_ISDIR(info.st_mode):
                shutil.copytree(source, destination, symlinks=True)
            else:
                shutil.copy2(source, destination, follow_symlinks=False)
        except FileNotFoundError:
            fail('Sumber tidak ditemukan.')
        except PermissionError:
            fail('Tidak ada izin menyalin.')
    elif action == 'upload':
        (path,) = rest
        if os.path.lexists(path):
            fail('File sudah ada di tujuan.')
        data = sys.stdin.buffer.read(LIMIT + 1)
        if len(data) > LIMIT:
            fail('File melebihi batas ukuran unggah.')
        try:
            fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o644)
            with os.fdopen(fd, 'wb') as target:
                target.write(data)
                target.flush()
                os.fsync(target.fileno())
        except FileExistsError:
            fail('File sudah ada di tujuan.')
        except FileNotFoundError:
            fail('Folder tujuan tidak ditemukan.')
        except PermissionError:
            fail('Tidak ada izin menulis di folder tujuan.')
    elif action == 'chmod':
        path, mode, recursive = rest
        mode = int(mode, 8) & 0o777
        # setuid/setgid/sticky bits are kept as they were; only rwx changes.
        def apply(target, bits):
            os.chmod(target, (os.stat(target).st_mode & 0o7000) | bits)
        try:
            apply(path, mode)
            if recursive == '1' and os.path.isdir(path) and not os.path.islink(path):
                failed = 0
                for root, dirs, files in os.walk(path):
                    for name in dirs:
                        full = os.path.join(root, name)
                        if os.path.islink(full):
                            continue
                        try:
                            apply(full, mode)
                        except PermissionError:
                            failed += 1
                    for name in files:
                        full = os.path.join(root, name)
                        if os.path.islink(full):
                            continue
                        try:
                            executable = os.stat(full).st_mode & 0o111
                            apply(full, mode if executable else mode & ~0o111)
                        except PermissionError:
                            failed += 1
                if failed:
                    fail('Izin diubah sebagian; %d item tidak dapat diubah (bukan milik akun ini).' % failed)
        except FileNotFoundError:
            fail('File atau folder tidak ditemukan.')
        except PermissionError:
            fail('Tidak ada izin mengubah izin akses (hanya pemilik atau root yang bisa).')
    else:
        fail('Perintah operasi file tidak dikenali.')
except ValueError as error:
    fail(str(error))
print('AWAN_FS_V1 OK')
"#;
