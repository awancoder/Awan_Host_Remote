//! One cancellable SSH session, with bounded queues between networking and UI.
use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use russh::{ChannelMsg, client, keys};
use russh::keys::PublicKeyOrCertificate;
use tokio::sync::{mpsc, oneshot, watch};
use zeroize::Zeroizing;

use crate::model::Host;

pub struct Credentials {
    pub secret: Zeroizing<String>,
    pub key_path: String,
    pub use_saved: bool,
    pub remember: bool,
}

pub struct TrustPrompt {
    pub host: String,
    pub fingerprint: String,
    pub algorithm: String,
    pub answer: oneshot::Sender<bool>,
}

#[allow(dead_code)]
pub enum Event {
    Status(&'static str),
    Trust(TrustPrompt),
    Ready,
    ProfileSaved(Result<String, String>),
    Output(Vec<u8>),
    UpdateReady,
    UpdateOutput(Vec<u8>),
    UpdateEnd(Result<(), String>),
    Inventory(Result<crate::packages::Inventory, String>),
    File(Result<crate::remote_file::Reply, String>),
    Fs(Result<crate::remote_fs::Reply, String>),
    Git(Result<crate::git_auth::Reply, String>),
    Docker(Result<crate::docker_access::Access, String>),
    DockerList(Result<Vec<crate::docker::Container>, String>),
    DockerStats(Result<crate::docker::Stats, String>),
    DockerOutput(Vec<u8>),
    DockerEnd(Result<(), String>),
    /// Bytes of a backup written to the local file so far.
    DockerProgress(u64),
    /// Container name the query ran in, and its result.
    DockerDb(String, Result<crate::docker_db::Reply, String>),
    Live(crate::live::Kind, Result<crate::live::Report, String>),
    End(Result<(), String>),
}

#[allow(dead_code)]
pub enum Command {
    Input { update: bool, bytes: Vec<u8> },
    Maintain(crate::packages::Maintenance),
    InspectPackages,
    File(crate::remote_file::Request),
    Fs(crate::remote_fs::Request),
    Git(crate::git_auth::Request),
    DockerCheck,
    DockerList,
    DockerStats,
    DockerRun(crate::docker::Action),
    /// Runs a download action (backup) and writes its stdout to `local`.
    DockerDownload { action: crate::docker::Action, local: PathBuf },
    DockerCancel,
    DockerDb(crate::docker_db::Request),
    Live(crate::live::Request),
}

#[derive(Clone)]
struct Events {
    sender: mpsc::Sender<Event>,
}

impl Events {
    async fn send(&self, event: Event) -> Result<()> {
        self.sender.send(event).await.map_err(|_| anyhow::anyhow!("Sesi ditutup"))?;
        Ok(())
    }

    async fn output(&self, bytes: &[u8]) -> Result<()> {
        self.channel_output(bytes, false).await
    }

    async fn channel_output(&self, bytes: &[u8], update: bool) -> Result<()> {
        // At most 64 queued events of 4 KiB. Backpressure reaches the SSH channel.
        for chunk in bytes.chunks(4096) {
            self.send(if update { Event::UpdateOutput(chunk.to_vec()) } else { Event::Output(chunk.to_vec()) }).await?;
        }
        Ok(())
    }
}

pub struct Connection {
    events: Option<mpsc::Receiver<Event>>,
    pub input: mpsc::Sender<Command>,
    pub size: watch::Sender<(u16, u16)>,
    stop: Option<oneshot::Sender<()>>,
}

impl Connection {
    /// Takes the event receiver so a `cx.spawn` task can own it directly.
    /// Returns `None` if already taken.
    pub fn events_once(&mut self) -> Option<mpsc::Receiver<Event>> {
        self.events.take()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() { let _ = stop.send(()); }
    }
}

pub fn start(host: Host, credentials: Credentials, size: (u16, u16)) -> Result<Connection> {
    let (sender, events) = mpsc::channel(64);
    let (input, input_rx) = mpsc::channel(64);
    let (size, size_rx) = watch::channel(size);
    let (stop, stopped) = oneshot::channel();
    let output = Events { sender };
    std::thread::Builder::new().name("awan-ssh".into()).spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = output.sender.try_send(Event::End(Err(format!("Runtime SSH gagal: {error}"))));
                return;
            }
        };
        runtime.block_on(async {
            let result = tokio::select! {
                _ = stopped => Ok(()),
                result = run(host, credentials, input_rx, size_rx, output.clone()) => result,
            };
            let _ = output.send(Event::End(result.map_err(|error| format!("{error:#}")))).await;
        });
        // Dropping this session's runtime cancels russh tasks and closes the socket.
        runtime.shutdown_timeout(Duration::from_millis(500));
    }).context("Tidak dapat memulai worker SSH")?;
    Ok(Connection { events: Some(events), input, size, stop: Some(stop) })
}

