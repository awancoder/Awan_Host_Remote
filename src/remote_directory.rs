use anyhow::{Context, Result, bail};
use russh::{Channel, client};

pub const MAX_ENTRIES: usize = 2000;

#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub kind: String,
    pub size: Option<u64>,
    pub modified: String,
    pub permissions: String,
    pub target: String,
    /// A folder that is a Git repository root (contains `.git`).
    pub git: bool,
}
impl Entry {
    pub fn directory(&self) -> bool { self.kind == "dir" || self.kind == "dirlink" }
    pub fn openable(&self) -> bool {
        !self.name.chars().any(char::is_control) && (self.directory() || (self.kind == "file" && self.size.is_some_and(|n| n <= crate::remote_file::MAX_BYTES as u64)))
    }
}

#[derive(Clone)]
pub struct Directory {
    pub path: String,
    pub home: String,
    pub entries: Vec<Entry>,
    pub truncated: bool,
    pub skipped: usize,
    pub query: String,
    /// Echoed back from the request; the app tracks its own show-hidden
    /// toggle instead of reading this back.
    #[allow(dead_code)]
    pub show_hidden: bool,
}

pub fn validate(path: &str, query: &str) -> Result<()> {
    if !(path.starts_with('/') || path == "~" || path.starts_with("~/")) || path.len() > 4096 || path.chars().any(char::is_control) {
        bail!("Gunakan path folder absolut Linux atau ~ untuk folder Home.");
    }
    if query.len() > 256 || query.chars().any(char::is_control) { bail!("Pencarian maksimal 256 byte tanpa karakter kontrol."); }
    Ok(())
}

fn unhex(value: &str) -> Result<String> {
    if value.len() % 2 != 0 || !value.bytes().all(|b| b.is_ascii_hexdigit()) { bail!("Format daftar folder tidak valid."); }
    let bytes = (0..value.len()).step_by(2).map(|i| u8::from_str_radix(&value[i..i+2], 16)).collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(String::from_utf8(bytes)?)
}

pub async fn read(channel: &mut Channel<client::Msg>, path: String, query: String, show_hidden: bool) -> Result<Directory> {
    validate(&path, &query)?;
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\"'\"'"));
    channel.exec(true, format!("python3 -c {} {} {} {}", quote(include_str!("remote_directory.py")), quote(&path), quote(&query), if show_hidden { "1" } else { "0" })).await?;
    channel.eof().await?;
    let output = crate::remote_file::receive(channel).await?;
    let mut lines = std::str::from_utf8(&output)?.lines();
    let header: Vec<_> = lines.next().context("Daftar folder kosong")?.split('\t').collect();
    if header.len() != 5 || header[0] != "AWAN_DIR_V1" || !["0", "1"].contains(&header[3]) { bail!("Respons folder tidak valid."); }
    let mut result = Directory {
        path: unhex(header[1])?, home: unhex(header[2])?, truncated: header[3] == "1",
        skipped: header[4].parse()?, entries: Vec::new(), query, show_hidden,
    };
    validate(&result.path, "")?;
    validate(&result.home, "")?;
    let mut complete = false;
    for line in lines {
        if line == "END" { complete = true; break; }
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 7 || result.entries.len() >= MAX_ENTRIES || !["dir", "dirlink", "file", "link", "other", "unknown"].contains(&fields[0]) || !["0", "1"].contains(&fields[6]) {
            bail!("Entri folder tidak valid atau terlalu banyak.");
        }
        let name = unhex(fields[4])?;
        if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') { bail!("Nama entri folder tidak valid."); }
        result.entries.push(Entry {
            name, kind: fields[0].into(), size: if fields[1] == "-" { None } else { Some(fields[1].parse()?) },
            modified: unhex(fields[2])?, permissions: unhex(fields[3])?, target: unhex(fields[5])?,
            git: fields[6] == "1",
        });
    }
    if !complete { bail!("Daftar folder tidak lengkap; muat ulang."); }
    Ok(result)
}
