//! Docker on the server for the Docker page: the container list (grouped by
//! Compose project via the labels Compose sets on every container) and the
//! actions the page runs, each a fixed command built from validated values.
use anyhow::{Context, Result, bail};
use russh::{Channel, ChannelMsg, client};

#[derive(Clone)]
pub struct Container {
    pub name: String,
    pub image: String,
    /// running, exited, restarting, paused, created, dead.
    pub state: String,
    /// Human text from Docker, e.g. "Up 3 hours", "Exited (0) 2 days ago".
    pub status: String,
    pub ports: String,
    /// Empty when the container was not started by Compose.
    pub project: String,
    pub service: String,
    pub working_dir: String,
    /// Absolute compose file paths, as Compose recorded them.
    pub config_files: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DbKind { MySql, Postgres }

impl Container {
    pub fn running(&self) -> bool { self.state == "running" }

    pub fn db_kind(&self) -> Option<DbKind> {
        let image = self.image.to_ascii_lowercase();
        if image.contains("mariadb") || image.contains("mysql") { Some(DbKind::MySql) }
        else if image.contains("postgres") { Some(DbKind::Postgres) }
        else { None }
    }

    /// A safe, zero-downtime config reload for well-known proxies.
    pub fn reload_command(&self) -> Option<&'static str> {
        let image = self.image.to_ascii_lowercase();
        if image.contains("caddy") { Some("caddy reload --config /etc/caddy/Caddyfile") }
        else if image.contains("nginx") { Some("nginx -t && nginx -s reload") }
        else { None }
    }

    pub fn project(&self) -> Option<Project> {
        if self.project.is_empty() || self.working_dir.is_empty() { return None; }
        Some(Project { name: self.project.clone(), working_dir: self.working_dir.clone(), config_files: self.config_files.clone() })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub working_dir: String,
    pub config_files: Vec<String>,
}

#[derive(Clone)]
pub enum Action {
    Logs(String),
    Restart(String),
    Stop(String),
    Start(String),
    Exec { container: String, command: String },
    /// Full dump streamed to a file on this computer (nothing is written on
    /// the server); see [`Action::is_download`].
    Backup { container: String, kind: DbKind },
    ComposeUp { project: Project, service: Option<String> },
    ComposeDown(Project),
    /// `docker network create <name>`, e.g. a shared `edge` network that a
    /// reverse proxy and several Compose projects join as `external: true`.
    NetworkCreate(String),
}

fn quote(value: &str) -> String { format!("'{}'", value.replace('\'', "'\"'\"'")) }

/// Docker container / Compose project and service names.
fn valid_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.bytes().all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}

fn valid_path(value: &str) -> bool {
    value.starts_with('/') && value.len() <= 4096 && !value.chars().any(char::is_control)
}

impl Action {
    pub fn title(&self) -> String {
        match self {
            Self::Logs(name) => format!("docker logs -f {name}"),
            Self::Restart(name) => format!("docker restart {name}"),
            Self::Stop(name) => format!("docker stop {name}"),
            Self::Start(name) => format!("docker start {name}"),
            Self::Exec { container, command } => format!("docker exec {container}: {command}"),
            Self::Backup { container, .. } => format!("backup {container}"),
            Self::ComposeUp { project, service: Some(service) } => format!("{}: up -d --build {service}", project.name),
            Self::ComposeUp { project, service: None } => format!("{}: up -d --build", project.name),
            Self::ComposeDown(project) => format!("{}: down", project.name),
            Self::NetworkCreate(name) => format!("docker network create {name}"),
        }
    }

    /// The command's stdout is the result itself (a dump), saved to a local
    /// file; stderr alone goes to the output panel.
    pub fn is_download(&self) -> bool { matches!(self, Self::Backup { .. }) }

    /// Whether this action can interrupt a running service, so the page
    /// asks before running it.
    pub fn disruptive(&self) -> bool {
        matches!(self, Self::Restart(_) | Self::Stop(_) | Self::ComposeUp { .. } | Self::ComposeDown(_))
    }