struct Handler {
    host: String,
    port: u16,
    events: Events,
    known_hosts: PathBuf,
}

impl client::Handler for Handler {
    type Error = anyhow::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            bail!("Sertifikat host SSH belum didukung. Gunakan host key biasa.");
        };
        if keys::check_known_hosts_path(&self.host, self.port, key, &self.known_hosts)
            .context("Host key berubah atau berkas known_hosts tidak dapat dibaca. Verifikasi identitas server sebelum memperbarui catatan host")? {
            return Ok(true);
        }
        let (answer, response) = oneshot::channel();
        self.events.send(Event::Trust(TrustPrompt {
            host: format!("{}:{}", self.host, self.port),
            fingerprint: key.fingerprint(keys::HashAlg::Sha256).to_string(),
            algorithm: key.algorithm().to_string(), answer,
        })).await?;
        if !response.await.unwrap_or(false) { bail!("Identitas server tidak disetujui"); }
        keys::known_hosts::learn_known_hosts_path(&self.host, self.port, key, &self.known_hosts)
            .context("Tidak dapat menyimpan host key")?;
        Ok(true)
    }
}

fn known_hosts_path() -> Result<PathBuf> {
    #[cfg(windows)]
    let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let root = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    Ok(root.context("Direktori data pengguna tidak tersedia")?.join("awan-host-remote").join("known_hosts"))
}

