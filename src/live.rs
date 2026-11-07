//! Live server reads for the Summary and Firewall pages, and the privileged
//! server actions (firewall changes, reboot/shutdown) they trigger.
#![allow(dead_code)]
use std::time::Instant;
use anyhow::{Context, Result, bail};
use russh::{Channel, ChannelMsg, client};
use zeroize::Zeroizing;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind { Summary, Ports, History }
impl Kind {
    pub fn index(self) -> usize { self as usize }
    pub fn name(self) -> &'static str { match self { Self::Summary => "summary", Self::Ports => "ports", Self::History => "history" } }
}

pub struct Request { pub kind: Kind, pub fields: Zeroizing<Vec<String>> }
impl Request {
    pub fn new(kind: Kind) -> Self { Self { kind, fields: Zeroizing::new(Vec::new()) } }
    pub fn history(range: Range) -> Self {
        Self { kind: Kind::History, fields: Zeroizing::new(vec![range.seconds().to_string()]) }
    }
}

/// The span the Summary history chart shows. `Live` is the app's own 5 s
/// samples; the others are read from sysstat (sar) on the server.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Range { #[default] Live, Day, Week }
impl Range {
    pub const ALL: [Self; 3] = [Self::Live, Self::Day, Self::Week];
    pub fn seconds(self) -> u64 {
        match self { Self::Live => 300, Self::Day => 86_400, Self::Week => 7 * 86_400 }
    }
}

/// sysstat records a sample every 10 minutes, so re-reading the archive more
/// often than this only repeats the same data.
pub const ARCHIVE_REFRESH: std::time::Duration = std::time::Duration::from_secs(600);

/// One averaged sysstat bucket. `ago` is seconds before the server's clock
/// at read time, so a skewed local clock does not shift the chart.
#[derive(Clone, Copy)]
pub struct ArchivePoint { pub ago: u64, pub cpu: f64, pub memory: f64 }

pub struct Report { pub rows: Vec<Vec<String>> }
impl Report {
    pub fn value(&self, key: &str) -> &str {
        self.rows.iter().find(|row| row.first().is_some_and(|v| v == key))
            .and_then(|row| row.get(1)).map(String::as_str).unwrap_or("Tidak tersedia")
    }
}

/// One Summary reading for the CPU/RAM history chart, both in percent.
#[derive(Clone, Copy)]
pub struct Sample { pub at: Instant, pub cpu: f64, pub memory: f64 }

/// Samples kept for the history chart: 60 × the 5 s Summary poll ≈ 5 min.
pub const HISTORY_LEN: usize = 60;

#[derive(Default)]
pub struct State {
    pub data: [Option<Report>; 3],
    pub errors: [Option<String>; 3],
    pub checked: [Option<Instant>; 3],
    pub busy: Option<Kind>,
    pub history: Vec<Sample>,
    pub range: Range,
    /// Parsed `point` rows of the last History report, oldest first.
    pub archive: Vec<ArchivePoint>,
}
impl State {
    pub fn finish(&mut self, kind: Kind, result: Result<Report, String>) {
        self.busy = None;
        let index = kind.index();
        self.checked[index] = Some(Instant::now());
        if kind == Kind::History {
            self.archive.clear();
            // A reply for a range the user has since switched away from is
            // dropped; leaving `checked` empty makes the poll ask again.
            if let Ok(report) = &result {
                if report.value("range") != self.range.seconds().to_string() {
                    self.data[index] = None;
                    self.checked[index] = None;
                    return;
                }
                self.archive = report.rows.iter()
                    .filter(|row| row.len() == 4 && row[0] == "point")
                    .filter_map(|row| Some(ArchivePoint { ago: row[1].parse().ok()?, cpu: row[2].parse().ok()?, memory: row[3].parse().ok()? }))
                    .collect();
            }
        }
        match result {
            Ok(report) => {
                if kind == Kind::Summary {
                    // Both series share one x axis, so a reading only joins
                    // the history when CPU and memory were both read.
                    let cpu = report.value("cpu").parse::<f64>().ok();
                    let memory = percent(report.value("memory_used"), report.value("memory_total"));
                    if let (Some(cpu), Some(memory)) = (cpu, memory) {
                        if self.history.len() >= HISTORY_LEN { self.history.remove(0); }
                        self.history.push(Sample { at: Instant::now(), cpu: cpu.clamp(0.0, 100.0), memory });
                    }
                }
                self.data[index] = Some(report);
                self.errors[index] = None;
            }
            Err(error) => { self.data[index] = None; self.errors[index] = Some(error); }
        }
    }
    pub fn invalidate(&mut self) {
        for kind in [Kind::Summary, Kind::Ports, Kind::History] {
            self.data[kind.index()] = None;
            self.checked[kind.index()] = None;
        }
        self.archive.clear();
    }

