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
    Delete { path: String, recursive: bool, sudo: bool },
    Rename { from: String, to: String },
    Copy { from: String, to: String, recursive: bool },
    Upload { path: String, data: Vec<u8> },
    /// `mode` holds the 9 rwx permission bits. With `recursive` on a folder,
    /// subfolders get `mode` and files get it without execute unless they
    /// were already executable (like `chmod -R a=rwX`).
    Chmod { path: String, mode: u32, recursive: bool },
    Stat { path: String },
}

#[derive(Clone, Debug, Default)]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub blocks: u64,
    pub mode_octal: String,
    pub permissions: String,
    pub owner: String,
    pub group: String,
    pub modified: String,
    pub accessed: String,
    pub changed: String,
    pub created: String,
    pub target: Option<String>,
    pub target_valid: bool,
    pub item_count: Option<String>,
    pub inode: u64,
    pub links: u64,
    pub device: u64,
}

pub enum Reply {
    Chmoded { path: String, mode: u32, recursive: bool },
    Created { path: String },
    Deleted { path: String },
    Renamed { from: String, to: String },
    Copied { to: String },
    Uploaded { path: String },
    Stat(FileInfo),
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
            Self::CreateFile { path } | Self::CreateFolder { path } | Self::Stat { path } => validate_path(path)?,
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
    if let Request::Delete { path, recursive, sudo: true } = &request {
        let flag = if *recursive { "-rf" } else { "-f" };
        let command = format!("sudo -n rm {flag} -- {} && echo 'AWAN_FS_V1 OK'", quote(path));
        channel.exec(true, command).await?;
        channel.eof().await?;
        let output = crate::remote_file::receive(channel).await?;
        let output = String::from_utf8(output).context("Respons operasi file tidak valid")?;
        if output.trim() != "AWAN_FS_V1 OK" { bail!("Respons operasi file tidak dikenali"); }
        return Ok(Reply::Deleted { path: path.clone() });
    }
    if let Request::Stat { path } = &request {
        let command = format!("python3 -c {} stat {}", quote(SCRIPT), quote(path));
        channel.exec(true, command).await?;
        channel.eof().await?;
        let output = crate::remote_file::receive(channel).await?;
        let text = String::from_utf8(output).context("Respons info file tidak valid")?;
        let info = parse_file_info(&text)?;
        return Ok(Reply::Stat(info));
    }
    let (action, args, payload, reply) = match request {
        Request::CreateFile { path } => ("touch", vec![path.clone()], None, Reply::Created { path }),
        Request::CreateFolder { path } => ("mkdir", vec![path.clone()], None, Reply::Created { path }),
        Request::Delete { path, recursive, sudo: _ } => {
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
        Request::Stat { .. } => unreachable!("handled above"),
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

fn unhex(value: &str) -> Result<String> {
    if value.len() % 2 != 0 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("Format heksadesimal tidak valid");
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i+2], 16))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(String::from_utf8(bytes)?)
}

fn parse_file_info(text: &str) -> Result<FileInfo> {
    let line = text.lines().find(|l| l.starts_with("AWAN_STAT_V1\t"))
        .context("Respons info file tidak ditemukan")?;
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.len() < 20 {
        bail!("Respons info file tidak lengkap");
    }
    let path = unhex(parts[1])?;
    let name = unhex(parts[2])?;
    let kind = unhex(parts[3])?;
    let size: u64 = parts[4].parse().unwrap_or(0);
    let blocks: u64 = parts[5].parse().unwrap_or(0);
    let mode_octal = parts[6].to_string();
    let permissions = parts[7].to_string();
    let owner = unhex(parts[8])?;
    let group = unhex(parts[9])?;
    let modified = unhex(parts[10])?;
    let accessed = unhex(parts[11])?;
    let changed = unhex(parts[12])?;
    let created = unhex(parts[13])?;
    let target_raw = unhex(parts[14])?;
    let target = if target_raw.is_empty() { None } else { Some(target_raw) };
    let target_valid = parts[15] == "1";
    let item_count = if parts[16] == "-" { None } else { Some(parts[16].to_string()) };
    let inode: u64 = parts[17].parse().unwrap_or(0);
    let links: u64 = parts[18].parse().unwrap_or(0);
    let device: u64 = parts[19].parse().unwrap_or(0);

    Ok(FileInfo {
        path,
        name,
        kind,
        size,
        blocks,
        mode_octal,
        permissions,
        owner,
        group,
        modified,
        accessed,
        changed,
        created,
        target,
        target_valid,
        item_count,
        inode,
        links,
        device,
    })
}