async fn run(
    host: Host, mut credentials: Credentials, mut input: mpsc::Receiver<Command>,
    mut size: watch::Receiver<(u16, u16)>, events: Events,
) -> Result<()> {
    use tokio::time::timeout;
    if credentials.use_saved {
        events.send(Event::Status("Membaca login tersimpan...")).await?;
        let saved = crate::host_store::read(&host.address)?;
        if saved.host.user != host.user || saved.host.port != host.port || saved.host.key_auth != host.key_auth {
            bail!("Profil tersimpan sudah berubah. Buka ulang aplikasi untuk memuat daftar terbaru atau gunakan login manual.");
        }
        credentials.secret = saved.secret;
        credentials.key_path = saved.key_path;
    }
    events.send(Event::Status("Menghubungkan ke server...")).await?;
    let known_hosts = known_hosts_path()?;
    // Do not treat access-denied or malformed storage as an unknown host.
    match std::fs::read_to_string(&known_hosts) {
        Ok(_) => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
        Err(error) => return Err(error).context("Tidak dapat membaca known_hosts"),
    }
    let socket = timeout(Duration::from_secs(15), tokio::net::TcpStream::connect((host.address.as_str(), host.port)))
        .await.context("Koneksi TCP melewati batas 15 detik")?.context("Server tidak dapat dihubungi")?;
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(15)), keepalive_max: 3,
        window_size: 256 * 1024, maximum_packet_size: 32768, channel_buffer_size: 32,
        ..Default::default()
    });
    let handler = Handler { host: host.address.clone(), port: host.port, events: events.clone(), known_hosts };
    let mut session = timeout(Duration::from_secs(120), client::connect_stream(config, socket, handler))
        .await.context("Negosiasi SSH/verifikasi host melewati batas 120 detik")??;
    events.send(Event::Status("Mengautentikasi...")).await?;
    let authentication = async {
        if host.key_auth {
            let key_path = credentials.key_path.clone();
            let passphrase = credentials.secret.clone();
            let key = tokio::task::spawn_blocking(move || {
                let passphrase = (!passphrase.is_empty()).then_some(passphrase.as_str());
                keys::load_secret_key(&key_path, passphrase)
            }).await.context("Worker private key gagal")?
                .context("Private key tidak dapat dibaca; periksa path, format, dan passphrase")?;
            let hash = session.best_supported_rsa_hash().await?.flatten();
            session.authenticate_publickey(&host.user, keys::PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await.map_err(anyhow::Error::from)
        } else {
            session.authenticate_password(&host.user, credentials.secret.as_str()).await.map_err(anyhow::Error::from)
        }
    };
    let authenticated = timeout(Duration::from_secs(30), authentication).await.context("Autentikasi timeout")??;
    if !authenticated.success() { bail!("Autentikasi ditolak. Periksa username, password, atau private key. MFA/keyboard-interactive belum didukung."); }
    let mut channel = timeout(Duration::from_secs(10), session.channel_open_session()).await.context("Membuka sesi timeout")??;
    let (rows, cols) = *size.borrow_and_update();
    channel.request_pty(true, "xterm-256color", cols.into(), rows.into(), 0, 0, &[]).await?;
    timeout(Duration::from_secs(10), wait_success(&mut channel, &events, false)).await.context("Permintaan PTY timeout")??;
    channel.request_shell(true).await?;
    timeout(Duration::from_secs(10), wait_success(&mut channel, &events, false)).await.context("Permintaan shell timeout")??;
    if credentials.remember && !credentials.use_saved {
        let result = crate::host_store::save(&host, &credentials.key_path, credentials.secret.as_str())
            .map(|()| host.address.clone()).map_err(|error| format!("{error:#}"));
        events.send(Event::ProfileSaved(result)).await?;
    }
    drop(credentials);
    events.send(Event::Ready).await?;
    let (mut reader, writer) = channel.split();
    let mut exit_status = None;
    let mut update: Option<SystemUpdate> = None;
    let mut shell_closed = false;
    let mut inspection: Option<tokio::task::JoinHandle<()>> = None;
    let mut file_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut fs_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut git_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut docker_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut live_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut docker_list_task: Option<tokio::task::JoinHandle<()>> = None;
    // A Docker page action (logs -f can run indefinitely), stoppable from the
    // page. It is not waited for when the shell ends: the session closes it.
    let mut docker_run_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut docker_cancel: Option<oneshot::Sender<()>> = None;
    let mut docker_db_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut docker_stats_task: Option<tokio::task::JoinHandle<()>> = None;
    loop {
        if shell_closed && update.is_none() && file_task.is_none() && fs_task.is_none() && git_task.is_none() && docker_task.is_none() && docker_list_task.is_none() && docker_db_task.is_none() && docker_stats_task.is_none() { break; }
        tokio::select! {
            _ = async {
                match &mut file_task {
                    Some(task) => { let _ = task.await; }
                    None => std::future::pending::<()>().await,
                }
            } => { file_task = None; },
            _ = async {
                match &mut fs_task {
                    Some(task) => { let _ = task.await; }
                    None => std::future::pending::<()>().await,
                }
            } => { fs_task = None; },
            _ = async {
                match &mut git_task {
                    Some(task) => { let _ = task.await; }
                    None => std::future::pending::<()>().await,
                }
            } => { git_task = None; },
            _ = async {
                match &mut docker_task {
                    Some(task) => { let _ = task.await; }
                    None => std::future::pending::<()>().await,
                }
            } => { docker_task = None; },
            _ = async {
                match &mut docker_list_task {
                    Some(task) => { let _ = task.await; }
                    None => std::future::pending::<()>().await,
                }
            } => { docker_list_task = None; },
            _ = async {
                match &mut docker_db_task {
                    Some(task) => { let _ = task.await; }
                    None => std::future::pending::<()>().await,
                }
            } => { docker_db_task = None; },
            _ = async {
                match &mut docker_stats_task {
                    Some(task) => { let _ = task.await; }
                    None => std::future::pending::<()>().await,
                }
            } => { docker_stats_task = None; },
            command = input.recv() => match command {
                Some(Command::DockerStats) if docker_stats_task.as_ref().is_none_or(|task| task.is_finished()) => {
                    match timeout(Duration::from_secs(10), session.channel_open_session()).await {
                        Ok(Ok(mut channel)) => {
                            let stats_events = events.clone();
                            docker_stats_task = Some(tokio::spawn(async move {
                                let result = match timeout(Duration::from_secs(25), crate::docker::stats(&mut channel)).await {
                                    Ok(result) => result.map_err(|error| format!("{error:#}")),
                                    Err(_) => Err("Membaca statistik container melewati batas 25 detik.".into()),
                                };
                                let _ = timeout(Duration::from_secs(2), channel.close()).await;
                                let _ = stats_events.send(Event::DockerStats(result)).await;
                            }));
                        }
                        Ok(Err(error)) => events.send(Event::DockerStats(Err(format!("Gagal membuka channel Docker: {error}")))).await?,
                        Err(_) => events.send(Event::DockerStats(Err("Membuka channel Docker timeout.".into()))).await?,
                    }
                },
                // A sample is already being taken; its result answers this request too.
                Some(Command::DockerStats) => {},
                Some(Command::DockerDb(request)) if docker_db_task.as_ref().is_none_or(|task| task.is_finished()) => {
                    let container = request.target.container.clone();
                    match timeout(Duration::from_secs(10), session.channel_open_session()).await {
                        Ok(Ok(mut channel)) => {
                            let db_events = events.clone();
                            docker_db_task = Some(tokio::spawn(async move {
                                let result = match timeout(Duration::from_secs(45), crate::docker_db::execute(&mut channel, request)).await {
                                    Ok(result) => result.map_err(|error| format!("{error:#}")),
                                    Err(_) => Err("Query database melewati batas 45 detik.".into()),
                                };
                                let _ = timeout(Duration::from_secs(2), channel.close()).await;
                                let _ = db_events.send(Event::DockerDb(container, result)).await;
                            }));
                        }
                        Ok(Err(error)) => events.send(Event::DockerDb(container, Err(format!("Gagal membuka channel database: {error}")))).await?,
                        Err(_) => events.send(Event::DockerDb(container, Err("Membuka channel database timeout.".into()))).await?,
                    }
                },
                Some(Command::DockerDb(request)) => events.send(Event::DockerDb(request.target.container, Err("Query sebelumnya belum selesai.".into()))).await?,
                Some(Command::DockerList) if docker_list_task.as_ref().is_none_or(|task| task.is_finished()) => {
                    match timeout(Duration::from_secs(10), session.channel_open_session()).await {
                        Ok(Ok(mut channel)) => {
                            let list_events = events.clone();
                            docker_list_task = Some(tokio::spawn(async move {
                                let result = match timeout(Duration::from_secs(30), crate::docker::list(&mut channel)).await {
                                    Ok(result) => result.map_err(|error| format!("{error:#}")),
                                    Err(_) => Err("Membaca daftar container melewati batas 30 detik.".into()),
                                };
                                let _ = timeout(Duration::from_secs(2), channel.close()).await;
                                let _ = list_events.send(Event::DockerList(result)).await;
                            }));
                        }
                        Ok(Err(error)) => events.send(Event::DockerList(Err(format!("Gagal membuka channel Docker: {error}")))).await?,
                        Err(_) => events.send(Event::DockerList(Err("Membuka channel Docker timeout.".into()))).await?,
                    }
                },
                Some(Command::DockerList) => events.send(Event::DockerList(Err("Daftar container masih dimuat.".into()))).await?,
                Some(Command::DockerRun(action)) if docker_run_task.as_ref().is_none_or(|task| task.is_finished()) => {
                    match action.command() {
                        Err(error) => events.send(Event::DockerEnd(Err(format!("{error:#}")))).await?,
                        Ok(command) => match timeout(Duration::from_secs(10), session.channel_open_session()).await {
                            Ok(Ok(channel)) => {
                                let (cancel, cancelled) = oneshot::channel();
                                docker_cancel = Some(cancel);
                                docker_run_task = Some(tokio::spawn(run_docker(channel, command, cancelled, events.clone())));
                            }
                            Ok(Err(error)) => events.send(Event::DockerEnd(Err(format!("Gagal membuka channel Docker: {error}")))).await?,
                            Err(_) => events.send(Event::DockerEnd(Err("Membuka channel Docker timeout.".into()))).await?,
                        },
                    }
                },
                Some(Command::DockerRun(_)) => events.send(Event::DockerEnd(Err("Hentikan atau tunggu aksi Docker yang sedang berjalan.".into()))).await?,
                Some(Command::DockerDownload { action, local }) if docker_run_task.as_ref().is_none_or(|task| task.is_finished()) => {
                    match action.command() {
                        Err(error) => events.send(Event::DockerEnd(Err(format!("{error:#}")))).await?,
                        Ok(command) => match timeout(Duration::from_secs(10), session.channel_open_session()).await {
                            Ok(Ok(channel)) => {
                                let (cancel, cancelled) = oneshot::channel();
                                docker_cancel = Some(cancel);
                                docker_run_task = Some(tokio::spawn(download_docker(channel, command, local, cancelled, events.clone())));
                            }
                            Ok(Err(error)) => events.send(Event::DockerEnd(Err(format!("Gagal membuka channel Docker: {error}")))).await?,
                            Err(_) => events.send(Event::DockerEnd(Err("Membuka channel Docker timeout.".into()))).await?,
                        },
                    }
                },
                Some(Command::DockerDownload { .. }) => events.send(Event::DockerEnd(Err("Hentikan atau tunggu aksi Docker yang sedang berjalan.".into()))).await?,
                Some(Command::DockerCancel) => {
                    if let Some(cancel) = docker_cancel.take() { let _ = cancel.send(()); }
                },
                Some(Command::Live(request)) if live_task.as_ref().is_none_or(|task| task.is_finished()) => {
                    let kind = request.kind;
                    match timeout(Duration::from_secs(10), session.channel_open_session()).await {
                        Ok(Ok(mut channel)) => {
                            let live_events = events.clone();
                            live_task = Some(tokio::spawn(async move {
                                let result = match timeout(Duration::from_secs(55), crate::live::execute(&mut channel, request)).await {
                                    Ok(result) => result.map_err(|error| format!("{error:#}")),
                                    Err(_) => Err("Pembacaan server melewati batas 55 detik.".into()),
                                };
                                let _ = timeout(Duration::from_secs(2), channel.close()).await;
                                let _ = live_events.send(Event::Live(kind, result)).await;
                            }));
                        }
                        Ok(Err(error)) => events.send(Event::Live(kind, Err(format!("Gagal membuka channel pembacaan: {error}")))).await?,
                        Err(_) => events.send(Event::Live(kind, Err("Membuka channel pembacaan timeout.".into()))).await?,
                    }
                },
                Some(Command::Live(request)) => events.send(Event::Live(request.kind, Err("Pembacaan sebelumnya belum selesai; muat ulang.".into()))).await?,
                Some(Command::File(request)) if file_task.as_ref().is_none_or(|task| task.is_finished()) => {
                    match timeout(Duration::from_secs(10), session.channel_open_session()).await {
                        Ok(Ok(mut channel)) => {
                            let file_events = events.clone();
                            file_task = Some(tokio::spawn(async move {
                                let result = match timeout(Duration::from_secs(45), crate::remote_file::execute(&mut channel, request)).await {
                                    Ok(result) => result.map_err(|error| format!("{error:#}")),
                                    Err(_) => Err("Operasi file melewati batas 45 detik. Jika menyimpan, hasil belum pasti; periksa file di server sebelum mengulang.".into()),
                                };
                                let _ = timeout(Duration::from_secs(2), channel.close()).await;
                                let _ = file_events.send(Event::File(result)).await;
                            }));
                        }
                        Ok(Err(error)) => events.send(Event::File(Err(format!("Gagal membuka channel file: {error}")))).await?,
                        Err(_) => events.send(Event::File(Err("Membuka channel file timeout.".into()))).await?,
                    }
                },
                Some(Command::File(_)) => events.send(Event::File(Err("Tunggu operasi file sebelumnya selesai.".into()))).await?,
                Some(Command::Fs(request)) if fs_task.as_ref().is_none_or(|task| task.is_finished()) => {
                    match timeout(Duration::from_secs(10), session.channel_open_session()).await {
                        Ok(Ok(mut channel)) => {
                            let fs_events = events.clone();
                            fs_task = Some(tokio::spawn(async move {
                                let result = match timeout(Duration::from_secs(60), crate::remote_fs::execute(&mut channel, request)).await {
                                    Ok(result) => result.map_err(|error| format!("{error:#}")),
                                    Err(_) => Err("Operasi file melewati batas 60 detik. Hasil belum pasti; periksa server sebelum mengulang.".into()),
                                };
                                let _ = timeout(Duration::from_secs(2), channel.close()).await;
                                let _ = fs_events.send(Event::Fs(result)).await;
                            }));
                        }
                        Ok(Err(error)) => events.send(Event::Fs(Err(format!("Gagal membuka channel file: {error}")))).await?,
                        Err(_) => events.send(Event::Fs(Err("Membuka channel file timeout.".into()))).await?,
                    }
                },
                Some(Command::Fs(_)) => events.send(Event::Fs(Err("Tunggu operasi file sebelumnya selesai.".into()))).await?,
                Some(Command::Git(request)) if git_task.as_ref().is_none_or(|task| task.is_finished()) => {
                    match timeout(Duration::from_secs(10), session.channel_open_session()).await {
                        Ok(Ok(mut channel)) => {
                            let git_events = events.clone();
                            git_task = Some(tokio::spawn(async move {
                                let result = match timeout(Duration::from_secs(40), crate::git_auth::execute(&mut channel, request)).await {
                                    Ok(result) => result.map_err(|error| format!("{error:#}")),
                                    Err(_) => Err("Pemeriksaan login Git melewati batas 40 detik.".into()),
                                };
                                let _ = timeout(Duration::from_secs(2), channel.close()).await;
                                let _ = git_events.send(Event::Git(result)).await;
                            }));
                        }
                        Ok(Err(error)) => events.send(Event::Git(Err(format!("Gagal membuka channel login Git: {error}")))).await?,
                        Err(_) => events.send(Event::Git(Err("Membuka channel login Git timeout.".into()))).await?,
                    }
                },
                Some(Command::Git(_)) => events.send(Event::Git(Err("Tunggu pemeriksaan login Git sebelumnya selesai.".into()))).await?,
                Some(Command::DockerCheck) if docker_task.as_ref().is_none_or(|task| task.is_finished()) => {
                    match timeout(Duration::from_secs(10), session.channel_open_session()).await {
                        Ok(Ok(mut channel)) => {
                            let docker_events = events.clone();
                            docker_task = Some(tokio::spawn(async move {
                                let result = match timeout(Duration::from_secs(20), crate::docker_access::check(&mut channel)).await {
                                    Ok(result) => result.map_err(|error| format!("{error:#}")),
                                    Err(_) => Err("Pemeriksaan Docker melewati batas 20 detik.".into()),
                                };
                                let _ = timeout(Duration::from_secs(2), channel.close()).await;
                                let _ = docker_events.send(Event::Docker(result)).await;
                            }));
                        }
                        Ok(Err(error)) => events.send(Event::Docker(Err(format!("Gagal membuka channel pemeriksaan Docker: {error}")))).await?,
                        Err(_) => events.send(Event::Docker(Err("Membuka channel pemeriksaan Docker timeout.".into()))).await?,
                    }
                },
                Some(Command::DockerCheck) => events.send(Event::Docker(Err("Tunggu pemeriksaan Docker sebelumnya selesai.".into()))).await?,
                Some(Command::Input { update: false, bytes }) => {
                    if !shell_closed {
                        timeout(Duration::from_secs(15), writer.data(&bytes[..])).await.context("Pengiriman input timeout")??;
                    }
                },
                Some(Command::Input { update: true, bytes }) => {
                    if let Some(task) = &update {
                        timeout(Duration::from_secs(15), task.writer.data(&bytes[..])).await.context("Input operasi timeout")??;
                    }
                },
                Some(Command::Maintain(action)) if update.is_none() && inspection.as_ref().is_none_or(|task| task.is_finished()) => {
                    let dimensions = *size.borrow();
                    match start_update(&session, dimensions, &events, action).await {
                        Ok(task) => { update = Some(task); events.send(Event::UpdateReady).await?; }
                        Err(error) => events.send(Event::UpdateEnd(Err(format!("{error:#}")))).await?,
                    }
                },
                Some(Command::Maintain(_)) => {
                    events.send(Event::UpdateEnd(Err("Tunggu pemeriksaan atau operasi paket yang masih berjalan.".into()))).await?;
                },
                Some(Command::InspectPackages) if update.is_none() && inspection.as_ref().is_none_or(|task| task.is_finished()) => {
                    match start_inspection(&session, events.clone()).await {
                        Ok(task) => inspection = Some(task),
                        Err(error) => events.send(Event::Inventory(Err(format!("{error:#}")))).await?,
                    }
                },
                Some(Command::InspectPackages) => {
                    events.send(Event::Inventory(Err("Tunggu operasi paket yang sedang berlangsung sebelum memuat ulang.".into()))).await?;
                },
                None => break,
            },
            changed = size.changed() => {
                if changed.is_err() { break; }
                let (rows, cols) = *size.borrow_and_update();
                if !shell_closed {
                    timeout(Duration::from_secs(10), writer.window_change(cols.into(), rows.into(), 0, 0)).await.context("Resize PTY timeout")??;
                }
                if let Some(task) = &update {
                    timeout(Duration::from_secs(10), task.writer.window_change(cols.into(), rows.into(), 0, 0)).await.context("Resize terminal operasi timeout")??;
                }
            },
            message = async {
                match &mut update {
                    Some(task) => task.reader.wait().await,
                    None => std::future::pending::<Option<ChannelMsg>>().await,
                }
            } => {
                match message {
                    Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => events.channel_output(&data, true).await?,
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        if let Some(task) = &mut update { task.exit_status = Some(exit_status); }
                    },
                    Some(ChannelMsg::ExitSignal { signal_name, .. }) => {
                        if let Some(task) = update.take() {
                            let _ = timeout(Duration::from_secs(2), task.writer.close()).await;
                        }
                        events.send(Event::UpdateEnd(Err(format!("Operasi berhenti karena sinyal {signal_name:?}. Periksa hasil di terminal.")))).await?;
                    },
                    Some(ChannelMsg::Close) | None => {
                        let code = update.take().and_then(|task| task.exit_status);
                        let result = match code {
                            Some(0) => Ok(()),
                            Some(code) => Err(format!("Operasi gagal (exit code {code}). Periksa output terminal.")),
                            None => Err("Sesi operasi ditutup tanpa exit code; hasil belum dapat dipastikan.".into()),
                        };
                        events.send(Event::UpdateEnd(result)).await?;
                    },
                    _ => {},
                }
            },
            message = reader.wait(), if !shell_closed => match message {
                Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => events.output(&data).await?,
                Some(ChannelMsg::ExitStatus { exit_status: code }) => exit_status = Some(code),
                Some(ChannelMsg::ExitSignal { signal_name, .. }) => {
                    if update.is_none() && file_task.is_none() { bail!("Shell berakhir dengan sinyal {signal_name:?}"); }
                    shell_closed = true;
                    exit_status = Some(1);
                },
                Some(ChannelMsg::Close) | None => shell_closed = true,
                _ => {},
            },
        }
    }
    let _ = timeout(Duration::from_secs(2), session.disconnect(russh::Disconnect::ByApplication, "Sesi ditutup", "en")).await;
    if let Some(code) = exit_status.filter(|code| *code != 0) { bail!("Shell berakhir dengan exit code {code}"); }
    Ok(())
}