    /// Switches the chart's range; a server range is re-read from scratch.
    pub fn set_range(&mut self, range: Range) {
        self.range = range;
        let index = Kind::History.index();
        self.data[index] = None;
        self.errors[index] = None;
        self.checked[index] = None;
        self.archive.clear();
    }
}

/// `used / total` in percent (0–100), or `None` when either is unreadable.
pub fn percent(used: &str, total: &str) -> Option<f64> {
    match (used.parse::<f64>(), total.parse::<f64>()) {
        (Ok(used), Ok(total)) if total > 0.0 => Some((used / total * 100.0).clamp(0.0, 100.0)),
        _ => None,
    }
}

pub async fn execute(channel: &mut Channel<client::Msg>, request: Request) -> Result<Report> {
    run_script(channel, python_command(include_str!("live_remote.py")), request.kind.name(), &request.fields).await
}

/// `python3 -c '<script>'`, quoted for a POSIX shell.
pub fn python_command(script: &str) -> String {
    format!("python3 -c '{}'", script.replace('\'', "'\"'\"'"))
}

/// Runs `command` (a script speaking the `AWAN_LIVE_V1` protocol) with
/// `kind` and hex-encoded `fields` on stdin, and parses its rows.
pub async fn run_script(channel: &mut Channel<client::Msg>, command: String, kind: &str, fields: &[String]) -> Result<Report> {
    channel.exec(true, command).await?;
    let mut payload = Zeroizing::new(kind.as_bytes().to_vec());
    payload.push(b'\n');
    for field in fields.iter() {
        if field.len() > 4096 || field.contains('\0') { bail!("Kolom permintaan terlalu panjang atau berisi NUL."); }
        for byte in field.bytes() { payload.extend_from_slice(format!("{byte:02x}").as_bytes()); }
        payload.push(b'\n');
    }
    channel.data(&payload[..]).await?;
    channel.eof().await?;
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut status = None;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => {
                if output.len() + data.len() > 2 * 1024 * 1024 { bail!("Hasil melebihi 2 MiB; persempit pilihan data."); }
                output.extend_from_slice(&data);
            }
            ChannelMsg::ExtendedData { data, .. } => {
                let count = data.len().min(16_384usize.saturating_sub(errors.len()));
                errors.extend_from_slice(&data[..count]);
            }
            ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
            ChannelMsg::Failure | ChannelMsg::ExitSignal { .. } => bail!("Pembacaan server ditolak atau dihentikan."),
            ChannelMsg::Close => break,
            _ => {},
        }
    }
    if status != Some(0) { bail!("Pembacaan gagal ({status:?}): {}", String::from_utf8_lossy(&errors)); }
    let output = std::str::from_utf8(&output)?;
    let mut lines = output.lines();
    if lines.next() != Some("AWAN_LIVE_V1") { bail!("Respons server tidak dikenali."); }
    let mut rows = Vec::new();
    let mut finished = false;
    for line in lines {
        if line == "END" { finished = true; break; }
        if rows.len() >= 5000 { bail!("Hasil melebihi 5.000 baris."); }
        let mut row = Vec::new();
        for cell in line.split('\t') {
            if row.len() >= 256 || cell.len() % 2 != 0 || !cell.bytes().all(|b| b.is_ascii_hexdigit()) { bail!("Format respons tidak valid."); }
            let bytes = (0..cell.len()).step_by(2).map(|index| u8::from_str_radix(&cell[index..index+2], 16)).collect::<std::result::Result<Vec<_>, _>>()?;
            row.push(String::from_utf8(bytes).context("Hasil bukan UTF-8")?);
        }
        rows.push(row);
    }
    if !finished { bail!("Hasil server tidak lengkap."); }
    Ok(Report { rows })
}

