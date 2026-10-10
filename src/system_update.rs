//! Fixed maintenance command; no user-supplied text is interpolated into it.
pub fn command() -> String {
    // `exec </dev/null` first: this (and everything it execs, including
    // dpkg/rpm package post-install scripts) can never read a prompt from
    // this channel anyway — the interactive shell is a separate channel —
    // so if anything still tries to ask a question, it gets immediate EOF
    // instead of hanging forever with `update_running` stuck true. Belt and
    // suspenders alongside the non-interactive flags below, which are what
    // should stop a prompt from being attempted at all.
    let script = r#"exec </dev/null
set -e
as_root() {
    if [ "$(id -u)" = 0 ]; then
        "$@"
    else
        sudo -- "$@"
    fi
}
if command -v apt-get >/dev/null 2>&1; then
    printf '\n[Awan] Memperbarui metadata APT...\n'
    as_root apt-get -o APT::Update::Error-Mode=any update
    printf '\n[Awan] Memperbarui paket terpasang...\n'
    as_root env DEBIAN_FRONTEND=noninteractive apt-get -y \
        -o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold upgrade
elif command -v dnf >/dev/null 2>&1; then
    printf '\n[Awan] Memperbarui paket menggunakan DNF...\n'
    as_root dnf upgrade --refresh -y
else
    printf '\n[Awan] Sistem belum didukung. Diperlukan APT atau DNF.\n' >&2
    exit 2
fi
printf '\n[Awan] Perintah update selesai. Tidak ada reboot otomatis.\n'
if [ -f /var/run/reboot-required ]; then
    printf '[Awan] Server melaporkan bahwa reboot diperlukan.\n'
fi
"#;
    format!("/bin/sh -c '{}'", script.replace('\'', "'\"'\"'"))
}