struct SystemUpdate {
    reader: russh::ChannelReadHalf,
    writer: russh::ChannelWriteHalf<client::Msg>,
    exit_status: Option<u32>,
}

async fn start_update(session: &client::Handle<Handler>, size: (u16, u16), events: &Events, action: crate::packages::Maintenance) -> Result<SystemUpdate> {
    use tokio::time::timeout;
    let command = action.command()?;
    let mut channel = timeout(Duration::from_secs(10), session.channel_open_session()).await.context("Membuka channel operasi timeout")??;
    let setup = async {
        channel.request_pty(true, "xterm-256color", size.1.into(), size.0.into(), 0, 0, &[]).await?;
        wait_success(&mut channel, events, true).await?;
        channel.exec(true, command).await?;
        wait_success(&mut channel, events, true).await
    };
    match timeout(Duration::from_secs(20), setup).await {
        Ok(Ok(())) => {},
        result => {
            let _ = timeout(Duration::from_secs(2), channel.close()).await;
            return match result {
                Ok(Err(error)) => Err(error),
                Err(_) => Err(anyhow::anyhow!("Pembukaan terminal operasi timeout; periksa server sebelum mencoba ulang.")),
                _ => unreachable!(),
            };
        }
    }
    let (reader, writer) = channel.split();
    Ok(SystemUpdate { reader, writer, exit_status: None })
}