    /// The shell command, built only from validated values; run on its own
    /// exec channel (no PTY) with output streamed back to the page.
    pub fn command(&self) -> Result<String> {
        let script = match self {
            Self::Logs(name) | Self::Restart(name) | Self::Stop(name) | Self::Start(name) => {
                if !valid_name(name) { bail!("Nama container tidak valid"); }
                let verb = match self { Self::Logs(_) => "logs --tail 300 --follow", Self::Restart(_) => "restart", Self::Stop(_) => "stop", _ => "start" };
                format!("docker {verb} {}", quote(name))
            }
            Self::Exec { container, command } => {
                if !valid_name(container) { bail!("Nama container tidak valid"); }
                if command.trim().is_empty() || command.len() > 4096 || command.chars().any(char::is_control) { bail!("Perintah tidak valid"); }
                format!("docker exec {} sh -c {}", quote(container), quote(command))
            }
            Self::Backup { container, kind } => {
                if !valid_name(container) { bail!("Nama container tidak valid"); }
                // Runs inside the container with the container's own root
                // password variable, so the password is never typed, shown,
                // or sent from this app. The dump goes to stdout, which the
                // app writes to the chosen local file; stderr stays separate
                // so error text never ends up inside the .sql file.
                let dump = match kind {
                    DbKind::MySql => r#"pw="${MARIADB_ROOT_PASSWORD:-${MYSQL_ROOT_PASSWORD:-}}"; f="${MARIADB_ROOT_PASSWORD_FILE:-${MYSQL_ROOT_PASSWORD_FILE:-}}"; if [ -z "$pw" ] && [ -n "$f" ]; then pw=$(cat "$f"); fi; if command -v mariadb-dump >/dev/null 2>&1; then d=mariadb-dump; else d=mysqldump; fi; MYSQL_PWD="$pw" exec "$d" -uroot --all-databases --single-transaction --routines --events"#,
                    DbKind::Postgres => r#"exec pg_dumpall -U "${POSTGRES_USER:-postgres}""#,
                };
                return Ok(docker_shell(&format!("docker exec {} sh -c {}", quote(container), quote(dump))));
            }
            Self::ComposeUp { project, service } => {
                let base = compose_base(project)?;
                match service {
                    Some(service) if valid_name(service) => format!("{base} up -d --build {}", quote(service)),
                    Some(_) => bail!("Nama service tidak valid"),
                    None => format!("{base} up -d --build"),
                }
            }
            Self::ComposeDown(project) => format!("{} down", compose_base(project)?),
            Self::NetworkCreate(name) => {
                if !valid_name(name) { bail!("Nama network hanya boleh huruf, angka, titik, garis bawah, dan tanda hubung."); }
                // An existing network is reported as such, not as a failure.
                format!(
                    "if docker network inspect {name} >/dev/null 2>&1; then printf '[Awan] Network %s sudah ada.\\n' {name}; \
                     else docker network create {name}; fi",
                    name = quote(name),
                )
            }
        };
        Ok(docker_shell(&format!("exec 2>&1; {script}")))
    }
}

/// Wraps a script for an exec channel. An account added to the `docker`
/// group after this SSH login does not have that group in this session yet;
/// `sg docker` applies it to this one command, so no reconnect is needed.
pub(crate) fn docker_shell(script: &str) -> String {
    let wrapper = format!(
        "S={}; if ! id -nG | tr ' ' '\\n' | grep -qx docker \
         && getent group docker 2>/dev/null | cut -d: -f4 | tr ',' '\\n' | grep -qx \"$(id -un)\" \
         && command -v sg >/dev/null 2>&1; then exec sg docker -c \"$S\"; else exec /bin/sh -c \"$S\"; fi",
        quote(script),
    );
    format!("/bin/sh -c {}", quote(&wrapper))
}

/// `cd <project dir> && docker compose -p <name> -f <file>...`, using the
/// exact files Compose recorded so overrides/non-default names still apply.
fn compose_base(project: &Project) -> Result<String> {
    if !valid_name(&project.name) { bail!("Nama project tidak valid"); }
    if !valid_path(&project.working_dir) { bail!("Folder project tidak valid"); }
    let mut command = format!("cd -- {} && docker compose -p {}", quote(&project.working_dir), quote(&project.name));
    for file in &project.config_files {
        if !valid_path(file) { bail!("File compose tidak valid"); }
        command.push_str(" -f ");
        command.push_str(&quote(file));
    }
    Ok(command)
}

const LIST: &str = r#"if ! command -v docker >/dev/null 2>&1; then
    printf '[Awan] Docker belum terpasang di server. Pasang Docker Engine dari halaman Applications.\n' >&2
    exit 2
fi
docker ps -a --no-trunc --format '{{.Names}}\t{{.Image}}\t{{.State}}\t{{.Status}}\t{{.Ports}}\t{{.Label "com.docker.compose.project"}}\t{{.Label "com.docker.compose.service"}}\t{{.Label "com.docker.compose.project.working_dir"}}\t{{.Label "com.docker.compose.project.config_files"}}'
"#;

