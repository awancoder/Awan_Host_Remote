//! SSH session lifecycle (connect/disconnect/status), independent of the
//! rendering widget. `src/app.rs` owns the `Entity<TerminalWidget>` (it
//! needs a GPUI `Context` to create one) and feeds it from
//! `Terminal::handle_ssh_event`'s `ssh::Event::Output` bytes.
use zeroize::Zeroize;

use crate::{model::Host, ssh};

pub struct Terminal {
    connection: Option<ssh::Connection>,
    pub update_running: bool,
    update_ready: bool,
    pub update_result: Option<String>,
    /// Whether the last maintenance operation ended with exit code 0 —
    /// `update_result` is display text only, so callers that act on the
    /// outcome (e.g. revealing a cloned folder) read this instead.
    pub update_ok: bool,
    pub task_title: String,
    pub inventory: Option<crate::packages::Inventory>,
    pub inventory_scanning: bool,
    pub package_error: Option<String>,
    pub file_busy: bool,
    pub live: crate::live::State,
    pub file_result: Option<Result<crate::remote_file::Reply, String>>,
    pub fs_busy: bool,
    pub fs_result: Option<Result<crate::remote_fs::Reply, String>>,
    pub git_busy: bool,
    pub git_result: Option<Result<crate::git_auth::Reply, String>>,
    pub docker_busy: bool,
    pub docker_result: Option<Result<crate::docker_access::Access, String>>,
    /// The Docker page: container list and the streamed output of one action.
    pub docker: crate::docker::State,
    /// The Database Browser page (opened from a DB container).
    pub db_browser: crate::docker_db::State,
    pub connected: bool,
    pub status: String,
    pub error: Option<String>,
    pub trust: Option<ssh::TrustPrompt>,
    pub key_path: String,
    pub secret: String,
    pub remember_login: bool,
    pub profile_saved: Option<Result<String, String>>,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            connection: None,
            update_running: false, update_ready: false, update_result: None, update_ok: false,
            task_title: "Update System".into(), inventory: None,
            inventory_scanning: false, package_error: None,
            file_busy: false, file_result: None,
            fs_busy: false, fs_result: None,
            git_busy: false, git_result: None,
            docker_busy: false, docker_result: None, docker: Default::default(),
            db_browser: Default::default(),
            live: crate::live::State::default(),
            connected: false, status: "Belum terhubung".into(), error: None,
            trust: None, key_path: String::new(), secret: String::new(),
            remember_login: cfg!(any(windows, target_os = "linux")), profile_saved: None,
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) { self.secret.zeroize(); }
}

/// Starting PTY size before the terminal widget has measured its own
/// on-screen size; the widget's `canvas` resize check corrects this as soon
/// as it paints (see `terminal_widget.rs`).
const INITIAL_SIZE: (u16, u16) = (24, 80);

impl Terminal {
    pub fn busy(&self) -> bool { self.connection.is_some() }

    pub fn disconnect(&mut self) {
        self.live = crate::live::State::default();
        if self.file_busy {
            self.file_result = Some(Err("SSH terputus saat operasi file. Draft tetap tersedia; jika sedang menyimpan, periksa hasil di server sebelum mencoba lagi.".into()));
            self.file_busy = false;
        }
        if self.fs_busy {
            self.fs_result = Some(Err("SSH terputus saat operasi file manager. Periksa hasil di server sebelum mencoba lagi.".into()));
            self.fs_busy = false;
        }
        if self.git_busy {
            self.git_result = Some(Err("SSH terputus saat memeriksa login Git.".into()));
            self.git_busy = false;
        }
        if self.docker_busy {
            self.docker_result = Some(Err("SSH terputus saat memeriksa akses Docker.".into()));
            self.docker_busy = false;
        }
        // Container data belongs to the host that just went away.
        self.docker = Default::default();
        self.db_browser = Default::default();
        if self.update_running {
            self.update_result = Some(format!("Koneksi terputus saat {}. Hasil belum dapat dipastikan; periksa server sebelum mengulang.", self.task_title));
        }
        self.update_running = false;
        self.update_ready = false;
        self.update_ok = false;
        self.inventory = None;
        self.inventory_scanning = false;
        self.connection = None;
        self.trust = None;
        self.connected = false;
        self.status = "Sesi terputus".into();
        self.secret.zeroize();
    }

