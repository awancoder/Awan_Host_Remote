//! Git-over-SSH login on the server: finds (or creates) the SSH account's
//! key, verifies the provider's host key with the user, and reports which
//! provider account that key authenticates as. Runs on its own exec channel.
use anyhow::{Context, Result, bail};
use russh::{Channel, client};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Provider {
    #[default]
    GitHub,
    GitLab,
}

impl Provider {
    pub const ALL: [Self; 2] = [Self::GitHub, Self::GitLab];

    pub fn name(self) -> &'static str {
        match self { Self::GitHub => "GitHub", Self::GitLab => "GitLab" }
    }

    fn host(self) -> &'static str {
        match self { Self::GitHub => "github.com", Self::GitLab => "gitlab.com" }
    }
}

pub enum Request {
    /// Key + known_hosts state, and an auth test once the host is trusted.
    Status(Provider),
    /// Fetches the host key fingerprint for the user to compare.
    Scan(Provider),
    /// Re-scans and trusts the host key only if it still matches the
    /// fingerprint the user approved, then reports status.
    Trust(Provider, String),
    /// Creates `~/.ssh/id_ed25519` unless a default key already exists,
    /// then reports status.
    Keygen(Provider),
}

#[derive(Clone)]
pub struct Key {
    pub path: String,
    pub public: String,
}

#[derive(Clone)]
pub enum Auth {
    Connected(String),
    Denied,
    Failed(String),
}

#[derive(Clone)]
pub struct Status {
    pub provider: Provider,
    pub key: Option<Key>,
    pub known_host: bool,
    /// `None` while the host key is not trusted yet (testing would prompt).
    pub auth: Option<Auth>,
}

pub enum Reply {
    Status(Status),
    Fingerprint(Provider, String),
}

fn quote(value: &str) -> String { format!("'{}'", value.replace('\'', "'\"'\"'")) }

fn valid_fingerprint(value: &str) -> bool {
    value.strip_prefix("SHA256:").is_some_and(|rest| {
        !rest.is_empty() && rest.len() <= 64 && rest.bytes().all(|c| c.is_ascii_alphanumeric() || b"+/=".contains(&c))
    })
}

fn valid_username(value: &str) -> bool {
    !value.is_empty() && value.len() <= 100 && value.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}

fn parse_auth(provider: Provider, text: &str) -> Auth {
    let user = match provider {
        Provider::GitHub if text.contains("successfully authenticated") => {
            text.split("Hi ").nth(1).and_then(|rest| rest.split('!').next())
        }
        Provider::GitLab => text.split("Welcome to GitLab, @").nth(1).and_then(|rest| rest.split('!').next()),
        _ => None,
    };
    match user.map(str::trim) {
        Some(user) if valid_username(user) => Auth::Connected(user.to_owned()),
        _ if text.contains("Permission denied") => Auth::Denied,
        _ => Auth::Failed(text.trim().chars().take(300).collect()),
    }
}

pub async fn execute(channel: &mut Channel<client::Msg>, request: Request) -> Result<Reply> {
    let (action, provider, fingerprint) = match &request {
        Request::Status(provider) => ("status", *provider, String::new()),
        Request::Scan(provider) => ("scan", *provider, String::new()),
        Request::Keygen(provider) => ("keygen", *provider, String::new()),
        Request::Trust(provider, fingerprint) => {
            if !valid_fingerprint(fingerprint) { bail!("Fingerprint tidak valid; ambil ulang fingerprint."); }
            ("trust", *provider, fingerprint.clone())
        }
    };
    // `sh -c SCRIPT $0 $1 $2 $3`: arguments are passed positionally, never
    // spliced into the script text.
    let command = format!("/bin/sh -c {} awan {} {} {}", quote(SCRIPT), quote(action), quote(provider.host()), quote(&fingerprint));
    channel.exec(true, command).await?;
    channel.eof().await?;
    let output = crate::remote_file::receive(channel).await?;
    let output = String::from_utf8(output).context("Respons login Git tidak valid")?;
    let mut lines = output.lines();
    if lines.next() != Some("AWAN_GIT_V1") { bail!("Respons login Git tidak dikenali"); }

    let mut status = Status { provider, key: None, known_host: false, auth: None };
    for line in lines {
        let fields: Vec<&str> = line.splitn(3, '\t').collect();
        match fields.as_slice() {
            ["FP", fp] => {
                if !valid_fingerprint(fp) { bail!("Fingerprint dari server tidak valid"); }
                return Ok(Reply::Fingerprint(provider, (*fp).to_owned()));
            }
            ["KEY", path, public] => {
                let public = public.trim();
                let looks_like_key = public.starts_with("ssh-") || public.starts_with("ecdsa-");
                if !looks_like_key || public.len() > 4096 || public.chars().any(char::is_control) || path.chars().any(char::is_control) {
                    bail!("Public key di server tidak valid");
                }
                status.key = Some(Key { path: (*path).to_owned(), public: public.to_owned() });
            }
            ["KNOWN", flag] => status.known_host = *flag == "1",
            ["AUTH", text] => status.auth = Some(parse_auth(provider, text)),
            ["AUTH"] => status.auth = Some(Auth::Failed(String::new())),
            _ => {}
        }
    }
    Ok(Reply::Status(status))
}