/// Runs a read-only script and returns its stdout (at most 4 MiB), or its
/// stderr as the error when it exits non-zero.
async fn collect(channel: &mut Channel<client::Msg>, script: &str, failure: &str) -> Result<String> {
    channel.exec(true, docker_shell(script)).await?;
    channel.eof().await?;
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut status = None;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => {
                if output.len() + data.len() > 4 * 1024 * 1024 { bail!("Keluaran Docker melebihi batas 4 MiB"); }
                output.extend_from_slice(&data);
            }
            ChannelMsg::ExtendedData { data, .. } => {
                let room = 8192usize.saturating_sub(errors.len());
                errors.extend_from_slice(&data[..data.len().min(room)]);
            }
            ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    if status != Some(0) {
        let message = String::from_utf8_lossy(&errors).trim().to_string();
        bail!("{}", if message.is_empty() { failure.to_owned() } else { message });
    }
    String::from_utf8(output).context("Keluaran Docker bukan UTF-8")
}

/// One running container's resource use, from `docker stats`.
#[derive(Clone)]
pub struct Usage {
    /// Percent of ONE core, as Docker reports it (a busy container on a
    /// 4-core host can show up to 400%).
    pub cpu: f32,
    pub mem_bytes: u64,
    /// e.g. "123.4MiB / 3.84GiB".
    pub mem_text: String,
    /// Percent of the container's memory limit (the host's RAM when unset).
    pub mem_percent: f32,
    pub net: String,
    pub block: String,
}

#[derive(Clone, Default)]
pub struct Stats {
    /// CPU cores on the host, to scale CPU bars (100% per core).
    pub cores: u32,
    pub usage: std::collections::HashMap<String, Usage>,
}

const STATS: &str = r#"if ! command -v docker >/dev/null 2>&1; then
    printf '[Awan] Docker belum terpasang di server.\n' >&2
    exit 2
fi
printf 'CORES\t%s\n' "$(nproc 2>/dev/null || echo 1)"
docker stats --no-stream --format '{{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}\t{{.NetIO}}\t{{.BlockIO}}'
"#;

/// Bytes from Docker's sizes: "123.4MiB", "1.2GiB", "512kB", "0B".
fn parse_size(text: &str) -> u64 {
    let text = text.trim();
    let split = text.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let number: f64 = number.parse().unwrap_or(0.);
    let factor: f64 = match unit.trim() {
        "KiB" => 1024., "MiB" => 1024f64.powi(2), "GiB" => 1024f64.powi(3), "TiB" => 1024f64.powi(4),
        "kB" | "KB" => 1e3, "MB" => 1e6, "GB" => 1e9, "TB" => 1e12,
        _ => 1.,
    };
    (number * factor) as u64
}

/// One `docker stats` sample (takes about two seconds on the server).
pub async fn stats(channel: &mut Channel<client::Msg>) -> Result<Stats> {
    let text = collect(channel, STATS, "Statistik container tidak dapat dibaca.").await?;
    let percent = |value: &str| value.trim().trim_end_matches('%').parse::<f32>().unwrap_or(0.);
    let mut stats = Stats { cores: 1, ..Stats::default() };
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["CORES", cores] => stats.cores = cores.trim().parse().unwrap_or(1).max(1),
            [name, cpu, mem, mem_percent, net, block] if valid_name(name) => {
                stats.usage.insert((*name).to_owned(), Usage {
                    cpu: percent(cpu),
                    mem_bytes: parse_size(mem.split('/').next().unwrap_or("")),
                    mem_text: mem.trim().to_owned(),
                    mem_percent: percent(mem_percent),
                    net: net.trim().to_owned(),
                    block: block.trim().to_owned(),
                });
            }
            _ => {}
        }
        if stats.usage.len() >= 500 { break; }
    }
    Ok(stats)
}

pub async fn list(channel: &mut Channel<client::Msg>) -> Result<Vec<Container>> {
    let text = collect(channel, LIST, "Daftar container tidak dapat dibaca.").await?;
    let mut containers = Vec::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 9 { continue; }
        if !valid_name(fields[0]) { continue; }
        containers.push(Container {
            name: fields[0].into(), image: fields[1].into(), state: fields[2].into(), status: fields[3].into(),
            ports: fields[4].into(), project: fields[5].into(), service: fields[6].into(), working_dir: fields[7].into(),
            config_files: fields[8].split(',').map(str::trim).filter(|file| !file.is_empty()).map(str::to_owned).collect(),
        });
        if containers.len() >= 500 { break; }
    }
    containers.sort_by(|a, b| a.project.cmp(&b.project).then(a.service.cmp(&b.service)).then(a.name.cmp(&b.name)));
    Ok(containers)
}