/// Runs one Docker page action on its own exec channel (no PTY, so Compose
/// prints plain progress) and streams its output until it exits or the page
/// stops it. Stopping sends TERM, then closes the channel.
async fn run_docker(mut channel: russh::Channel<client::Msg>, command: String, mut cancelled: oneshot::Receiver<()>, events: Events) {
    use tokio::time::timeout;
    let result = stream_docker(&mut channel, command, &mut cancelled, &events).await;
    let _ = timeout(Duration::from_secs(2), channel.close()).await;
    let _ = events.send(Event::DockerEnd(result.map_err(|error| format!("{error:#}")))).await;
}

async fn stream_docker(channel: &mut russh::Channel<client::Msg>, command: String, cancelled: &mut oneshot::Receiver<()>, events: &Events) -> Result<()> {
    use tokio::time::timeout;
    timeout(Duration::from_secs(10), channel.exec(true, command)).await.context("Menjalankan perintah Docker timeout")??;
    let _ = channel.eof().await;
    let mut exit_status = None;
    loop {
        tokio::select! {
            _ = &mut *cancelled => {
                let _ = timeout(Duration::from_secs(2), channel.signal(russh::Sig::TERM)).await;
                bail!("Dihentikan.");
            },
            message = channel.wait() => match message {
                Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => {
                    for chunk in data.chunks(4096) {
                        events.send(Event::DockerOutput(chunk.to_vec())).await?;
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status: code }) => exit_status = Some(code),
                Some(ChannelMsg::ExitSignal { signal_name, .. }) => bail!("Perintah berhenti karena sinyal {signal_name:?}."),
                Some(ChannelMsg::Close) | None => break,
                _ => {}
            },
        }
    }
    match exit_status {
        Some(0) => Ok(()),
        Some(code) => bail!("Perintah gagal (exit code {code})."),
        None => bail!("Channel ditutup tanpa exit code; hasil belum dapat dipastikan."),
    }
}

