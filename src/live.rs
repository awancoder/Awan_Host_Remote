//! Live server reads for the Summary and Firewall pages, and the privileged
//! server actions (firewall changes, reboot/shutdown) they trigger.
#![allow(dead_code)]
use std::time::Instant;
use anyhow::{Context, Result, bail};
use russh::{Channel, ChannelMsg, client};
use zeroize::Zeroizing;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind { Summary, Ports }
impl Kind {
    pub fn index(self) -> usize { self as usize }
    pub fn name(self) -> &'static str { match self { Self::Summary => "summary", Self::Ports => "ports" } }
}

pub struct Request { pub kind: Kind, pub fields: Zeroizing<Vec<String>> }
impl Request {
    pub fn new(kind: Kind) -> Self { Self { kind, fields: Zeroizing::new(Vec::new()) } }
}

pub struct Report { pub rows: Vec<Vec<String>> }
impl Report {
    pub fn value(&self, key: &str) -> &str {
        self.rows.iter().find(|row| row.first().is_some_and(|v| v == key))
            .and_then(|row| row.get(1)).map(String::as_str).unwrap_or("Tidak tersedia")
    }
}

#[derive(Default)]
pub struct State {
    pub data: [Option<Report>; 2],
    pub errors: [Option<String>; 2],
    pub checked: [Option<Instant>; 2],
    pub busy: Option<Kind>,
    pub cpu: Vec<f32>,
}
impl State {
    pub fn finish(&mut self, kind: Kind, result: Result<Report, String>) {
        self.busy = None;
        let index = kind.index();
        self.checked[index] = Some(Instant::now());
        match result {
            Ok(report) => {
                if kind == Kind::Summary {
                    if let Ok(cpu) = report.value("cpu").parse::<f32>() {
                        if self.cpu.len() >= 60 { self.cpu.remove(0); }
                        self.cpu.push(cpu / 100.0);
                    }
                }
                self.data[index] = Some(report);
                self.errors[index] = None;
            }
            Err(error) => { self.data[index] = None; self.errors[index] = Some(error); }
        }
    }
    pub fn invalidate(&mut self) {
        for kind in [Kind::Summary, Kind::Ports] {
            self.data[kind.index()] = None;
            self.checked[kind.index()] = None;
        }
    }
}

pub async fn execute(channel: &mut Channel<client::Msg>, request: Request) -> Result<Report> {
    let script = include_str!("live_remote.py");
    channel.exec(true, format!("python3 -c '{}'", script.replace('\'', "'\"'\"'"))).await?;
    let mut payload = Zeroizing::new(request.kind.name().as_bytes().to_vec());
    payload.push(b'\n');
    for field in request.fields.iter() {
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