// POSIX sh + OpenSSH client tools (ssh, ssh-keygen, ssh-keyscan), which any
// server reached over SSH normally has. The private key never leaves the
// server; only the public key and the provider's greeting are read back.
const SCRIPT: &str = r#"set -u
action="$1"
host="$2"
expected="${3:-}"
umask 077
for tool in ssh ssh-keygen ssh-keyscan; do
    command -v "$tool" >/dev/null 2>&1 || { printf '[Awan] %s tidak tersedia di server (paket openssh-client).\n' "$tool" >&2; exit 2; }
done
mkdir -p "$HOME/.ssh" && chmod 700 "$HOME/.ssh" || { printf '[Awan] Tidak dapat menyiapkan folder ~/.ssh.\n' >&2; exit 2; }
scan() {
    ssh-keyscan -T 10 -t ed25519 "$host" 2>/dev/null
}
fingerprint() {
    printf '%s\n' "$1" | ssh-keygen -lf - 2>/dev/null | awk 'NR == 1 { print $2 }'
}
key=""
for name in id_ed25519 id_ecdsa id_rsa; do
    if [ -f "$HOME/.ssh/$name" ] && [ -f "$HOME/.ssh/$name.pub" ]; then key="$HOME/.ssh/$name"; break; fi
done
case "$action" in
    scan)
        lines=$(scan)
        fp=$(fingerprint "$lines")
        [ -n "$fp" ] || { printf '[Awan] Tidak dapat mengambil host key %s dari server ini.\n' "$host" >&2; exit 3; }
        printf 'AWAN_GIT_V1\nFP\t%s\n' "$fp"
        exit 0
        ;;
    trust)
        lines=$(scan)
        fp=$(fingerprint "$lines")
        if [ -z "$fp" ] || [ "$fp" != "$expected" ]; then
            printf '[Awan] Host key %s berubah atau tidak dapat dibaca; ulangi verifikasi.\n' "$host" >&2
            exit 4
        fi
        printf '%s\n' "$lines" >> "$HOME/.ssh/known_hosts" && chmod 600 "$HOME/.ssh/known_hosts"
        ;;
    keygen)
        if [ -z "$key" ]; then
            ssh-keygen -q -t ed25519 -N '' -C "awan-host-remote@$(uname -n)" -f "$HOME/.ssh/id_ed25519" </dev/null \
                || { printf '[Awan] Gagal membuat SSH key.\n' >&2; exit 5; }
            key="$HOME/.ssh/id_ed25519"
        fi
        ;;
    status) ;;
    *) printf '[Awan] Perintah tidak dikenali.\n' >&2; exit 1 ;;
esac
printf 'AWAN_GIT_V1\n'
if [ -n "$key" ]; then
    printf 'KEY\t%s\t%s\n' "$key.pub" "$(head -n 1 "$key.pub")"
fi
if ssh-keygen -F "$host" >/dev/null 2>&1; then
    printf 'KNOWN\t1\n'
    out=$(ssh -T -o BatchMode=yes -o ConnectTimeout=10 -o StrictHostKeyChecking=yes "git@$host" </dev/null 2>&1 | head -c 2000 | tr '\t\r\n' '   ')
    printf 'AUTH\t%s\n' "$out"
else
    printf 'KNOWN\t0\n'
fi
"#;