    /// Starts the SSH session. On success, returns the size watch sender
    /// (for the terminal widget's resize hook) and the input sender (for
    /// forwarding the widget's output bytes) so the caller can wire up a
    /// `TerminalWidget`; the connection's event receiver is taken by the
    /// caller via [`Self::take_events`].
    pub fn connect(&mut self, host: &Host) -> Option<(tokio::sync::watch::Sender<(u16, u16)>, tokio::sync::mpsc::Sender<ssh::Command>)> {
        self.connect_mode(host, false)
    }

    pub fn connect_saved(&mut self, host: &Host) -> Option<(tokio::sync::watch::Sender<(u16, u16)>, tokio::sync::mpsc::Sender<ssh::Command>)> {
        self.connect_mode(host, true)
    }

    fn connect_mode(&mut self, host: &Host, use_saved: bool) -> Option<(tokio::sync::watch::Sender<(u16, u16)>, tokio::sync::mpsc::Sender<ssh::Command>)> {
        if self.busy() { return None; }
        if !use_saved && host.key_auth && self.key_path.trim().is_empty() {
            self.error = Some("Isi path private key terlebih dahulu.".into());
            return None;
        }
        if !use_saved && host.key_auth && !std::path::Path::new(self.key_path.trim()).is_absolute() {
            self.error = Some("Gunakan path absolut private key agar profil dapat dipakai kembali setelah aplikasi dibuka ulang.".into());
            return None;
        }
        let credentials = ssh::Credentials {
            secret: zeroize::Zeroizing::new(std::mem::take(&mut self.secret)),
            key_path: self.key_path.trim().to_owned(),
            use_saved, remember: self.remember_login,
        };
        self.error = None;
        self.trust = None;
        self.connected = false;
        match ssh::start(host.clone(), credentials, INITIAL_SIZE) {
            Ok(connection) => {
                let size = connection.size.clone();
                let input = connection.input.clone();
                self.connection = Some(connection);
                self.status = "Menghubungkan...".into();
                Some((size, input))
            }
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                self.status = "Koneksi gagal".into();
                None
            }
        }
    }

    /// Takes the connection's event receiver so the caller can drive a
    /// `cx.spawn` loop forwarding events into [`Self::handle_ssh_event`].
    /// Returns `None` once already taken for this connection.
    pub fn take_events(&mut self) -> Option<tokio::sync::mpsc::Receiver<ssh::Event>> {
        self.connection.as_mut().and_then(|connection| connection.events_once())
    }

    /// A `ssh::Event` arrived on the connection's event channel.
    pub fn handle_ssh_event(&mut self, event: ssh::Event) -> Option<Vec<u8>> {
        match event {
            ssh::Event::Status(status) => self.status = status.into(),
            ssh::Event::Trust(prompt) => {
                self.status = "Menunggu verifikasi identitas server".into();
                self.trust = Some(prompt);
            }
            ssh::Event::Ready => {
                self.connected = true;
                self.status = "SSH terhubung".into();
                self.refresh_packages();
            }
            ssh::Event::ProfileSaved(result) => self.profile_saved = Some(result),
            ssh::Event::Output(bytes) => return Some(bytes),
            // Routed into the same terminal widget as interactive shell
            // output, so a maintenance action's live progress (apt-get/dnf,
            // reboot, firewall changes...) is visible in the terminal panel
            // — including the persistent footer shown on every page — while
            // it runs, not just the pass/fail summary once it finishes.
            ssh::Event::UpdateOutput(bytes) => return Some(bytes),
            ssh::Event::UpdateReady => self.update_ready = true,
            ssh::Event::UpdateEnd(result) => {
                self.live.invalidate();
                self.update_running = false;
                self.update_ready = false;
                self.update_ok = result.is_ok();
                self.update_result = Some(match result {
                    Ok(()) => format!("{} selesai (exit code 0). Lihat log untuk rincian; status server akan diperiksa ulang saat halaman dibuka.", self.task_title),
                    Err(error) => format!("{}: {error}", self.task_title),
                });
                self.refresh_packages();
            }
            ssh::Event::Inventory(result) => {
                self.inventory_scanning = false;
                match result {
                    Ok(inventory) => { self.inventory = Some(inventory); self.package_error = None; }
                    Err(error) => { self.inventory = None; self.package_error = Some(error); }
                }
            }
            ssh::Event::File(result) => {
                self.file_busy = false;
                self.file_result = Some(result);
            }
            ssh::Event::Fs(result) => {
                self.fs_busy = false;
                self.fs_result = Some(result);
            }
            ssh::Event::Git(result) => {
                self.git_busy = false;
                self.git_result = Some(result);
            }
            ssh::Event::Docker(result) => {
                self.docker_busy = false;
                self.docker_result = Some(result);
            }
            ssh::Event::DockerList(result) => {
                self.docker.listing = false;
                match result {
                    Ok(containers) => { self.docker.containers = Some(containers); self.docker.list_error = None; }
                    Err(error) => { self.docker.containers = None; self.docker.list_error = Some(error); }
                }
            }
            ssh::Event::DockerStats(result) => {
                self.docker.stats_loading = false;
                self.docker.stats_at = Some(std::time::Instant::now());
                match result {
                    Ok(stats) => { self.docker.stats = Some(stats); self.docker.stats_error = None; }
                    Err(error) => self.docker.stats_error = Some(error),
                }
            }
            ssh::Event::DockerOutput(bytes) => self.docker.push_output(&bytes),
            ssh::Event::DockerProgress(bytes) => {
                self.docker.downloaded = Some(bytes);
                self.docker.revision += 1;
            }
            ssh::Event::DockerEnd(result) => {
                self.docker.running = false;
                self.docker.stopping = false;
                // A closed panel stays closed; drop the late tail of output.
                if self.docker.title.is_empty() {
                    self.docker.output.clear();
                } else {
                    if let (Ok(()), Some(path), Some(bytes)) = (&result, &self.docker.saved_to, self.docker.downloaded) {
                        let line = format!("[Awan] Backup tersimpan di komputer ini: {} ({})\n", path.display(), crate::docker::size_text(bytes));
                        self.docker.push_output(line.as_bytes());
                    }
                    self.docker.result = Some(result);
                }
                self.docker.revision += 1;
                // Restart/stop/up/down change container state; re-read it.
                self.request_docker_list();
                self.request_docker_stats();
            }
            ssh::Event::DockerDb(container, result) => {
                // A reply for a container the page has since switched away from.
                let follow_up = if self.db_browser.target.as_ref().is_some_and(|target| target.container == container) {
                    self.db_browser.finish(result)
                } else {
                    self.db_browser.loading = false;
                    self.db_browser.in_flight_sql = false;
                    Vec::new()
                };
                if let Some(query) = self.db_browser.queued.pop_front() { self.request_db(query); }
                for query in follow_up { self.request_db(query); }
            }
            ssh::Event::Live(kind, result) => self.live.finish(kind, result),
            ssh::Event::End(result) => {
                self.disconnect();
                if let Err(error) = result { self.error = Some(error); self.status = "Sesi gagal / terputus".into(); }
            }
        }
        None
    }

    pub fn can_maintain(&self) -> bool {
        self.connected && !self.update_running && !self.inventory_scanning && self.live.busy.is_none()
    }

    pub fn refresh_packages(&mut self) {
        if !self.can_maintain() { return; }
        let Some(connection) = &self.connection else { return; };
        self.inventory = None;
        self.package_error = None;
        if connection.input.try_send(ssh::Command::InspectPackages).is_ok() {
            self.inventory_scanning = true;
        } else {
            self.package_error = Some("Pemeriksaan belum dimulai: antrean penuh atau koneksi terputus.".into());
        }
    }

    // Reference for the Applications page port (plan: migrasi GUI ke
    // gpui-kit, langkah 8) for `Maintenance::Install`/`Remove`; Server
    // Control already drives `Maintenance::Server`/`SystemUpdate` through
    // this same method.
    pub fn start_maintenance(&mut self, action: crate::packages::Maintenance) -> bool {
        if !self.can_maintain() { return false; }
        if matches!(&action, crate::packages::Maintenance::Install(_) | crate::packages::Maintenance::Remove(_)) && self.inventory.is_none() {
            self.package_error = Some("Muat ulang inventaris sebelum mengubah paket.".into());
            return false;
        }
        let title = action.title();
        let Some(connection) = &self.connection else { return false; };
        if connection.input.try_send(ssh::Command::Maintain(action)).is_err() {
            self.error = Some("Operasi belum dimulai: koneksi terputus atau antrean penuh.".into());
            return false;
        }
        self.update_running = true;
        self.task_title = title;
        self.inventory = None;
        self.update_ready = false;
        self.update_result = None;
        self.update_ok = false;
        self.error = None;
        true
    }

    pub fn request_file(&mut self, request: crate::remote_file::Request) -> Result<(), String> {
        request.validate().map_err(|error| error.to_string())?;
        if !self.connected || self.file_busy { return Err("Hubungkan SSH dan tunggu operasi file selesai.".into()); }
        let connection = self.connection.as_ref().ok_or("SSH tidak tersedia")?;
        connection.input.try_send(ssh::Command::File(request)).map_err(|_| "Antrean SSH penuh atau koneksi terputus.".to_owned())?;
        self.file_result = None;
        self.file_busy = true;
        Ok(())
    }

    /// Independent from [`Self::request_file`] (its own SSH channel and busy
    /// flag) so a file-manager action — new file/folder, rename, move, copy,
    /// delete, import upload — never has to wait on, or block, the editor's
    /// own read/save of the currently open document.
    pub fn request_fs(&mut self, request: crate::remote_fs::Request) -> Result<(), String> {
        request.validate().map_err(|error| error.to_string())?;
        if !self.connected || self.fs_busy { return Err("Hubungkan SSH dan tunggu operasi sebelumnya selesai.".into()); }
        let connection = self.connection.as_ref().ok_or("SSH tidak tersedia")?;
        connection.input.try_send(ssh::Command::Fs(request)).map_err(|_| "Antrean SSH penuh atau koneksi terputus.".to_owned())?;
        self.fs_result = None;
        self.fs_busy = true;
        Ok(())
    }

    pub fn request_docker_check(&mut self) -> Result<(), String> {
        if !self.connected || self.docker_busy { return Err("Hubungkan SSH dan tunggu pemeriksaan Docker sebelumnya selesai.".into()); }
        let connection = self.connection.as_ref().ok_or("SSH tidak tersedia")?;
        connection.input.try_send(ssh::Command::DockerCheck).map_err(|_| "Antrean SSH penuh atau koneksi terputus.".to_owned())?;
        self.docker_result = None;
        self.docker_busy = true;
        Ok(())
    }

    pub fn request_docker_list(&mut self) {
        if !self.connected || self.docker.listing { return; }
        let Some(connection) = &self.connection else { return; };
        if connection.input.try_send(ssh::Command::DockerList).is_ok() {
            self.docker.listing = true;
        } else {
            self.docker.list_error = Some("Antrean SSH penuh atau koneksi terputus.".into());
        }
    }

    /// Takes one CPU/RAM sample of the running containers.
    pub fn request_docker_stats(&mut self) {
        if !self.connected || self.docker.stats_loading { return; }
        let Some(connection) = &self.connection else { return; };
        if connection.input.try_send(ssh::Command::DockerStats).is_ok() {
            self.docker.stats_loading = true;
        }
    }

    /// Runs a backup and saves its output to `local` on this computer.
    pub fn download_docker(&mut self, action: crate::docker::Action, local: std::path::PathBuf) -> Result<(), String> {
        if !self.connected || self.docker.running { return Err("Hubungkan SSH dan tunggu aksi Docker sebelumnya selesai.".into()); }
        action.command().map_err(|error| error.to_string())?;
        let title = action.title();
        let connection = self.connection.as_ref().ok_or("SSH tidak tersedia")?;
        connection.input.try_send(ssh::Command::DockerDownload { action, local: local.clone() })
            .map_err(|_| "Antrean SSH penuh atau koneksi terputus.".to_owned())?;
        self.docker.begin(title);
        self.docker.downloaded = Some(0);
        self.docker.saved_to = Some(local);
        Ok(())
    }

    pub fn run_docker(&mut self, action: crate::docker::Action) -> Result<(), String> {
        if !self.connected || self.docker.running { return Err("Hubungkan SSH dan tunggu aksi Docker sebelumnya selesai.".into()); }
        if action.is_download() { return Err("Backup perlu lokasi penyimpanan di komputer ini.".into()); }
        action.command().map_err(|error| error.to_string())?;
        let title = action.title();
        let follow = matches!(action, crate::docker::Action::Logs(_));
        let connection = self.connection.as_ref().ok_or("SSH tidak tersedia")?;
        connection.input.try_send(ssh::Command::DockerRun(action)).map_err(|_| "Antrean SSH penuh atau koneksi terputus.".to_owned())?;
        self.docker.begin(title);
        self.docker.follow = follow;
        Ok(())
    }

    /// Sends a Database Browser query; while one runs, the latest request
    /// waits and is sent when it finishes.
    pub fn request_db(&mut self, query: crate::docker_db::Query) {
        let Some(target) = self.db_browser.target.clone() else { return; };
        if !self.connected { return; }
        let sql = matches!(query, crate::docker_db::Query::Sql { .. });
        if sql {
            self.db_browser.sql_running = true;
            self.db_browser.sql_error = None;
        }
        if self.db_browser.loading {
            let queued = &mut self.db_browser.queued;
            if matches!(query, crate::docker_db::Query::Rows { .. }) {
                queued.retain(|waiting| !matches!(waiting, crate::docker_db::Query::Rows { .. }));
            }
            if queued.len() < 8 { queued.push_back(query); }
            return;
        }
        let Some(connection) = &self.connection else { return; };
        if connection.input.try_send(ssh::Command::DockerDb(crate::docker_db::Request { target, query })).is_ok() {
            self.db_browser.loading = true;
            self.db_browser.in_flight_sql = sql;
            if !sql { self.db_browser.error = None; }
        } else if sql {
            self.db_browser.sql_running = false;
            self.db_browser.sql_error = Some("Antrean SSH penuh atau koneksi terputus.".into());
        } else {
            self.db_browser.error = Some("Antrean SSH penuh atau koneksi terputus.".into());
        }
    }

    pub fn stop_docker(&mut self) {
        if !self.docker.running || self.docker.stopping { return; }
        let Some(connection) = &self.connection else { return; };
        if connection.input.try_send(ssh::Command::DockerCancel).is_ok() { self.docker.stopping = true; }
    }

    /// Hides the output panel; a log stream still running is stopped too.
    pub fn close_docker_output(&mut self) {
        self.stop_docker();
        let docker = &mut self.docker;
        docker.title.clear();
        docker.output.clear();
        docker.result = None;
        docker.follow = false;
    }

    pub fn request_git(&mut self, request: crate::git_auth::Request) -> Result<(), String> {
        if !self.connected || self.git_busy { return Err("Hubungkan SSH dan tunggu pemeriksaan login Git sebelumnya selesai.".into()); }
        let connection = self.connection.as_ref().ok_or("SSH tidak tersedia")?;
        connection.input.try_send(ssh::Command::Git(request)).map_err(|_| "Antrean SSH penuh atau koneksi terputus.".to_owned())?;
        self.git_result = None;
        self.git_busy = true;
        Ok(())
    }

    pub fn request_live(&mut self, request: crate::live::Request) {
        let kind = request.kind;
        if !self.connected || self.live.busy.is_some() || self.update_running { return; }
        let Some(connection) = &self.connection else { return; };
        if connection.input.try_send(ssh::Command::Live(request)).is_ok() {
            self.live.busy = Some(kind);
            self.live.errors[kind.index()] = None;
        } else {
            self.live.finish(kind, Err("Antrean SSH penuh atau koneksi terputus.".into()));
        }
    }
}
