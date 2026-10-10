//! Whether the SSH account can use Docker (and Docker Compose) on the server,
//! checked before running `docker compose` so a missing install or missing
//! `docker` group membership is handled in the GUI instead of failing in the
//! terminal with "permission denied ... docker.sock".
use anyhow::{Context, Result, bail};
use russh::{Channel, client};

pub struct Access {
    pub installed: bool,
    pub compose: bool,
    /// Root, listed in the `docker` group, or Docker already reachable.
    pub member: bool,
    pub user: String,
}

pub async fn check(channel: &mut Channel<client::Msg>) -> Result<Access> {
    channel.exec(true, format!("/bin/sh -c '{}'", SCRIPT.replace('\'', "'\"'\"'"))).await?;
    channel.eof().await?;
    let output = crate::remote_file::receive(channel).await?;
    let output = String::from_utf8(output).context("Respons pemeriksaan Docker tidak valid")?;
    let line = output.lines().find(|line| line.starts_with("AWAN_DOCKER_V1\t")).context("Respons pemeriksaan Docker tidak dikenali")?;
    let fields: Vec<&str> = line.split('\t').collect();
    let [_, installed, compose, member, user] = fields.as_slice() else { bail!("Respons pemeriksaan Docker tidak dikenali"); };
    let flag = |value: &str| value == "1";
    if user.is_empty() || user.len() > 64 || !user.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.$".contains(&c)) {
        bail!("Nama akun dari server tidak valid");
    }
    Ok(Access { installed: flag(*installed), compose: flag(*compose), member: flag(*member), user: (*user).to_owned() })
}

// Membership is read from /etc/group (the configured state), not `id -G`:
// a session that logged in before the user was added still lacks the group
// at runtime, which the typed command handles separately with `sg docker`.
const SCRIPT: &str = r#"user=$(id -un)
installed=0; compose=0; member=0
if command -v docker >/dev/null 2>&1; then
    installed=1
    docker compose version >/dev/null 2>&1 && compose=1
fi
if [ "$(id -u)" = 0 ]; then
    member=1
elif grep '^docker:' /etc/group 2>/dev/null | cut -d: -f4 | tr ',' '\n' | grep -qx "$user"; then
    member=1
elif [ "$installed" = 1 ] && docker info >/dev/null 2>&1; then
    member=1
fi
printf 'AWAN_DOCKER_V1\t%s\t%s\t%s\t%s\n' "$installed" "$compose" "$member" "$user"
"#;