// Python 3 must already exist on the Linux server (same requirement as
// remote_file.rs/remote_directory.rs). Every action fails loudly instead of
// silently overwriting or partially applying: create/upload refuse to
// replace an existing path, rename/copy refuse an existing destination.
const SCRIPT: &str = r#"import os, sys, shutil, stat, datetime

LIMIT = 25 * 1024 * 1024
args = sys.argv[1:]

def hextext(value):
    return value.encode('utf-8').hex()

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
            owner = 'root'
            current_user = 'user'
            try:
                import pwd
                info = os.lstat(path)
                try:
                    owner = pwd.getpwuid(info.st_uid).pw_name
                except Exception:
                    owner = str(info.st_uid)
            except Exception:
                try:
                    import pwd
                    parent = os.path.dirname(path) or '.'
                    owner = pwd.getpwuid(os.lstat(parent).st_uid).pw_name
                except Exception:
                    pass
            try:
                import pwd
                current_user = pwd.getpwuid(os.getuid()).pw_name
            except Exception:
                pass
            fail('PERMISSION_DENIED\t' + owner + '\t' + current_user + '\t' + path + '\t' + recursive)
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
    elif action == 'stat':
        (path,) = rest
        try:
            info = os.lstat(path)
            mode = info.st_mode
            filemode = stat.filemode(mode)
            octal = f"{mode & 0o7777:04o}"

            if stat.S_ISLNK(mode):
                kind = "Symbolic Link"
            elif stat.S_ISDIR(mode):
                kind = "Folder"
            elif stat.S_ISREG(mode):
                kind = "File"
            elif stat.S_ISCHR(mode):
                kind = "Character Device"
            elif stat.S_ISBLK(mode):
                kind = "Block Device"
            elif stat.S_ISFIFO(mode):
                kind = "FIFO / Pipe"
            elif stat.S_ISSOCK(mode):
                kind = "Socket"
            else:
                kind = "Unknown"

            try:
                import pwd
                pw = pwd.getpwuid(info.st_uid)
                owner = f"{pw.pw_name} (UID: {info.st_uid})"
            except Exception:
                owner = f"UID: {info.st_uid}"

            try:
                import grp
                gr = grp.getgrgid(info.st_gid)
                group = f"{gr.gr_name} (GID: {info.st_gid})"
            except Exception:
                group = f"GID: {info.st_gid}"

            size = str(info.st_size)
            blocks = str(getattr(info, 'st_blocks', 0))

            def fmt_time(ts):
                try:
                    return datetime.datetime.fromtimestamp(ts, datetime.timezone.utc).strftime('%Y-%m-%d %H:%M:%S UTC')
                except Exception:
                    return '-'

            mtime = fmt_time(info.st_mtime)
            atime = fmt_time(info.st_atime)
            ctime = fmt_time(info.st_ctime)

            btime = '-'
            try:
                if hasattr(info, 'st_birthtime') and info.st_birthtime:
                    btime = fmt_time(info.st_birthtime)
                else:
                    import subprocess
                    res = subprocess.run(['stat', '-c', '%w', path], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=1)
                    out = res.stdout.strip()
                    if out and out != '-':
                        btime = out
            except Exception:
                pass

            target = ''
            target_valid = '0'
            if stat.S_ISLNK(mode):
                try:
                    target = os.readlink(path)
                    target_valid = '1' if os.path.exists(path) else '0'
                except Exception:
                    pass

            item_count = '-'
            if stat.S_ISDIR(mode):
                try:
                    item_count = str(len(os.listdir(path)))
                except Exception:
                    item_count = '?'

            ino = str(info.st_ino)
            nlink = str(info.st_nlink)
            dev = str(info.st_dev)
            name = os.path.basename(path.rstrip('/')) or path

            fields = [
                hextext(path),
                hextext(name),
                hextext(kind),
                size,
                blocks,
                octal,
                filemode,
                hextext(owner),
                hextext(group),
                hextext(mtime),
                hextext(atime),
                hextext(ctime),
                hextext(btime),
                hextext(target),
                target_valid,
                item_count,
                ino,
                nlink,
                dev,
            ]
            sys.stdout.write('AWAN_STAT_V1\t' + '\t'.join(fields) + '\n')
            sys.exit(0)
        except FileNotFoundError:
            fail('File atau folder tidak ditemukan.')
        except PermissionError:
            fail('Tidak ada izin membaca status file.')
    else:
        fail('Perintah operasi file tidak dikenali.')
except ValueError as error:
    fail(str(error))
print('AWAN_FS_V1 OK')
"#;