/// Output kept for the page; older text is dropped from the front.
const MAX_OUTPUT: usize = 128 * 1024;

/// Docker page state for the active session; reset on disconnect.
#[derive(Default)]
pub struct State {
    pub containers: Option<Vec<Container>>,
    pub list_error: Option<String>,
    pub listing: bool,
    pub running: bool,
    pub stopping: bool,
    pub title: String,
    /// The running action only streams logs, so the page offers Close
    /// (stop + hide) instead of Stop.
    pub follow: bool,
    pub output: String,
    pub result: Option<Result<(), String>>,
    /// Bumped on new output so the page can keep the log scrolled down.
    pub revision: u64,
    /// A backup download: bytes written to the local file so far.
    pub downloaded: Option<u64>,
    /// Local file a backup is (being) saved to.
    pub saved_to: Option<std::path::PathBuf>,
    /// Latest CPU/RAM sample of the running containers.
    pub stats: Option<Stats>,
    pub stats_loading: bool,
    pub stats_error: Option<String>,
    pub stats_at: Option<std::time::Instant>,
    /// Bytes of a UTF-8 character split across two chunks.
    partial: Vec<u8>,
}

impl State {
    pub fn push_output(&mut self, bytes: &[u8]) {
        let mut data = std::mem::take(&mut self.partial);
        data.extend_from_slice(bytes);
        let valid = match std::str::from_utf8(&data) {
            Ok(_) => data.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(_) => data.len(),
        };
        self.partial = data.split_off(valid);
        self.output.push_str(&clean_output(&data));
        if self.output.len() > MAX_OUTPUT {
            let mut cut = self.output.len() - MAX_OUTPUT;
            while !self.output.is_char_boundary(cut) { cut += 1; }
            let cut = self.output[cut..].find('\n').map_or(cut, |line| cut + line + 1);
            self.output.drain(..cut);
        }
        self.revision += 1;
    }

    pub fn begin(&mut self, title: String) {
        self.running = true;
        self.stopping = false;
        self.title = title;
        self.follow = false;
        self.downloaded = None;
        self.saved_to = None;
        self.output.clear();
        self.partial.clear();
        self.result = None;
        self.revision += 1;
    }

    /// Docker reports a socket permission error when the SSH user is not in
    /// the `docker` group (and is not root).
    pub fn permission_denied(&self) -> bool {
        let denied = |text: &str| { let text = text.to_ascii_lowercase(); text.contains("permission denied") && text.contains("docker") };
        self.list_error.as_deref().is_some_and(denied)
            || (self.result.as_ref().is_some_and(Result::is_err) && denied(&self.output))
    }
}

/// 950 B, 12.4 KB, 3.1 MB, 1.2 GB.
pub fn size_text(bytes: u64) -> String {
    match bytes {
        0..=999 => format!("{bytes} B"),
        1_000..=999_999 => format!("{:.1} KB", bytes as f64 / 1e3),
        1_000_000..=999_999_999 => format!("{:.1} MB", bytes as f64 / 1e6),
        _ => format!("{:.2} GB", bytes as f64 / 1e9),
    }
}

/// `backup_<container>_<YYYY-MM-DD_HHMMSS>.sql`, in UTC (std has no local
/// time zone).
pub fn backup_file_name(container: &str) -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("backup_{container}_{year:04}-{month:02}-{day:02}_{:02}{:02}{:02}.sql", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Output is shown as plain text, so terminal escape sequences (colors,
/// cursor moves from build progress) are stripped and `\r` becomes a newline.
pub fn clean_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.peek() {
                // CSI: ESC [ ... final byte in '@'..='~'
                Some('[') => { chars.next(); while let Some(c) = chars.next() { if ('@'..='~').contains(&c) { break; } } }
                // OSC: ESC ] ... BEL or ESC \
                Some(']') => { chars.next(); while let Some(c) = chars.next() { if c == '\x07' || c == '\x1b' { break; } } }
                _ => { chars.next(); }
            },
            '\r' => { if chars.peek() != Some(&'\n') { clean.push('\n'); } }
            '\n' | '\t' => clean.push(c),
            c if c.is_control() => {}
            c => clean.push(c),
        }
    }
    clean
}
