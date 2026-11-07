# Awan Host Manager

**English** | [Bahasa Indonesia](README_id.md)

A **native Rust** desktop application to connect to and manage Linux servers over SSH: interactive terminal, remote file manager and editor, Docker, container databases, firewall, and package installation - all from a single window with no Electron, WebView, or browser.

> **🤖 Built 100% with AI assistance.**
> All application code - architecture, UI, SSH handling, server scripts, and this documentation - was written by an AI coding assistant. The project owner directs features, reviews results, builds, and tests manually. The code has not been independently audited; review and test before using on production servers.

## Features

### Hosts and Connections

- Add hosts with a friendly name, IP/hostname, SSH port, username, and **password** or **private key** authentication (with passphrase support).
- **Host key verification**: new hosts prompt with their SHA-256 fingerprint for verification; changed host keys are rejected.
- **Saved logins** in Windows Credential Manager or the Linux desktop keyring (GNOME Keyring / KWallet via Secret Service), never in plaintext files, strictly one profile per IP address. Failed sign-ins never overwrite existing saved logins.
- Multi-host support: multiple hosts can remain connected concurrently; the sidebar displays each active host and its navigation tabs.

### Summary

Server identity (hostname, OS, kernel, uptime), real-time CPU, RAM, and disk usage, plus confirmed **Update System**, **Reboot**, and **Shutdown** actions.

### Terminal

- Full interactive terminal emulator powered by `alacritty_terminal`: ANSI colors, keyboard navigation, PTY resizing, vim, htop, and fullscreen TUI apps.
- Persistent bottom terminal dock on every page: draggable height, full-window toggle, and right-click Copy/Paste context menu.

### File Editor

- Hierarchical **remote folder Explorer**; click blank space to return focus to the Home folder.
- Create files/folders, **import** local files or folders (up to 25 MiB per file), copy, paste, rename, **move** (destination picker), delete, and **chmod** (Owner/Group/Other × Read/Write/Execute grid, presets, recursive).
- Text editor with syntax highlighting, dirty buffer tracking, and conflict detection before saving.
- **Markdown preview** for `.md` files.
- **Git integration**: clone repositories, SSH Git sign-in guide (GitHub/GitLab) displaying current username, and Status/Pull/Push for repository folders.
- Open folder directly in terminal, and run `docker compose up -d --build` directly from Compose files.

### Docker

- Container list grouped by **Compose project**, with status, image, and exposed ports.
- **CPU and RAM utilization** per container, overall total, and sortable by highest CPU or RAM to quickly identify resource-heavy containers.
- Actions: stream **logs** (follow), **restart**, **stop/start**, **delete/remove** stopped containers (with safety confirmation dialog), **exec** commands, **reload** Caddy/Nginx configuration, **rebuild** per service or entire project, **down**, and **create network** (e.g. `edge`). Destructive actions always require confirmation.
- **Database backups** (MariaDB/MySQL/PostgreSQL) **downloaded directly to your local computer** - zero temporary dump files left on the server. Credentials are extracted safely from the container's environment variables.
- One-click guidance if the SSH user is not yet part of the `docker` group.

### Database Browser

Accessed directly from the table action button on any database container on the Docker page.

- Left pane: hierarchical tree of **databases → tables** with estimated row counts.
- Right pane, **Data** tab: virtualized table view (resizable columns, Shift + scroll for horizontal scrolling) with 100-row pagination.
- **SQL** tab: query editor with templates (SELECT, CREATE TABLE, INSERT, UPDATE, DELETE). SELECT queries run in a read-only transaction; **DDL/DML operations always prompt for confirmation**, with distinct warnings for DROP/TRUNCATE/DELETE/ALTER. On PostgreSQL, queries execute inside a single transaction.

### Firewall

Status of UFW/firewalld, currently listening ports, and add/delete port rules with confirmations and warnings if a change might cut off the active SSH connection.

### Applications

Preconfigured installation catalog (Nginx, Docker Engine, Docker Compose, Git, Node.js, Redis, PostgreSQL) and searchable installed packages inventory, with install/uninstall via APT or DNF and live execution logs.

### Settings

Dark/light theme toggle, interface language switcher (**English / Indonesia**) with native country flag icons, and direct quick links to **Awan Apps** and **Contribute**.

## Guiding Principles

- **Real data only**: every page queries the connected live server; no demo hosts, dummy metrics, or simulated fixtures. Unavailable metrics are presented transparently with failure reasons.
- **No plaintext credentials**: SSH logins live in Windows Credential Manager or the OS keyring; database passwords are never typed, displayed, logged, or stored by the application.
- **Confirmation for destructive actions**: reboot, shutdown, container stop/restart/rebuild/down/remove, firewall edits, file deletions, and state-modifying database queries.
- **Lightweight & responsive**: all network operations run asynchronously in background tasks over dedicated SSH channels, buffers and outputs are bounded, large tables render only visible rows, and polling halts immediately when a tab is inactive or a host disconnects.