/// Streams a backup's stdout into `<local>.part` (stderr goes to the output
/// panel), then renames it to `local` only when the dump exited 0. A failed,
/// cancelled, or interrupted dump never leaves a half file under the real name.
async fn download_docker(mut channel: russh::Channel<client::Msg>, command: String, local: PathBuf, mut cancelled: oneshot::Receiver<()>, events: Events) {
    use tokio::time::timeout;
    let mut partial = local.clone().into_os_string();
    partial.push(".part");
    let partial = PathBuf::from(partial);
    let result = stream_download(&mut channel, command, &partial, &mut cancelled, &events).await;
    let _ = timeout(Duration::from_secs(2), channel.close()).await;
    let result = match result {
        Ok(_) => std::fs::rename(&partial, &local).map_err(|error| {
            let _ = std::fs::remove_file(&partial);
            format!("Backup selesai tetapi file tidak dapat disimpan sebagai {}: {error}", local.display())
        }),
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            Err(format!("{error:#}"))
        }
    };
    let _ = events.send(Event::DockerEnd(result)).await;
}

async fn stream_download(channel: &mut russh::Channel<client::Msg>, command: String, partial: &std::path::Path, cancelled: &mut oneshot::Receiver<()>, events: &Events) -> Result<u64> {
    use std::io::Write;
    use tokio::time::timeout;
    let file = std::fs::File::create(partial).with_context(|| format!("Tidak dapat membuat file {}", partial.display()))?;
    let mut writer = std::io::BufWriter::with_capacity(256 * 1024, file);
    timeout(Duration::from_secs(10), channel.exec(true, command)).await.context("Menjalankan backup timeout")??;
    let _ = channel.eof().await;
    let (mut bytes, mut reported) = (0u64, 0u64);
    let mut exit_status = None;
    loop {
        tokio::select! {
            _ = &mut *cancelled => {
                let _ = timeout(Duration::from_secs(2), channel.signal(russh::Sig::TERM)).await;
                bail!("Dihentikan; file backup sementara dihapus.");
            },
            message = channel.wait() => match message {
                Some(ChannelMsg::Data { data }) => {
                    writer.write_all(&data).with_context(|| format!("Gagal menulis ke {}", partial.display()))?;
                    bytes += data.len() as u64;
                    if bytes - reported >= 1024 * 1024 {
                        reported = bytes;
                        events.send(Event::DockerProgress(bytes)).await?;
                    }
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => {
                    for chunk in data.chunks(4096) { events.send(Event::DockerOutput(chunk.to_vec())).await?; }
                }
                Some(ChannelMsg::ExitStatus { exit_status: code }) => exit_status = Some(code),
                Some(ChannelMsg::ExitSignal { signal_name, .. }) => bail!("Backup berhenti karena sinyal {signal_name:?}."),
                Some(ChannelMsg::Close) | None => break,
                _ => {}
            },
        }
    }
    writer.flush().context("Gagal menyimpan file backup")?;
    writer.into_inner().map_err(|error| anyhow::anyhow!("Gagal menyimpan file backup: {}", error.error()))?
        .sync_all().context("Gagal menyimpan file backup ke disk")?;
    events.send(Event::DockerProgress(bytes)).await?;
    match exit_status {
        Some(0) if bytes > 0 => Ok(bytes),
        Some(0) => bail!("Dump kosong; tidak ada yang disimpan."),
        Some(code) => bail!("Backup gagal (exit code {code}); lihat pesan di atas."),
        None => bail!("Channel ditutup tanpa exit code; backup dianggap gagal."),
    }
}

async fn start_inspection(session: &client::Handle<Handler>, events: Events) -> Result<tokio::task::JoinHandle<()>> {
    use tokio::time::timeout;
    let mut channel = timeout(Duration::from_secs(10), session.channel_open_session()).await.context("Membuka pemeriksaan paket timeout")??;
    Ok(tokio::spawn(async move {
        let result = match timeout(Duration::from_secs(30), read_inventory(&mut channel)).await {
            Ok(result) => result.map_err(|error| format!("{error:#}")),
            Err(_) => Err("Pemeriksaan paket melewati batas 30 detik.".into()),
        };
        let _ = timeout(Duration::from_secs(2), channel.close()).await;
        let _ = events.send(Event::Inventory(result)).await;
    }))
}

async fn read_inventory(channel: &mut russh::Channel<client::Msg>) -> Result<crate::packages::Inventory> {
    channel.exec(true, crate::packages::inventory_command()).await?;
    let mut output = Vec::new();
    let mut error_output = Vec::new();
    let mut exit_status = None;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => {
                if output.len() + data.len() > 4 * 1024 * 1024 { bail!("Inventaris melebihi batas 4 MiB"); }
                output.extend_from_slice(&data);
            }
            ChannelMsg::ExtendedData { data, .. } => {
                let remaining = 16_384usize.saturating_sub(error_output.len());
                error_output.extend_from_slice(&data[..data.len().min(remaining)]);
            }
            ChannelMsg::ExitStatus { exit_status: code } => exit_status = Some(code),
            ChannelMsg::Failure | ChannelMsg::ExitSignal { .. } => bail!("Server menolak atau menghentikan pemeriksaan paket"),
            ChannelMsg::Close => break,
            _ => {},
        }
    }
    if exit_status != Some(0) {
        bail!("Pemeriksaan paket gagal (status {exit_status:?}): {}", String::from_utf8_lossy(&error_output));
    }
    crate::packages::Inventory::parse(&output)
}

async fn wait_success(channel: &mut russh::Channel<client::Msg>, events: &Events, update: bool) -> Result<()> {
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => return Ok(()),
            Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => events.channel_output(&data, update).await?,
            Some(ChannelMsg::Failure | ChannelMsg::Close) | None => bail!("Server menolak PTY atau shell interaktif"),
            _ => {},
        }
    }
}