#[derive(Clone)]
pub enum Action {
    Firewall { engine: String, port: u16, protocol: String, open: bool },
    FirewallToggle { engine: String, enable: bool },
    FirewallDeleteRule { number: u32 },
    FirewallReload { engine: String },
    Power { reboot: bool },
}
impl Action {
    pub fn title(&self) -> String {
        match self {
            Self::Firewall { engine, port, protocol, open } => format!("{} {port}/{protocol} · {engine}", if *open { "Tambah izin port" } else if engine == "ufw" { "Tambah deny" } else { "Hapus izin port" }),
            Self::FirewallToggle { engine, enable } => format!("{} {engine}", if *enable { "Aktifkan" } else { "Nonaktifkan" }),
            Self::FirewallDeleteRule { number } => format!("Hapus aturan UFW #{number}"),
            Self::FirewallReload { engine } => format!("Reload {engine}"),
            Self::Power { reboot } => if *reboot { "Reboot server" } else { "Shutdown server" }.into(),
        }
    }
    pub fn command(&self) -> Result<String> {
        let command = match self {
            Self::Firewall { engine, port, protocol, open } => {
                if *port == 0 || !["tcp", "udp"].contains(&protocol.as_str()) { bail!("Port atau protokol tidak valid."); }
                match engine.as_str() {
                    "ufw" => format!("status=$(as_root ufw status)\nprintf '%s\\n' \"$status\"\nprintf '%s\\n' \"$status\" | grep -q '^Status: active' || {{ echo 'UFW tidak aktif; tidak diaktifkan otomatis.' >&2; exit 2; }}\nas_root ufw {} {port}/{protocol}\nas_root ufw status verbose", if *open { "allow" } else { "deny" }),
                    "firewalld" => format!("as_root firewall-cmd --state\nas_root firewall-cmd --{}-port={port}/{protocol}\nas_root firewall-cmd --list-all", if *open { "add" } else { "remove" }),
                    _ => bail!("Pengelolaan firewall mendukung UFW atau firewalld."),
                }
            }
            Self::FirewallToggle { engine, enable } => {
                match (engine.as_str(), enable) {
                    // `--force` skips the interactive "may disrupt existing
                    // ssh connections" prompt, which this exec channel could
                    // never answer anyway (it's separate from the shell PTY).
                    // `ufw disable` never prompts, so no `--force` needed there.
                    ("ufw", true) => "as_root ufw --force enable\nas_root ufw status verbose".to_string(),
                    ("ufw", false) => "as_root ufw disable\nas_root ufw status verbose".to_string(),
                    // Stopping firewalld means a follow-up `firewall-cmd
                    // --list-all` would just fail, so skip it on disable.
                    ("firewalld", true) => "as_root systemctl enable --now firewalld\nas_root firewall-cmd --state\nas_root firewall-cmd --list-all".to_string(),
                    ("firewalld", false) => "as_root systemctl disable --now firewalld".to_string(),
                    _ => bail!("Pengelolaan firewall mendukung UFW atau firewalld."),
                }
            }
            Self::FirewallDeleteRule { number } => {
                if *number == 0 || *number > 9999 { bail!("Nomor aturan UFW tidak valid."); }
                // `--force` skips ufw's interactive "proceed with operation
                // (y|n)?" confirmation, which this exec channel could never
                // answer (same reasoning as the enable/disable commands above).
                format!("as_root ufw --force delete {number}\nas_root ufw status numbered")
            }
            Self::FirewallReload { engine } => {
                match engine.as_str() {
                    "ufw" => "as_root ufw reload\nas_root ufw status numbered".to_string(),
                    "firewalld" => "as_root firewall-cmd --reload\nas_root firewall-cmd --list-all".to_string(),
                    _ => bail!("Pengelolaan firewall mendukung UFW atau firewalld."),
                }
            }
            Self::Power { reboot } => format!("as_root systemctl {}", if *reboot { "reboot" } else { "poweroff" }),
        };
        Ok(crate::packages::shell_command(&format!("set -e\nexport LC_ALL=C\nas_root() {{ if [ \"$(id -u)\" = 0 ]; then \"$@\"; else sudo -- \"$@\"; fi; }}\n{command}\n")))
    }
}
