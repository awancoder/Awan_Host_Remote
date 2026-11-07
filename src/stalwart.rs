//! The Stalwart Setup page: a guided setup for a Stalwart mail server running
//! in Docker. The server-side check (`stalwart_remote.py`) only reads; the
//! setup itself happens in Stalwart's own wizard, reached through an SSH
//! tunnel, and the restart goes through the Docker page's confirmed action.
use anyhow::{Result, bail};
use russh::{Channel, client};

pub struct Request {
    pub container: String,
    pub hostname: String,
    pub domain: String,
    /// The address the SSH session connected to, a fallback for the
    /// server's public IP when the check cannot learn it from DNS.
    pub address: String,
}

/// A DNS name the check can query: ASCII letters, digits and hyphens in
/// dot-separated labels, with at least one dot.
pub fn valid_hostname(name: &str) -> bool {
    name.len() <= 253 && name.contains('.')
        && name.split('.').all(|label| {
            !label.is_empty() && label.len() <= 63 && !label.starts_with('-') && !label.ends_with('-')
                && label.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

/// `mail.example.org` → `example.org`.
pub fn domain_of(hostname: &str) -> Option<&str> {
    hostname.split_once('.').map(|(_, rest)| rest).filter(|rest| rest.contains('.'))
}

pub async fn check(channel: &mut Channel<client::Msg>, request: Request) -> Result<crate::live::Report> {
    if !crate::docker::valid_name(&request.container) { bail!("Nama container tidak valid"); }
    if !valid_hostname(&request.hostname) || !valid_hostname(&request.domain) { bail!("Hostname atau domain tidak valid"); }
    let command = crate::docker::docker_shell(&crate::live::python_command(include_str!("stalwart_remote.py")));
    let fields = [request.container, request.hostname, request.domain, request.address];
    crate::live::run_script(channel, command, "check", &fields).await
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status { Ok, Warn, Fail, Unknown }

pub struct Check { pub status: Status, pub detail: String }

#[derive(Default)]
pub struct State {
    /// Container the page is set up for.
    pub container: Option<String>,
    pub busy: bool,
    pub report: Option<crate::live::Report>,
    pub error: Option<String>,
    /// Hostname/domain the current report was made for.
    pub checked_for: Option<(String, String)>,
}

impl State {
    pub fn open(&mut self, container: String) {
        if self.container.as_deref() != Some(container.as_str()) {
            *self = Self { container: Some(container), ..Self::default() };
        }
    }

    pub fn finish(&mut self, result: Result<crate::live::Report, String>) {
        self.busy = false;
        match result {
            Ok(report) => { self.report = Some(report); self.error = None; }
            Err(error) => { self.report = None; self.error = Some(error); }
        }
    }

    pub fn check(&self, key: &str) -> Option<Check> {
        let row = self.report.as_ref()?.rows.iter().find(|row| row.len() == 4 && row[0] == "check" && row[1] == key)?;
        let status = match row[2].as_str() { "ok" => Status::Ok, "warn" => Status::Warn, "fail" => Status::Fail, _ => Status::Unknown };
        Some(Check { status, detail: row[3].clone() })
    }

    pub fn value(&self, key: &str) -> Option<&str> {
        let report = self.report.as_ref()?;
        report.rows.iter().any(|row| row.first().is_some_and(|k| k == key)).then(|| report.value(key))
    }
}

/// When a DNS record has to exist for the setup to succeed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Before restarting after the wizard: with manual DNS, Let's Encrypt
    /// checks every certificate name on port 443 at that restart.
    BeforeRestart,
    /// Values Stalwart generates (keys, policy ids): copied from its zone
    /// file in the WebUI once the wizard has run.
    FromZoneFile,
}

pub struct Record {
    /// Check key in the report; `None` for records the check cannot query.
    pub key: Option<&'static str>,
    pub kind: &'static str,
    pub name: String,
    /// Exact value to enter, or `None` when it comes from the zone file.
    pub value: Option<String>,
    pub phase: Phase,
}

/// The records with a known value as a BIND zone file for Cloudflare's
/// DNS › Records › Import. `cf-proxied:false` keeps A/CNAME on DNS only;
/// Cloudflare reads that tag on import.
pub fn zone_file(records: &[Record], domain: &str) -> String {
    let mut text = format!(
        "; Stalwart DNS records for {domain}, from Awan Host Manager.\n\
         ; Cloudflare: DNS > Records > Import and Export > Import, with \"Proxy imported DNS records\" off.\n\
         $ORIGIN {domain}.\n$TTL 300\n",
    );
    for record in records {
        let Some(value) = &record.value else { continue; };
        let data = match record.kind {
            "TXT" => format!("\"{value}\""),
            "MX" | "CNAME" => format!("{value}."),
            _ => value.clone(),
        };
        let tag = if matches!(record.kind, "A" | "CNAME") { " ; cf_tags=cf-proxied:false" } else { "" };
        text.push_str(&format!("{}.\t300\tIN\t{}\t{data}{tag}\n", record.name, record.kind));
    }
    text
}

/// The records to create in Cloudflare (all DNS only), in the order the
/// page lists them.
pub fn records(hostname: &str, domain: &str, server_ip: &str) -> Vec<Record> {
    let ip = if server_ip.is_empty() { "<IP server>".to_string() } else { server_ip.to_string() };
    let mut list = vec![
        Record { key: Some("a_record"), kind: "A", name: hostname.into(), value: Some(ip), phase: Phase::BeforeRestart },
        Record { key: Some("mx"), kind: "MX", name: domain.into(), value: Some(format!("10 {hostname}")), phase: Phase::BeforeRestart },
        Record { key: Some("spf_domain"), kind: "TXT", name: domain.into(), value: Some("v=spf1 mx -all".into()), phase: Phase::BeforeRestart },
        Record { key: Some("spf_host"), kind: "TXT", name: hostname.into(), value: Some("v=spf1 a -all".into()), phase: Phase::BeforeRestart },
    ];
    for (key, label) in [("autoconfig", "autoconfig"), ("autodiscover", "autodiscover"), ("mta_sts", "mta-sts"), ("ua_auto_config", "ua-auto-config")] {
        list.push(Record { key: Some(key), kind: "CNAME", name: format!("{label}.{domain}"), value: Some(hostname.into()), phase: Phase::BeforeRestart });
    }
    for (key, kind, name) in [
        (None, "TXT", format!("<selector>._domainkey.{domain}")),
        (Some("dmarc"), "TXT", format!("_dmarc.{domain}")),
        (Some("mta_sts_txt"), "TXT", format!("_mta-sts.{domain}")),
        (Some("tls_rpt"), "TXT", format!("_smtp._tls.{domain}")),
        (Some("ua_txt"), "TXT", format!("_ua-auto-config.{domain}")),
        (Some("srv_imaps"), "SRV", format!("_imaps._tcp.{domain}")),
        (Some("srv_submissions"), "SRV", format!("_submissions._tcp.{domain}")),
    ] {
        list.push(Record { key, kind, name, value: None, phase: Phase::FromZoneFile });
    }
    list
}
