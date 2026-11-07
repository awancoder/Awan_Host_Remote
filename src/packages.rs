//! Package inventory and a fixed catalog. All mutations run on the remote host.
use anyhow::{Result, bail};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Software { Nginx, Certbot, Docker, Compose, Git, Node, Redis, Postgres }

impl Software {
    pub const ALL: [Self; 8] = [Self::Nginx, Self::Certbot, Self::Docker, Self::Compose, Self::Git, Self::Node, Self::Redis, Self::Postgres];

    pub fn name(self) -> &'static str {
        match self {
            Self::Nginx => "Nginx", Self::Certbot => "Certbot", Self::Docker => "Docker Engine", Self::Compose => "Docker Compose",
            Self::Git => "Git", Self::Node => "Node.js", Self::Redis => "Redis", Self::Postgres => "PostgreSQL",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Nginx => "Web server dan reverse proxy", Self::Certbot => "Sertifikat HTTPS Let's Encrypt untuk Nginx",
            Self::Docker => "Engine untuk menjalankan container",
            Self::Compose => "Pengelolaan aplikasi multi-container", Self::Git => "Version control",
            Self::Node => "JavaScript runtime", Self::Redis => "Cache dan data store", Self::Postgres => "Database PostgreSQL",
        }
    }

    pub fn id(self) -> &'static str {
        match self { Self::Nginx => "nginx", Self::Certbot => "certbot", Self::Docker => "docker", Self::Compose => "compose", Self::Git => "git", Self::Node => "node", Self::Redis => "redis", Self::Postgres => "postgres" }
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
            // The Nginx plugin depends on certbot itself, so one package
            // installs both.
            (Self::Certbot, _) => "python3-certbot-nginx",
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
    /// Nginx reverse proxy `domain` → `127.0.0.1:port`, optionally with a
    /// Let's Encrypt certificate from Certbot. `email` may be empty.
    NginxSite { domain: String, port: u16, email: String, https: bool },
    /// Installs sysstat if missing, switches its 10-minute collection on and
    /// keeps at least 31 days, for the Summary page's long history ranges.
    Sysstat,
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
            Self::NginxSite { domain, .. } => format!("Setup Nginx {domain}"),
            Self::Sysstat => "Setup sysstat".into(),
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
            Self::NginxSite { domain, port, email, https } => {
                if !valid_domain(domain) { bail!("Domain tidak valid. Contoh: mail.example.com"); }
                if *port == 0 { bail!("Port aplikasi harus berupa angka 1 sampai 65535"); }
                if !email.is_empty() && !valid_email(email) { bail!("Email tidak valid"); }
                let account = if email.is_empty() { "--register-unsafely-without-email".to_owned() } else { format!("-m {}", single_quote(email)) };
                // Never touches a config this app did not write (first-line
                // marker), so hand-made sites such as an existing API proxy
                // stay as they are. Rerunning on its own config is allowed so
                // a failed Certbot step can be retried after fixing DNS.
                let script = format!(r#"exec </dev/null
set -e
as_root() {{
    if [ "$(id -u)" = 0 ]; then "$@"; else sudo -- "$@"; fi
}}
domain='{domain}'
https={https}
marker='# Managed by Awan Host Manager'
tmp=
backup=
trap 'rm -f "$tmp" "$backup"' EXIT
if ! command -v nginx >/dev/null 2>&1; then
    printf '[Awan] Nginx belum terpasang. Pasang melalui katalog Applications.\n' >&2
    exit 2
fi
if [ "$https" = 1 ] && ! command -v certbot >/dev/null 2>&1; then
    printf '[Awan] Certbot belum terpasang. Pasang Certbot dari katalog Applications, lalu ulangi.\n' >&2
    exit 2
fi
if [ -d /etc/nginx/sites-available ] && [ -d /etc/nginx/sites-enabled ]; then
    conf="/etc/nginx/sites-available/$domain"
    link="/etc/nginx/sites-enabled/$domain"
else
    conf="/etc/nginx/conf.d/$domain.conf"
    link=
fi
if [ -e "$conf" ]; then
    if [ "$(as_root head -n 1 "$conf")" != "$marker" ]; then
        printf '[Awan] %s sudah ada dan tidak dibuat oleh aplikasi ini; tidak ditimpa.\n' "$conf" >&2
        exit 3
    fi
    if as_root grep -q ssl_certificate "$conf"; then
        printf '[Awan] %s sudah memakai HTTPS. Tidak ada yang diubah.\n' "$domain"
        exit 0
    fi
    printf '[Awan] Memperbarui %s (belum memakai HTTPS).\n' "$conf"
else
    existing=$(as_root grep -rlsE '^[^#]*server_name[^;]*[[:space:]]{domain_re}([[:space:]]|;)' /etc/nginx/ || true)
    if [ -n "$existing" ]; then
        printf '[Awan] Domain %s sudah dipakai konfigurasi Nginx lain; tidak diubah:\n%s\n' "$domain" "$existing" >&2
        exit 3
    fi
fi
if command -v curl >/dev/null 2>&1 && ! curl -s -o /dev/null --max-time 5 'http://127.0.0.1:{port}/'; then
    printf '[Awan] Peringatan: http://127.0.0.1:{port} tidak merespons. Pastikan aplikasi berjalan di port ini.\n'
fi
tmp=$(mktemp)
cat > "$tmp" <<'AWAN_NGINX'
# Managed by Awan Host Manager
server {{
    listen 80;
    listen [::]:80;
    server_name {domain};

    client_max_body_size 100m;

    location / {{
        proxy_pass http://127.0.0.1:{port};
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 300s;
    }}
}}
AWAN_NGINX
if [ ! -f /proc/net/if_inet6 ]; then sed -i '/listen \[::\]:80;/d' "$tmp"; fi
if [ -e "$conf" ]; then
    backup=$(mktemp)
    as_root cat "$conf" > "$backup"
fi
as_root install -m 644 "$tmp" "$conf"
created_link=0
if [ -n "$link" ] && [ ! -e "$link" ]; then
    as_root ln -s "$conf" "$link"
    created_link=1
fi
if ! as_root nginx -t; then
    printf '[Awan] Konfigurasi Nginx tidak valid; perubahan dibatalkan.\n' >&2
    if [ -n "$backup" ]; then as_root install -m 644 "$backup" "$conf"; else as_root rm -f "$conf"; fi
    if [ "$created_link" = 1 ]; then as_root rm -f "$link"; fi
    exit 4
fi
if command -v systemctl >/dev/null 2>&1; then
    as_root systemctl reload-or-restart nginx
else
    as_root nginx -s reload
fi
printf '[Awan] HTTP aktif: http://%s -> 127.0.0.1:{port}\n' "$domain"
if [ "$https" = 1 ]; then
    printf "[Awan] Meminta sertifikat Let's Encrypt untuk %s...\n" "$domain"
    if ! as_root certbot --nginx --non-interactive --agree-tos {account} --redirect -d "$domain"; then
        printf '[Awan] Certbot gagal; konfigurasi HTTP tetap aktif. Pastikan DNS domain mengarah ke server ini dan port 80/443 terbuka, lalu jalankan setup lagi.\n' >&2
        exit 5
    fi
    printf '[Awan] Selesai: https://%s -> 127.0.0.1:{port}\n' "$domain"
fi
"#, https = if *https { 1 } else { 0 }, domain_re = domain.replace('.', "\\."));
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
            Self::Sysstat => {
                let script = format!(r#"{SETUP}
if installed sysstat; then
    printf '[Awan] Paket sysstat sudah terpasang.\n'
elif [ "$manager" = apt ]; then
    as_root apt-get -o APT::Update::Error-Mode=any update
    as_root apt-get install -y sysstat
else
    as_root dnf install -y sysstat
fi
# Debian/Ubuntu ship collection switched off.
if [ -f /etc/default/sysstat ]; then
    as_root sed -i 's/^ENABLED=.*/ENABLED="true"/' /etc/default/sysstat
fi
for conf in /etc/sysstat/sysstat /etc/sysconfig/sysstat; do
    [ -f "$conf" ] || continue
    current=$(sed -n 's/^HISTORY=["]*\([0-9][0-9]*\).*/\1/p' "$conf" | tail -n 1)
    if [ -z "$current" ] || [ "$current" -lt 31 ]; then
        if grep -q '^HISTORY=' "$conf"; then
            as_root sed -i 's/^HISTORY=.*/HISTORY=31/' "$conf"
        else
            printf 'HISTORY=31\n' | as_root tee -a "$conf" >/dev/null
        fi
        printf '[Awan] Retensi data sysstat diatur ke 31 hari (%s).\n' "$conf"
    fi
done
if command -v systemctl >/dev/null 2>&1; then
    as_root systemctl enable --now sysstat
    # Newer packages collect through timers instead of cron.
    for unit in sysstat-collect.timer sysstat-summary.timer; do
        if systemctl cat "$unit" >/dev/null 2>&1; then
            as_root systemctl enable --now "$unit"
        fi
    done
fi
printf '[Awan] sysstat aktif. Sampel pertama tercatat dalam 10 menit.\n'
"#);
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

/// A lowercase DNS name with at least two labels, e.g. `mail.example.com`.
/// Rejects IP addresses (Let's Encrypt does not issue for them) and
/// wildcards, so the value is safe in both shell and Nginx config.
pub fn valid_domain(domain: &str) -> bool {
    let labels: Vec<&str> = domain.split('.').collect();
    domain.len() <= 253 && labels.len() >= 2
        && !labels.last().is_some_and(|tld| tld.bytes().all(|c| c.is_ascii_digit()))
        && labels.iter().all(|label| {
            !label.is_empty() && label.len() <= 63 && !label.starts_with('-') && !label.ends_with('-')
                && label.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        })
}

pub fn valid_email(email: &str) -> bool {
    email.len() <= 254 && email.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && local.bytes().all(|c| c.is_ascii_alphanumeric() || b"._%+-".contains(&c))
            && valid_domain(&domain.to_ascii_lowercase())
    })
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
for entry in nginx:nginx certbot:certbot docker:docker git:git node:node redis:redis-server postgres:postgres; do
    id=${entry%%:*}
    executable=${entry#*:}
    if command -v "$executable" >/dev/null 2>&1; then printf 'B\t%s\n' "$id"; fi
done
if command -v docker-compose >/dev/null 2>&1; then printf 'B\tcompose\n'; fi
# Enabled Nginx sites, one line per file: domains, proxy targets, whether it
# has a certificate, and whether this app wrote it. Unreadable files are
# skipped; nothing here needs root.
for file in /etc/nginx/sites-enabled/* /etc/nginx/conf.d/*.conf; do
    [ -f "$file" ] && [ -r "$file" ] || continue
    awk -v file="$file" '
        NR == 1 && (index($0, "Managed by Awan Host Manager") || index($0, "Managed by Awan Host Remote")) { managed = 1 }
        { sub(/#.*/, "") }
        /^[[:space:]]*server_name[[:space:]]/ {
            line = $0; sub(/^[[:space:]]*server_name[[:space:]]+/, "", line); sub(/;.*/, "", line)
            n = split(line, parts, /[[:space:]]+/)
            for (i = 1; i <= n; i++) if (parts[i] != "" && parts[i] != "_" && !(parts[i] in seen)) { seen[parts[i]] = 1; names = names (names == "" ? "" : " ") parts[i] }
        }
        /^[[:space:]]*proxy_pass[[:space:]]/ {
            line = $0; sub(/^[[:space:]]*proxy_pass[[:space:]]+/, "", line); sub(/;.*/, "", line)
            if (line != "" && !(line in targets_seen)) { targets_seen[line] = 1; targets = targets (targets == "" ? "" : " ") line }
        }
        /^[[:space:]]*ssl_certificate[[:space:]]/ { ssl = 1 }
        END { if (names != "") printf "N\t%s\t%s\t%s\t%d\t%d\n", file, names, targets, ssl, managed }
    ' "$file"
done
printf 'AWAN_PACKAGES_END\n'
"#)
}

pub struct Package { pub name: String, pub version: String }

pub struct Inventory {
    pub manager: String,
    pub packages: Vec<Package>,
    pub binaries: Vec<String>,
    pub nginx_sites: Vec<NginxSite>,
}

/// One enabled Nginx config file, read from its `server_name`,
/// `proxy_pass`, and `ssl_certificate` lines.
#[derive(Clone)]
pub struct NginxSite {
    pub file: String,
    /// Space-separated `server_name` values.
    pub names: String,
    /// Space-separated `proxy_pass` targets; empty for static sites.
    pub targets: String,
    pub https: bool,
    /// Written by the Setup domain dialog (first-line marker).
    pub managed: bool,
}

pub fn valid_package_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 200 && name.as_bytes()[0].is_ascii_alphanumeric()
        && name.bytes().all(|c| c.is_ascii_alphanumeric() || b"+._:-".contains(&c))
}

impl Inventory {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let output = std::str::from_utf8(bytes)?;
        let mut result = Self { manager: String::new(), packages: Vec::new(), binaries: Vec::new(), nginx_sites: Vec::new() };
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
                ["N", file, names, targets, https @ ("0" | "1"), managed @ ("0" | "1")]
                    if result.nginx_sites.len() < 500
                        && [file, names, targets].iter().all(|field| field.len() <= 4096 && !field.chars().any(char::is_control)) => {
                    result.nginx_sites.push(NginxSite {
                        file: (*file).into(), names: (*names).into(), targets: (*targets).into(),
                        https: *https == "1", managed: *managed == "1",
                    });
                }
                ["AWAN_PACKAGES_END"] => { finished = true; break; }
                _ => {},
            }
        }
        if result.manager.is_empty() || !finished { bail!("Inventaris server tidak lengkap"); }
        result.packages.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
        Ok(result)
    }
}
