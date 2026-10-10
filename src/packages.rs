//! Package inventory and a fixed catalog. All mutations run on the remote host.
use anyhow::{Result, bail};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Software { Nginx, Docker, Compose, Git, Node, Redis, Postgres }

impl Software {
    pub const ALL: [Self; 7] = [Self::Nginx, Self::Docker, Self::Compose, Self::Git, Self::Node, Self::Redis, Self::Postgres];

    pub fn name(self) -> &'static str {
        match self {
            Self::Nginx => "Nginx", Self::Docker => "Docker Engine", Self::Compose => "Docker Compose",
            Self::Git => "Git", Self::Node => "Node.js", Self::Redis => "Redis", Self::Postgres => "PostgreSQL",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Nginx => "Web server dan reverse proxy", Self::Docker => "Engine untuk menjalankan container",
            Self::Compose => "Pengelolaan aplikasi multi-container", Self::Git => "Version control",
            Self::Node => "JavaScript runtime", Self::Redis => "Cache dan data store", Self::Postgres => "Database PostgreSQL",
        }
    }

    pub fn id(self) -> &'static str {
        match self { Self::Nginx => "nginx", Self::Docker => "docker", Self::Compose => "compose", Self::Git => "git", Self::Node => "node", Self::Redis => "redis", Self::Postgres => "postgres" }
    }

    pub fn candidates(self, manager: &str) -> &'static str {
        match (self, manager) {
            (Self::Docker, "APT") => "docker.io docker-ce",
            (Self::Docker, _) => "moby-engine docker-ce",
            (Self::Compose, "APT") => "docker-compose-v2 docker-compose-plugin docker-compose",
            (Self::Compose, _) => "docker-compose-plugin docker-compose",
            (Self::Redis, "APT") => "redis-server",
            (Self::Redis, _) => "redis",
            (Self::Postgres, "APT") => "postgresql",
            (Self::Postgres, _) => "postgresql-server",
            (Self::Nginx, _) => "nginx", (Self::Git, _) => "git", (Self::Node, _) => "nodejs",
        }
    }

    pub fn installed<'a>(self, inventory: &'a Inventory) -> Vec<&'a Package> {
        inventory.packages.iter().filter(|package| {
            let name = package.name.split(':').next().unwrap_or(&package.name);
            self.candidates(&inventory.manager).split_whitespace().any(|candidate| candidate == name)
                || (self == Self::Postgres && name.strip_prefix("postgresql-").is_some_and(|suffix| suffix.chars().all(|c| c.is_ascii_digit()) && !suffix.is_empty()))
        }).collect()
    }
}

#[derive(Clone)]
pub enum Maintenance {
    SystemUpdate,
    Install(Software),
    Remove(String),
    Server(crate::live::Action),
    /// `git clone <url>` into `<parent>/<dir>` on the server.
    GitClone { url: String, parent: String, dir: String },
}

impl Maintenance {
    pub fn title(&self) -> String {
        match self {
            Self::SystemUpdate => "Update System".into(),
            Self::Install(app) => format!("Install {}", app.name()),
            Self::Remove(package) => format!("Uninstall {package}"),
            Self::Server(action) => action.title(),
            // The URL is left out on purpose: it may embed a token.
            Self::GitClone { dir, .. } => format!("Git clone {dir}"),
        }
    }