## Requirements

**Local Machine**

- Rust **1.89+** (edition 2024) and platform-native toolchain. On Windows with MSVC: Microsoft C++ Build Tools and Windows SDK.
- On Ubuntu/Linux: Vulkan drivers and X11/Wayland development libraries (see [Ubuntu / Linux](#ubuntu--linux)).
- Saved logins available on **Windows** (Credential Manager) and **Linux** (desktop keyring via Secret Service). Without a running keyring, or on macOS, manual sign-in is always available.

**Server**

- Linux server accessible via SSH.
- **Python 3** for Summary, Firewall, and remote filesystem helper routines.
- **Docker** (and Docker Compose) for Docker management; `mariadb`/`mysql` client or `psql` inside database containers for the Database Browser and database dumps.
- APT or DNF for the Applications page. Root-level operations use `sudo`.

The application never installs tools on the server automatically; packages are only installed upon explicit user request on the Applications page.

## Building & Running

```powershell
cargo run            # build debug and launch
cargo run --release  # optimized release build
```

The first run downloads dependencies and compiles everything, which may take several minutes.

### Ubuntu / Linux

Build natively on Ubuntu (or WSL2 with WSLg); cross-compilation from Windows is not supported.

```bash
sudo apt install build-essential pkg-config clang cmake \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libx11-xcb-dev libxcb1-dev libfontconfig-dev libfreetype-dev \
  libvulkan1 mesa-vulkan-drivers
cargo run --release
```

Saved logins require an active desktop keyring (`gnome-keyring` or KWallet); if the keyring is locked, the system prompts you to unlock it.

Build a `.deb` package (includes executable, desktop application menu entry, and icon):

```bash
cargo install cargo-deb
cargo deb                                   # output: target/debian/awan-host-manager_<version>_amd64.deb
sudo apt install ./target/debian/awan-host-manager_*.deb
```

## Technology Stack

| Component               | Library                                                                            |
| ----------------------- | ---------------------------------------------------------------------------------- |
| Native UI               | [gpui-kit](https://crates.io/crates/gpui-kit) (gpui + gpui-component, Lucide icons) |
| SSH                     | [russh](https://crates.io/crates/russh)                                            |
| Async Runtime           | [Tokio](https://tokio.rs)                                                          |
| Terminal Emulator       | [alacritty_terminal](https://crates.io/crates/alacritty_terminal)                  |
| In-memory Secret Erasure| [zeroize](https://crates.io/crates/zeroize)                                        |

## Project Structure

| File / Folder                             | Purpose                                                                                          |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `src/main.rs`                             | Application entry point and window creation.                                                     |
| `src/app.rs`                              | Complete UI views: sidebar, pages, modal dialogs, and navigation logic.                          |
| `src/model.rs`                            | Page routing and host connection models.                                                         |
| `src/i18n.rs`                             | Bilingual Indonesian & English UI strings and SVG icons.                                         |
| `src/ssh.rs`                              | SSH sessions, host key verification, PTY allocation, and background channels.                    |
| `src/terminal.rs`                         | Per-host terminal state and command queue to the SSH backend.                                    |
| `src/terminal_widget.rs`                  | Terminal rendering widget (grid rendering, keyboard handling, copy/paste).                       |
| `src/host_store.rs`                       | Address normalization and credential storage via Windows Credential Manager / Secret Service.    |
| `src/live.rs`, `src/live_remote.py`       | Live Summary and Firewall telemetry gathering and remote actions.                               |
| `src/remote_file.rs`                      | Remote file read/write with revision tracking and atomic writes.                                 |
| `src/remote_directory.rs`, `.py`          | Remote folder directory listing helper.                                                          |
| `src/remote_fs.rs`                        | File manager operations: create, delete, rename, copy, upload, chmod.                            |
| `src/git_auth.rs`                         | Git SSH authentication assistant.                                                                |
| `src/packages.rs`, `src/system_update.rs` | Package manager detection, install/uninstall, system upgrade, git clone.                         |
| `src/docker.rs`                           | Container inspection, CPU/RAM telemetry, and container lifecycle actions.                       |
| `src/docker_access.rs`                    | Docker permission check (`docker` group membership).                                             |
| `src/docker_db.rs`                        | In-container database query engine for Database Browser.                                         |
| `build.rs`                                | 8 MB main thread stack configuration, application icons, and Windows version metadata.          |
| `packaging/linux/`                        | Desktop entry (`.desktop`) configuration for `.deb` package generation.                          |

## Project Status

Under active development. Features are AI-generated and manually tested by the project owner; there is currently no automated test suite. Always test against non-critical servers when performing system-altering operations (firewall rules, package updates, Docker management, or DDL/DML queries).