    pub fn command(&self) -> Result<String> {
        match self {
            Self::SystemUpdate => Ok(crate::system_update::command()),
            Self::Server(action) => action.command(),
            Self::GitClone { url, parent, dir } => {
                if !valid_git_url(url) { bail!("URL repository tidak valid"); }
                if !parent.starts_with('/') || parent.len() > 4096 || parent.chars().any(char::is_control) { bail!("Folder tujuan tidak valid"); }
                if dir.is_empty() || dir == "." || dir == ".." || dir.contains('/') || dir.chars().any(char::is_control) { bail!("Nama folder tidak valid"); }
                // Prompts are disabled: keystrokes are not routed to this
                // operation's terminal, so a credential or host-key prompt
                // would otherwise hang forever instead of failing clearly.
                let script = format!(r#"set -e
export GIT_TERMINAL_PROMPT=0
export GIT_SSH_COMMAND='ssh -o BatchMode=yes'
if ! command -v git >/dev/null 2>&1; then
    printf '[Awan] Git belum terpasang di server. Pasang melalui menu Applications.\n' >&2
    exit 2
fi
cd -- {parent}
if [ -e {dir} ]; then
    printf '[Awan] %s sudah ada di folder tujuan; pilih nama folder lain.\n' {dir} >&2
    exit 3
fi
git clone --progress -- {url} {dir}
printf '[Awan] Clone selesai.\n'
"#, parent = single_quote(parent), dir = single_quote(dir), url = single_quote(url));
                Ok(shell_command(&script))
            }
            Self::Install(app) => {
                let script = format!(r#"{SETUP}
if [ "$manager" = apt ]; then
    candidates='{apt}'
else
    candidates='{dnf}'
fi
for package in $candidates; do
    if installed "$package"; then
        printf '[Awan] Paket %s sudah terpasang.\n' "$package"
        exit 0
    fi
done
if [ "$manager" = apt ]; then
    as_root apt-get -o APT::Update::Error-Mode=any update
else
    as_root dnf makecache --refresh
fi
for package in $candidates; do
    if [ "$manager" = apt ]; then
        if apt-cache policy "$package" | awk '/Candidate:/ {{ if ($2 != "(none)") found=1 }} END {{ exit !found }}'; then
            printf '[Awan] Menginstal %s dari repository server.\n' "$package"
            as_root apt-get install -y "$package"
            exit 0
        fi
    elif dnf -q list --available "$package" >/dev/null 2>&1; then
        printf '[Awan] Menginstal %s dari repository server.\n' "$package"
        as_root dnf install -y "$package"
        exit 0
    fi
done
printf '[Awan] Paket tidak tersedia di repository yang dikonfigurasi: %s\n' "$candidates" >&2
exit 2
"#, apt = app.candidates("APT"), dnf = app.candidates("DNF"));
                Ok(shell_command(&script))
            }
            Self::Remove(package) => {
                if !valid_package_name(package) { bail!("Nama paket tidak valid"); }
                let script = format!(r#"{SETUP}
package='{package}'
if ! installed "$package"; then
    printf '[Awan] Paket %s tidak lagi terpasang. Muat ulang daftar.\n' "$package" >&2
    exit 3
fi
printf '[Awan] Tinjau rencana penghapusan dari package manager sebelum menjawab Y.\n'
if [ "$manager" = apt ]; then
    as_root apt-get -o APT::Get::AutomaticRemove=false -o APT::Get::Assume-Yes=false remove "$package"
else
    as_root dnf --setopt=clean_requirements_on_remove=False --setopt=assumeyes=False remove "$package"
fi
if installed "$package"; then
    printf '[Awan] Paket masih terpasang; uninstall dibatalkan atau belum selesai.\n' >&2
    exit 4
fi
"#);
                Ok(shell_command(&script))
            }
        }
    }
}

const SETUP: &str = r#"set -e
export LC_ALL=C
as_root() {
    if [ "$(id -u)" = 0 ]; then "$@"; else sudo -- "$@"; fi
}
if command -v apt-get >/dev/null 2>&1 && command -v dpkg-query >/dev/null 2>&1; then
    manager=apt
elif command -v dnf >/dev/null 2>&1 && command -v rpm >/dev/null 2>&1; then
    manager=dnf
else
    printf '[Awan] Diperlukan APT/dpkg atau DNF/RPM.\n' >&2
    exit 2
fi
installed() {
    if [ "$manager" = apt ]; then
        status=$(dpkg-query -W -f='${db:Status-Status}' "$1" 2>/dev/null) || return 1
        [ "$status" = installed ]
    else
        rpm -q -- "$1" >/dev/null 2>&1
    fi
}
"#;

pub fn shell_command(script: &str) -> String {
    format!("/bin/sh -c '{}'", script.replace('\'', "'\"'\"'"))
}

fn single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Accepts the URL forms `git clone` normally takes over the network —
/// https/http/ssh/git schemes and scp-like `user@host:path` — and rejects a
/// leading `-` so the value can never be parsed as a git option.
pub fn valid_git_url(url: &str) -> bool {
    if url.is_empty() || url.len() > 2048 || url.starts_with('-') || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    ["https://", "http://", "ssh://", "git://"].iter().any(|scheme| url.starts_with(scheme))
        || url.split_once(':').is_some_and(|(host, path)| host.contains('@') && !host.contains('/') && !path.is_empty())
}

/// The SSH form of a GitHub/GitLab HTTPS repository URL, e.g.
/// `https://github.com/owner/repo` → `git@github.com:owner/repo.git`. HTTPS
/// clones authenticate with a username/token, not the server's SSH key.
pub fn https_to_ssh_url(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("https://").or_else(|| url.trim().strip_prefix("http://"))?;
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let (host, path) = rest.split_once('/')?;
    if host != "github.com" && host != "gitlab.com" { return None; }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let segments: Vec<&str> = path.split('/').collect();
    let valid = segments.len() >= 2 && segments.iter().all(|segment| {
        !segment.is_empty() && segment.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    });
    valid.then(|| format!("git@{host}:{path}.git"))
}

/// The folder name `git clone` would pick on its own: the URL's last path
/// segment without a trailing `.git`.
pub fn git_repo_name(url: &str) -> Option<String> {
    let trimmed = url.trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty() && name != "." && name != "..").then(|| name.to_owned())
}

pub fn inventory_command() -> String {
    shell_command(r#"set -e
export LC_ALL=C
if command -v apt-get >/dev/null 2>&1 && command -v dpkg-query >/dev/null 2>&1; then
    packages=$(dpkg-query -W -f='${binary:Package}\t${Version}\t${db:Status-Status}\n')
    printf 'AWAN_PACKAGES_V1\tAPT\n'
    printf '%s\n' "$packages" | awk -F '\t' '$3 == "installed" { printf "P\t%s\t%s\n", $1, $2 }'
elif command -v dnf >/dev/null 2>&1 && command -v rpm >/dev/null 2>&1; then
    packages=$(rpm -qa --qf '%{NAME}\t%{VERSION}-%{RELEASE}.%{ARCH}\n')
    printf 'AWAN_PACKAGES_V1\tDNF\n'
    printf '%s\n' "$packages" | awk -F '\t' 'NF == 2 { printf "P\t%s\t%s\n", $1, $2 }'
else
    printf '[Awan] Pemeriksaan memerlukan APT/dpkg atau DNF/RPM.\n' >&2
    exit 2
fi
for entry in nginx:nginx docker:docker git:git node:node redis:redis-server postgres:postgres; do
    id=${entry%%:*}
    executable=${entry#*:}
    if command -v "$executable" >/dev/null 2>&1; then printf 'B\t%s\n' "$id"; fi
done
if command -v docker-compose >/dev/null 2>&1; then printf 'B\tcompose\n'; fi
printf 'AWAN_PACKAGES_END\n'
"#)
}

pub struct Package { pub name: String, pub version: String }

pub struct Inventory {
    pub manager: String,
    pub packages: Vec<Package>,
    pub binaries: Vec<String>,
}

pub fn valid_package_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 200 && name.as_bytes()[0].is_ascii_alphanumeric()
        && name.bytes().all(|c| c.is_ascii_alphanumeric() || b"+._:-".contains(&c))
}

impl Inventory {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let output = std::str::from_utf8(bytes)?;
        let mut result = Self { manager: String::new(), packages: Vec::new(), binaries: Vec::new() };
        let mut finished = false;
        for line in output.lines() {
            let columns: Vec<_> = line.split('\t').collect();
            match columns.as_slice() {
                ["AWAN_PACKAGES_V1", manager @ ("APT" | "DNF")] => result.manager = (*manager).into(),
                ["P", name, version] if !result.manager.is_empty() => {
                    if !valid_package_name(name) || version.len() > 256 || version.chars().any(char::is_control) {
                        bail!("Format paket dari server tidak valid");
                    }
                    if result.packages.len() >= 20_000 { bail!("Daftar melebihi batas 20.000 paket; tidak ditampilkan sebagai inventaris lengkap"); }
                    result.packages.push(Package { name: (*name).into(), version: (*version).into() });
                }
                ["B", id] if Software::ALL.iter().any(|app| app.id() == *id) => result.binaries.push((*id).into()),
                ["AWAN_PACKAGES_END"] => { finished = true; break; }
                _ => {},
            }
        }
        if result.manager.is_empty() || !finished { bail!("Inventaris server tidak lengkap"); }
        result.packages.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
        Ok(result)
    }
}
