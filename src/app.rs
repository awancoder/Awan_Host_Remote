use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable, WindowExt, h_flex, v_flex,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle},
    form::{Field, Form},
    input::{Input, InputState},
    menu::{ContextMenuExt, PopupMenu, PopupMenuItem},
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    skeleton::Skeleton,
    spinner::Spinner,
    table::{Column, DataTable, TableDelegate, TableState},
    tag::Tag,
    toolbar::Toolbar,
};
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::base::StyledExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::i18n::{self, Language};
use crate::model::{Host, HostForm, Page};
use crate::{host_store, ssh, terminal_widget::TerminalWidget};

/// One host's live SSH state: its shell session (connection, status, live
/// Summary/Control/Ports/Database data — all nested inside `Terminal`
/// already) plus the terminal widget rendering its PTY. Kept per host
/// (keyed by the host's normalized address, the same stable identity
/// `host_store` uses) so connecting to a second host does not tear down
/// the first — only an explicit Disconnect, or deleting the host profile,
/// does that.
#[derive(Default)]
struct Session {
    terminal: crate::terminal::Terminal,
    widget: Option<Entity<TerminalWidget>>,
}

/// Source entry for a pending right-click Copy/Paste on the Explorer tree.
/// Kept separate from the OS clipboard, which the text editor's own
/// copy/paste already uses for file *contents*.
struct FsClipboardEntry {
    path: String,
    is_dir: bool,
}

/// What the shared `fs_name_input` dialog is currently being used for — New
/// File/Folder need the parent folder the name goes under; Rename/Move need
/// the source path being renamed/relocated.
enum FsPending {
    NewFile { parent: String },
    NewFolder { parent: String },
    Rename { from: String },
    Move { from: String },
}

/// The Move dialog's folder picker: the item being moved, the folder chosen
/// as its destination, and which folders are open in the picker (separate
/// from the Explorer's own expanded set; both share the cached listings).
struct MovePicker {
    from: String,
    dest: String,
    expanded: std::collections::HashSet<String>,
}

/// The Chmod dialog: the entry, its permission bits when opened, and the
/// bits being edited.
struct ChmodUi {
    path: String,
    is_dir: bool,
    original: u32,
    mode: u32,
    recursive: bool,
}

enum MoveRow {
    Folder { path: String, name: String, depth: usize, expanded: bool },
    /// Under an open folder: still loading, or it has no subfolders.
    Note { depth: usize, loading: bool },
}

/// Git-over-SSH login wizard state for the host being viewed. The "busy"
/// flag lives on `Terminal::git_busy`, next to its SSH channel.
#[derive(Clone, Default)]
struct GitAuthUi {
    provider: crate::git_auth::Provider,
    status: Option<crate::git_auth::Status>,
    /// Host key fingerprint fetched for the user to compare before trusting.
    fingerprint: Option<String>,
    error: Option<String>,
}

/// One step of a queued Import (a whole folder, or multiple picked files):
/// either a remote folder that must exist before the files under it upload,
/// or one local file to upload to a remote path. Processed one at a time
/// since the fs channel only runs a single operation at once.
enum ImportTask {
    EnsureDir(String),
    UploadFile { remote: String, local: std::path::PathBuf },
}

pub struct AwanApp {
    page: Page,
    /// UI display language. Covers every string this file renders; backend
    /// status/error text (ssh.rs, host_store.rs, packages.rs, live.rs,
    /// remote_file.rs, remote_directory.rs, terminal.rs) is not yet routed
    /// through the catalog — see the note on `terminal.status` below.
    lang: Language,
    hosts: Vec<Host>,
    /// Which host's page is currently being viewed — independent of which
    /// hosts are connected (see [`Session`]/[`Self::sessions`]).
    active: Option<usize>,
    notice: Option<String>,
    /// Keyed by `Host::address` (stable across list reordering/deletion,
    /// unlike a `Vec` index). Only entries for hosts that have connected at
    /// least once exist here; [`Self::terminal`]/[`Self::terminal_mut`] fall
    /// back to `empty_session` when the active host has none yet.
    sessions: std::collections::HashMap<String, Session>,
    empty_session: Session,
    host_store: host_store::Store,
    host_store_message: Option<String>,
    /// Which connected hosts' Workspace page lists are expanded in the
    /// sidebar — independent of `active`/`page`: clicking a host row only
    /// expands/collapses its own menu (any number of hosts can be expanded
    /// at once) so the user can see the options (or close them) without
    /// jumping the main panel to that host's Summary. Only clicking an
    /// actual page item underneath switches `active`/`page`.
    sidebar_expanded: std::collections::HashSet<usize>,
    /// Whether `poll_live_page`'s timer may fetch Summary/Server
    /// Control/Ports data automatically. Off just stops the timer; the
    /// manual Reload button in `live_status_row` still works either way.
    live_auto_refresh: bool,
    /// Address of the host `select_host` is waiting to finish connecting,
    /// so the event loop in `start_session` knows to reveal its workspace
    /// (sidebar entry + switch to Summary) the moment it reports `Ready`,
    /// instead of exposing a half-connected session immediately.
    connecting_focus: Option<String>,
    /// Whether the persistent terminal panel is shown at all. Closing it
    /// (its own X button) leaves a slim reopen strip instead of reserving
    /// the panel's full height for nothing.
    terminal_footer_open: bool,
    /// Whether the terminal panel is expanded to fill the whole content
    /// area in place of the page body, instead of sharing a resizable
    /// split with it.
    terminal_footer_fullscreen: bool,
    /// Height of the docked terminal panel; changed by dragging its tab bar.
    terminal_footer_height: Pixels,
    /// `(mouse y, panel height)` when a tab-bar drag started; `None` when
    /// not dragging.
    terminal_footer_drag: Option<(Pixels, Pixels)>,

    host_name: Entity<InputState>,
    host_address: Entity<InputState>,
    host_user: Entity<InputState>,
    host_port: Entity<InputState>,
    host_key_auth: bool,
    host_form_error: Option<String>,

    key_path: Entity<InputState>,
    secret: Entity<InputState>,

    firewall_engine: String,
    firewall_port: Entity<InputState>,
    firewall_udp: bool,

    package_search: Entity<InputState>,
    /// 0 = install catalog, 1 = all installed packages.
    package_tab: usize,
    /// The installed-packages list is virtualized (hundreds of rows), so it
    /// scrolls on its own handle instead of the page's scroll area.
    package_list_scroll: UniformListScrollHandle,

    explorer_initialized: bool,
    explorer_owner: String,
    explorer_error: Option<String>,
    /// Resolved home path shown as the tree's root; starts empty until the
    /// server resolves `~` on the first listing.
    explorer_root: String,
    /// Cached listing per directory path, populated lazily as branches are
    /// expanded (fetching the whole hierarchy upfront is not possible: a
    /// listing is only known once its own `Request::List` reply arrives).
    explorer_nodes: std::collections::HashMap<String, crate::remote_directory::Directory>,
    /// Which directory paths are expanded in the tree.
    explorer_expanded: std::collections::HashSet<String>,
    /// The path most recently requested via `Request::List`, so the reply
    /// (or its failure) can be routed back without guessing.
    explorer_pending: Option<String>,
    explorer_query: Entity<InputState>,
    explorer_show_hidden: bool,
    /// The last clicked tree row `(path, is_dir)`. The toolbar's New file /
    /// New folder / Import / Git clone / Paste act on this folder (or the
    /// clicked file's folder) instead of always the root.
    explorer_selected: Option<(String, bool)>,
    /// Set right before sending a `Request::List`; tells `handle_file_result`
    /// whether an error reply belongs to the explorer panel or the editor.
    explorer_browsing: bool,

    editor_path: String,
    editor_host: String,
    editor_pending_host: String,
    editor_path_input: Entity<InputState>,
    editor_text: Entity<gpui_kit::component::input::EditorState>,
    editor_original: String,
    editor_revision: Option<String>,
    editor_message: Option<String>,
    editor_failed: bool,
    editor_saving: bool,
    /// Markdown files: show the rendered preview instead of the text editor.
    editor_preview: bool,

    fs_clipboard: Option<FsClipboardEntry>,
    fs_pending: Option<FsPending>,
    move_picker: Option<MovePicker>,
    chmod_ui: Option<ChmodUi>,
    fs_name_input: Entity<InputState>,
    /// Last file-manager action's (or Import batch's) result/error, shown in
    /// the Explorer panel until the next action replaces it.
    fs_message: Option<String>,
    /// Whether `fs_message` reports a failure (rendered as a red notice)
    /// rather than a neutral/success line.
    fs_message_error: bool,
    import_queue: Vec<ImportTask>,
    import_total: usize,
    import_failed: usize,
    /// Folder paths to expand (and whose cache to drop) once the Import
    /// batch finishes, so the newly created folder(s) are immediately
    /// visible instead of sitting collapsed somewhere in a long listing.
    import_reveal: Vec<String>,
    git_url_input: Entity<InputState>,
    git_dir_input: Entity<InputState>,
    /// `(parent folder, cloned folder)` of a running Git clone, so the
    /// Explorer can reveal the result once the maintenance channel finishes.
    git_clone_target: Option<(String, String)>,
    git_auth: GitAuthUi,
    /// Compose file waiting on the background Docker access check before
    /// its confirmation dialog opens.
    compose_pending: Option<String>,

    docker_search: Entity<InputState>,
    /// Command typed in the Docker page's Exec dialog.
    docker_exec_input: Entity<InputState>,
    docker_network_input: Entity<InputState>,
    docker_output_scroll: ScrollHandle,
    /// Last `docker::State::revision` scrolled to, so new output keeps the
    /// log pinned to the bottom without re-scrolling on every frame.
    docker_seen_revision: u64,
    /// Docker page order: 0 = by Compose project, 1 = highest CPU first,
    /// 2 = highest RAM first (flat list, to spot what uses the most).
    docker_sort: u8,
    /// Database Browser data grid (virtualized, resizable columns).
    db_grid: Entity<TableState<DbGrid>>,
    /// Last `docker_db::State::revision` loaded into `db_grid`.
    db_grid_revision: u64,
    /// Database Browser right panel: 0 = table data, 1 = SQL editor.
    db_tab: usize,
    db_sql_editor: Entity<gpui_kit::component::input::EditorState>,
    db_sql_grid: Entity<TableState<DbGrid>>,
    db_sql_revision: u64,

    _subscriptions: Vec<Subscription>,
}

impl AwanApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (host_store, events) = host_store::Store::new();
        cx.spawn(async move |this, cx| {
            let mut events = events;
            loop {
                match events.recv().await {
                    Some(event) => {
                        if this.update(cx, |app, cx| { app.handle_host_store_event(event); cx.notify(); }).is_err() { break; }
                    }
                    None => break,
                }
            }
        }).detach();

        // Refreshes whichever live page (Summary/Server Control/Ports) is
        // currently open, on its own interval, instead of a page needing to
        // kick off its own timer each time it becomes visible.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_secs(1)).await;
                if this.update(cx, |app, cx| app.poll_live_page(cx)).is_err() { break; }
            }
        }).detach();

        let lang = Language::load_saved().unwrap_or_else(Language::detect_system);
        let t = i18n::strings(lang);

        Self {
            page: Page::Hosts, lang, hosts: Vec::new(), active: None, notice: None,
            sessions: std::collections::HashMap::new(), empty_session: Session::default(),
            host_store, host_store_message: None, sidebar_expanded: std::collections::HashSet::new(), live_auto_refresh: false,
            connecting_focus: None,
            terminal_footer_open: false, terminal_footer_fullscreen: false,
            terminal_footer_height: px(240.), terminal_footer_drag: None,
            host_name: cx.new(|cx| InputState::new(window, cx).placeholder(t.host_name_placeholder)),
            host_address: cx.new(|cx| InputState::new(window, cx).placeholder("192.0.2.10")),
            host_user: cx.new(|cx| InputState::new(window, cx).placeholder("ubuntu").default_value("ubuntu")),
            host_port: cx.new(|cx| InputState::new(window, cx).placeholder("22").default_value("22")),
            host_key_auth: true,
            host_form_error: None,
            key_path: cx.new(|cx| InputState::new(window, cx).placeholder(t.host_key_path_placeholder)),
            secret: cx.new(|cx| InputState::new(window, cx).placeholder(t.host_secret_placeholder).masked(true)),
            firewall_engine: "ufw".into(),
            firewall_port: cx.new(|cx| InputState::new(window, cx).placeholder(t.db_field_port)),
            firewall_udp: false,
            package_search: cx.new(|cx| InputState::new(window, cx).placeholder(t.tools_search_placeholder)),
            package_tab: 0,
            package_list_scroll: UniformListScrollHandle::new(),
            explorer_initialized: false,
            explorer_owner: String::new(),
            explorer_error: None,
            explorer_root: String::new(),
            explorer_nodes: std::collections::HashMap::new(),
            explorer_expanded: std::collections::HashSet::new(),
            explorer_pending: None,
            explorer_query: cx.new(|cx| InputState::new(window, cx).placeholder(t.editor_search_placeholder)),
            explorer_show_hidden: false,
            explorer_selected: None,
            explorer_browsing: false,
            editor_path: String::new(),
            editor_host: String::new(),
            editor_pending_host: String::new(),
            editor_path_input: cx.new(|cx| InputState::new(window, cx).placeholder(t.editor_path_placeholder)),
            editor_text: cx.new(|cx| gpui_kit::component::input::EditorState::new(window, cx).language("plaintext")),
            editor_original: String::new(),
            editor_revision: None,
            editor_message: None,
            editor_failed: false,
            editor_saving: false,
            editor_preview: false,
            fs_clipboard: None,
            fs_pending: None,
            move_picker: None,
            chmod_ui: None,
            fs_name_input: cx.new(|cx| InputState::new(window, cx)),
            fs_message: None,
            fs_message_error: false,
            import_queue: Vec::new(),
            import_total: 0,
            import_failed: 0,
            import_reveal: Vec::new(),
            git_url_input: cx.new(|cx| InputState::new(window, cx).placeholder(t.git_clone_url_placeholder)),
            git_dir_input: cx.new(|cx| InputState::new(window, cx).placeholder(t.git_clone_dir_placeholder)),
            git_clone_target: None,
            git_auth: GitAuthUi::default(),
            compose_pending: None,
            docker_search: cx.new(|cx| InputState::new(window, cx).placeholder(t.docker_search_placeholder)),
            docker_exec_input: cx.new(|cx| InputState::new(window, cx).placeholder("ls -la /app")),
            docker_network_input: cx.new(|cx| InputState::new(window, cx).placeholder("edge").default_value("edge")),
            docker_output_scroll: ScrollHandle::new(),
            docker_seen_revision: 0,
            docker_sort: 0,
            db_grid: cx.new(|cx| TableState::new(DbGrid::default(), window, cx).sortable(false).col_movable(false).cell_selectable(true)),
            db_grid_revision: 0,
            db_tab: 0,
            db_sql_editor: cx.new(|cx| gpui_kit::component::input::EditorState::new(window, cx).language("sql")),
            db_sql_grid: cx.new(|cx| TableState::new(DbGrid::default(), window, cx).sortable(false).col_movable(false).cell_selectable(true)),
            db_sql_revision: 0,
            _subscriptions: Vec::new(),
        }
    }

    fn t(&self) -> &'static i18n::Strings { i18n::strings(self.lang) }

    fn active_address(&self) -> Option<String> { self.active_host().map(|host| host.address.clone()) }

    /// The active host's session state, or a standing empty/disconnected
    /// one when there isn't an active host or it has never connected —
    /// callers don't need an `Option` at every read site.
    fn terminal(&self) -> &crate::terminal::Terminal {
        self.active_address().and_then(|address| self.sessions.get(&address)).map(|s| &s.terminal).unwrap_or(&self.empty_session.terminal)
    }

    /// Like [`Self::terminal`], but creates the active host's session entry
    /// on first write instead of falling back to the empty one — so a
    /// connect actually persists. With no active host, mutations land on
    /// the throwaway empty session (callers that can run with no host
    /// selected don't need to special-case this).
    fn terminal_mut(&mut self) -> &mut crate::terminal::Terminal {
        match self.active_address() {
            Some(address) => &mut self.sessions.entry(address).or_default().terminal,
            None => &mut self.empty_session.terminal,
        }
    }

    fn terminal_widget(&self) -> Option<&Entity<TerminalWidget>> {
        self.active_address().and_then(|address| self.sessions.get(&address)).and_then(|s| s.widget.as_ref())
    }

    fn active_host(&self) -> Option<&Host> { self.active.and_then(|index| self.hosts.get(index)) }

    /// Switches which host's page is being viewed, and to which page. Does
    /// **not** touch that host's (or any other host's) session —
    /// connecting/disconnecting is explicit (see [`Self::start_session`] and
    /// the Terminal page's Disconnect button), so other connected hosts stay
    /// live in the background and keep listing in the sidebar.
    fn switch_to(&mut self, index: usize, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal().file_busy {
            self.notice = Some(self.t().host_switch_in_progress.into());
            return;
        }
        self.active = Some(index);
        self.page = page;
        self.notice = None;
        self.sidebar_expanded.insert(index);
        let _ = (window, cx);
    }

    /// Activates a host on its default page (Summary) — used when the user
    /// explicitly picks a host to view from outside the sidebar (the Hosts
    /// page's Connect button, or right after adding a new host).
    fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.switch_to(index, Page::Summary, window, cx);
    }

    /// Connects (with the saved login) or restarts the view for a host
    /// picked from the Hosts page, without jumping into its workspace until
    /// the connection is actually confirmed — `start_session`'s event loop
    /// calls `activate` for this host once it reports `Ready` (see
    /// `connecting_focus`). Showing the sidebar's per-host pages and the
    /// Summary view before that point made a stalled or failed connection
    /// look like a live, fully-loaded session.
    fn select_host(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.hosts.get(index).cloned() else { return; };
        let connected = self.sessions.get(&host.address).is_some_and(|s| s.terminal.connected);
        let busy = self.sessions.get(&host.address).is_some_and(|s| s.terminal.busy());
        if connected {
            self.activate(index, window, cx);
            return;
        }
        // `start_session` (and the `terminal_mut()`/`terminal()` calls it
        // makes before it starts keying everything off `host.address`) acts
        // on whichever host `active` currently points to, so it has to be
        // this one *before* either runs — only the page/sidebar reveal
        // waits for the confirmed connection.
        self.active = Some(index);
        self.connecting_focus = Some(host.address.clone());
        if busy {
            cx.notify();
            return;
        }
        if host.saved {
            self.start_session(&host, true, window, cx);
        } else {
            // No saved login to connect with yet — land on this host's
            // Terminal page, where the password/key form lives, instead of
            // an empty, not-yet-connected Summary.
            self.switch_to(index, Page::Terminal, window, cx);
        }
        cx.notify();
    }

    /// Closes one host's SSH session (and its terminal widget) without
    /// touching the host list, usable for any connected host regardless of
    /// which one is currently being viewed.
    fn disconnect_host(&mut self, address: &str) {
        if let Some(session) = self.sessions.get_mut(address) {
            session.terminal.disconnect();
            session.widget = None;
        }
    }

    /// Drops the host's session entirely (if any), closing its SSH
    /// connection and terminal widget, then removes it from the list.
    fn remove_host_from_list(&mut self, address: &str) {
        let Some(index) = self.hosts.iter().position(|host| host.address == address) else { return; };
        self.sessions.remove(address);
        self.hosts.remove(index);
        self.active = match self.active {
            Some(active) if active == index => None,
            Some(active) if active > index => Some(active - 1),
            other => other,
        };
        self.sidebar_expanded = self.sidebar_expanded.iter().filter_map(|&expanded| match expanded.cmp(&index) {
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(expanded - 1),
            std::cmp::Ordering::Less => Some(expanded),
        }).collect();
    }

    fn confirm_delete_host(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.hosts.get(index).cloned() else { return; };
        let lang = self.lang;
        let t = self.t();
        let entity = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let entity = entity.clone();
            let address = host.address.clone();
            let saved = host.saved;
            alert
                .title(i18n::delete_host_title(lang, &host.name))
                .description(t.hosts_delete_dialog_desc)
                .confirm()
                .ok_text(t.hosts_delete_dialog_ok)
                .ok_variant(gpui_kit::component::button::ButtonVariant::Danger)
                .on_ok(move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        if saved {
                            match this.host_store.delete(address.clone()) {
                                Ok(()) => { this.sessions.remove(&address); }
                                Err(error) => this.host_store_message = Some(error.to_string()),
                            }
                        } else {
                            this.remove_host_from_list(&address);
                            this.host_store_message = Some(this.t().hosts_deleted_unsaved.into());
                        }
                        cx.notify();
                    });
                    true
                })
        });
    }

    fn handle_host_store_event(&mut self, event: host_store::Event) {
        self.host_store.event_received();
        match event {
            host_store::Event::Listed(Ok((hosts, skipped))) => {
                for host in hosts {
                    if !self.hosts.iter().any(|existing| existing.address == host.address) { self.hosts.push(host); }
                }
                if skipped > 0 { self.host_store_message = Some(i18n::profiles_skipped(self.lang, skipped)); }
            }
            host_store::Event::Listed(Err(error)) => self.host_store_message = Some(error),
            host_store::Event::Deleted(address, Ok(())) => {
                self.remove_host_from_list(&address);
                self.host_store_message = Some(i18n::profile_deleted(self.lang, &address));
            }
            host_store::Event::Deleted(_, Err(error)) => self.host_store_message = Some(error),
        }
    }

    /// Requests fresh data for whichever live page is currently open, on
    /// that page's own refresh interval — ticked from the timer loop in
    /// `new()`, not from `render()` (which only runs on redraw, not on a
    /// schedule).
    fn poll_live_page(&mut self, cx: &mut Context<Self>) {
        if !self.live_auto_refresh { return; }
        // Docker page: a fresh CPU/RAM sample every 10 s while it is open.
        if self.page == Page::Docker {
            let docker = &self.terminal().docker;
            if self.terminal().connected && !docker.stats_loading && docker.stats_at.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(10)) {
                self.terminal_mut().request_docker_stats();
                cx.notify();
            }
            return;
        }
        let kind = match self.page {
            Page::Summary => crate::live::Kind::Summary,
            Page::Firewall => crate::live::Kind::Ports,
            _ => return,
        };
        if !self.terminal().connected { return; }
        let interval = std::time::Duration::from_secs(if kind == crate::live::Kind::Summary { 5 } else { 15 });
        let last = self.terminal().live.checked[kind.index()];
        if last.is_none_or(|last| last.elapsed() >= interval) {
            self.terminal_mut().request_live(crate::live::Request::new(kind));
            cx.notify();
        }
    }

    /// Connects (or reuses the saved login for) the given host, then spawns
    /// the task that drains SSH events and creates the terminal widget once
    /// the session is established enough to exist. The spawned task
    /// captures `host.address` and always routes back to that host's
    /// session — not whichever one happens to be active when an event
    /// arrives — so switching to a different host mid-connect (or while
    /// several are live at once) can't cross-wire their output.
    fn start_session(&mut self, host: &Host, use_saved: bool, window: &mut Window, cx: &mut Context<Self>) {
        // `Terminal::connect`/`connect_saved` read credentials off
        // `terminal.key_path`/`secret` (plain Strings); the actual typed
        // values live in the Input widgets' own entities and have to be
        // copied across here first, or the session connects with an empty
        // password/key path every time.
        self.terminal_mut().key_path = self.key_path.read(cx).value().to_string();
        self.terminal_mut().secret = self.secret.read(cx).value().to_string();

        let result = if use_saved { self.terminal_mut().connect_saved(host) } else { self.terminal_mut().connect(host) };
        let Some((size, input)) = result else { return; };
        let Some(events) = self.terminal_mut().take_events() else { return; };

        // The widget only knows how to produce plain bytes (keystrokes, PTY
        // query replies); bridge those into `ssh::Command::Input` here so
        // `terminal_widget.rs` stays independent of the SSH session type.
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);
        cx.spawn(async move |_this, _cx| {
            while let Some(bytes) = output_rx.recv().await {
                if input.send(ssh::Command::Input { update: false, bytes }).await.is_err() { break; }
            }
        }).detach();

        let widget = cx.new(|cx| TerminalWidget::new(80, 24, output_tx, size, window, cx));
        // So the user can start typing right away instead of having to
        // click the terminal first.
        let focus_handle = widget.read(cx).focus_handle().clone();
        window.focus(&focus_handle, cx);
        let address = host.address.clone();
        self.sessions.entry(address.clone()).or_default().widget = Some(widget.clone());

        // `_in` so `handle_file_result` can reach `window` (needed to write
        // into the editor's `EditorState`) — a plain `cx.spawn` only hands
        // the closure a `Context`, with no window access.
        cx.spawn_in(window, async move |this, cx| {
            let mut events = events;
            loop {
                match events.recv().await {
                    Some(event) => {
                        let outcome = this.update_in(cx, |app, window, cx| {
                            let session = app.sessions.entry(address.clone()).or_default();
                            let was_connected = session.terminal.connected;
                            let bytes = session.terminal.handle_ssh_event(event);
                            let became_connected = !was_connected && session.terminal.connected;
                            let file_result = session.terminal.file_result.take();
                            let fs_result = session.terminal.fs_result.take();
                            let git_result = session.terminal.git_result.take();
                            let docker_result = session.terminal.docker_result.take();
                            // `session`'s borrow of `app.sessions` must have
                            // ended (last use above) before this, since the
                            // rest of this closure needs `&mut app` as a whole
                            // (handle_file_result/handle_fs_result/activate).
                            if let Some(result) = file_result {
                                // Explorer/Editor state is a single slot for
                                // "whichever host is being viewed" (not yet
                                // per-session); only apply a reply that still
                                // belongs to the host currently on screen, so
                                // switching away mid-request can't bleed a
                                // stale result into a different host's view.
                                if app.active_address().as_deref() == Some(address.as_str()) {
                                    app.handle_file_result(result, window, cx);
                                }
                            }
                            if let Some(result) = fs_result {
                                if app.active_address().as_deref() == Some(address.as_str()) {
                                    app.handle_fs_result(result, window, cx);
                                }
                            }
                            if let Some(result) = git_result {
                                if app.active_address().as_deref() == Some(address.as_str()) {
                                    app.handle_git_result(result, cx);
                                }
                            }
                            if let Some(result) = docker_result {
                                if app.active_address().as_deref() == Some(address.as_str()) {
                                    app.handle_docker_result(result, window, cx);
                                }
                            }
                            if became_connected && app.connecting_focus.as_deref() == Some(address.as_str()) {
                                app.connecting_focus = None;
                                if let Some(index) = app.hosts.iter().position(|host| host.address == address) {
                                    app.activate(index, window, cx);
                                }
                            }
                            cx.notify();
                            bytes
                        });
                        let Ok(bytes) = outcome else { break; };
                        if let Some(bytes) = bytes {
                            let _ = widget.update(cx, |widget, cx| widget.feed(&bytes, cx));
                        }
                    }
                    None => break,
                }
            }
        }).detach();
    }

    fn open_add_host_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.host_name.clone();
        let address = self.host_address.clone();
        let user = self.host_user.clone();
        let port = self.host_port.clone();
        let key_auth = self.host_key_auth;
        let t = self.t();
        let entity = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let entity_for_checkbox = entity.clone();
            let entity_for_submit = entity.clone();
            dialog
                .title(t.add_new_host)
                .child(
                    Form::new()
                        .child(Field::new().label(t.add_host_field_name).child(Input::new(&name)))
                        .child(Field::new().label(t.add_host_field_address).child(Input::new(&address)))
                        .child(Field::new().label(t.add_host_field_username).child(Input::new(&user)))
                        .child(Field::new().label(t.add_host_field_port).child(Input::new(&port)))
                        .child(Field::new().label(t.add_host_field_auth).child(
                            Checkbox::new("key-auth").label(t.add_host_use_key_checkbox).checked(key_auth)
                                .on_change(move |value: &bool, _, cx| {
                                    let value = *value;
                                    entity_for_checkbox.update(cx, |this, cx| { this.host_key_auth = value; cx.notify(); });
                                }),
                        )),
                )
                .footer(
                    gpui_kit::component::dialog::DialogFooter::new()
                        .gap_2()
                        .child(gpui_kit::component::dialog::DialogClose::new().child(Button::new("cancel-host").label(t.add_host_cancel)))
                        .child(gpui_kit::component::dialog::DialogAction::new().child(
                            Button::new("submit-host").primary().label(t.add_host_submit).on_click(
                                move |_, window, cx| {
                                    entity_for_submit.update(cx, |this, cx| this.submit_host_form(window, cx));
                                },
                            ),
                        )),
                )
        });
    }

    fn submit_host_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = HostForm {
            name: self.host_name.read(cx).value().to_string(),
            address: self.host_address.read(cx).value().to_string(),
            user: self.host_user.read(cx).value().to_string(),
            port: self.host_port.read(cx).value().to_string(),
            key_auth: self.host_key_auth,
        };
        match form.host() {
            Ok(host) => {
                if self.hosts.iter().any(|existing| existing.address == host.address) {
                    self.host_form_error = Some(i18n::host_already_exists(self.lang));
                    window.push_notification(self.host_form_error.clone().unwrap(), cx);
                } else {
                    self.hosts.push(host);
                    let index = self.hosts.len() - 1;
                    self.activate(index, window, cx);
                }
            }
            Err(error) => {
                self.host_form_error = Some(error.clone());
                window.push_notification(error, cx);
            }
        }
        cx.notify();
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.t();
        let lang = self.lang;
        let host_menu = SidebarMenu::new().child(
            SidebarMenuItem::new(t.sidebar_manage_hosts)
                .icon(IconName::Settings)
                .active(self.page == Page::Hosts)
                .on_click(cx.listener(|this, _, _, cx| { this.page = Page::Hosts; cx.notify(); })),
        );

        // Every connected host appears here, not just the one being viewed —
        // connecting to a second host does not close the first (see
        // `Session`/`start_session`), so both stay listed until each is
        // explicitly disconnected. A host that's still connecting (or that
        // failed) does not get its workspace pages revealed yet — showing
        // them before the session is actually up made a stalled or failed
        // connection look like a live, fully-loaded one; progress for that
        // case is visible on the Hosts page's own row instead.
        let mut menu = SidebarMenu::new();
        let mut any_live = false;
        for (index, host) in self.hosts.iter().enumerate() {
            let Some(session) = self.sessions.get(&host.address) else { continue; };
            if !session.terminal.connected { continue; }
            any_live = true;
            let is_active = self.active == Some(index);
            let is_expanded = self.sidebar_expanded.contains(&index);
            menu = menu.child(
                SidebarMenuItem::new(host.name.clone())
                    .icon(IconName::HardDrive)
                    .active(is_active)
                    // Only expands/collapses this host's page list — it does
                    // not switch which host's page is being viewed, so the
                    // user can look at (or close) the menu for any number of
                    // hosts at once without navigating away from whichever
                    // one is currently on screen.
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.sidebar_expanded.insert(index) { this.sidebar_expanded.remove(&index); }
                        cx.notify();
                    })),
            );
            if is_expanded {
                // `Hosts` manages the host list itself (add/delete), not a
                // per-host page, so it does not belong nested under a
                // connected host.
                for page in Page::ALL.into_iter().filter(|page| *page != Page::Hosts) {
                    menu = menu.child(
                        SidebarMenuItem::new(page.title(lang))
                            .pl(px(20.))
                            // The Database Browser is reached from Docker, so Docker stays highlighted.
                            .active(is_active && (self.page == page || (page == Page::Docker && self.page == Page::DbBrowser)))
                            .on_click(cx.listener(move |this, _, window, cx| { this.switch_to(index, page, window, cx); cx.notify(); })),
                    );
                }
            }
        }
        if !any_live {
            menu = menu.child(SidebarMenuItem::new(t.sidebar_no_server));
        }

        let auto_refresh_dot = div().w(px(8.)).h(px(8.)).rounded(px(4.))
            .bg(if self.live_auto_refresh { cx.theme().green } else { cx.theme().red });
        let auto_refresh_row = h_flex().id("sidebar-auto-refresh").gap_2().items_center().cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| { this.live_auto_refresh = !this.live_auto_refresh; cx.notify(); }))
            .child(auto_refresh_dot)
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(if self.live_auto_refresh { t.live_auto_refresh_on } else { t.live_auto_refresh_off }));

        let sidebar = Sidebar::new("sidebar")
            .w(px(220.))
            .header(
                v_flex().gap_1()
                    .child(SidebarHeader::new().child(div().font_bold().child(t.app_tagline)))
                    .child(div().px_2().pb_1().child(auto_refresh_row)),
            )
            .child(SidebarGroup::new("").child(host_menu))
            .child(SidebarGroup::new(t.sidebar_workspace_label).child(menu))
            .footer(
                h_flex().w_full().justify_end().p_2().child(
                    Button::new("sidebar-settings").small().ghost()
                        .icon(IconName::Settings)
                        .tooltip(t.sidebar_settings_tooltip)
                        .selected(self.page == Page::Settings)
                        .on_click(cx.listener(|this, _, _, cx| { this.page = Page::Settings; cx.notify(); })),
                ),
            );

        sidebar.into_any_element()
    }

    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.page {
            // These manage their own internal layout/scrolling (the
            // terminal canvas needs real pixel bounds; the editor already
            // scrolls its explorer panel and text area) — wrapping them in
            // another scroll container would fight that, not help it.
            Page::Terminal => self.render_terminal(window, cx),
            Page::Editor => self.render_editor(window, cx),
            // The container list scrolls on its own above a docked output
            // panel that stays visible while an action runs.
            Page::Docker => self.render_docker(cx),
            // Tree on the left and a virtualized data table on the right,
            // each scrolling on its own.
            Page::DbBrowser => self.render_db_browser(cx),
            // The full package list renders only its visible rows in its own
            // scroll area, which needs a bounded height, not an outer scroll.
            Page::Tools if self.package_tab == 1 => self.render_tools(cx),
            // Everything else is a plain stack of cards/rows that can just
            // grow, so it needs an explicit scroll area once that content
            // is taller than the window (long service/port/package lists).
            _ => {
                let content = match self.page {
                    Page::Hosts => self.render_hosts(window, cx),
                    Page::Summary => self.render_summary(cx),
                    Page::Firewall => self.render_firewall(cx),
                    Page::Tools => self.render_tools(cx),
                    Page::Settings => self.render_settings(cx),
                    Page::Terminal | Page::Editor | Page::Docker | Page::DbBrowser => unreachable!(),
                };
                div().flex_1().min_h_0().size_full().overflow_y_scrollbar().child(content).into_any_element()
            }
        }
    }

    /// Status row shown at the top of every live page (Summary/Server
    /// Control/Ports): a LIVE/OFFLINE badge, a manual reload button, and
    /// (while connected) a spinner or error line depending on the last
    /// fetch for `kind`.
    fn live_status_row(&self, kind: crate::live::Kind, cx: &Context<Self>) -> AnyElement {
        let t = self.t();
        let connected = self.terminal().connected;
        let can_reload = connected && self.terminal().live.busy.is_none() && !self.terminal().update_running;
        let mut row = h_flex().gap_3().items_center().flex_wrap()
            .child(div().text_xs().font_bold().text_color(if connected { cx.theme().green } else { cx.theme().muted_foreground })
                .child(if connected { t.live_badge } else { t.offline_badge }))
            .child(Button::new("reload-live").small().label(t.live_reload)
                .disabled(!can_reload)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.terminal_mut().request_live(crate::live::Request::new(kind));
                    cx.notify();
                })));
        let mut status = v_flex().gap_1();
        if !connected {
            status = status.child(div().text_color(cx.theme().muted_foreground).child(t.live_offline_hint));
        } else if self.terminal().live.busy == Some(kind) {
            status = status.child(div().text_color(cx.theme().muted_foreground).child(t.live_reading));
        } else if let Some(error) = &self.terminal().live.errors[kind.index()] {
            status = status.child(div().text_color(cx.theme().red).child(error.clone()));
        }
        row = row.child(status);
        row.into_any_element()
    }

    fn render_settings(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.t();
        let muted = cx.theme().muted_foreground;
        let mode = cx.theme().mode;
        let mut language_row = h_flex().gap_2();
        for option in Language::ALL {
            language_row = language_row.child(
                Button::new(SharedString::from(format!("lang-{}", option.label()))).small().label(option.label())
                    .selected(self.lang == option)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.lang = option;
                        option.persist();
                        cx.notify();
                    })),
            );
        }
        v_flex().w_full().p_4().gap_4()
            .child(div().text_lg().font_bold().child(t.settings_title))
            .child(
                v_flex().gap_2()
                    .child(div().font_bold().child(t.settings_appearance_heading))
                    .child(div().text_color(muted).child(t.settings_appearance_desc))
                    .child(
                        h_flex().gap_2()
                            .child(Button::new("theme-light").small().label(t.settings_theme_light)
                                .selected(mode == gpui_kit::component::ThemeMode::Light)
                                .on_click(cx.listener(|_, _, window, cx| {
                                    gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Light, Some(window), cx);
                                    cx.notify();
                                })))
                            .child(Button::new("theme-dark").small().label(t.settings_theme_dark)
                                .selected(mode == gpui_kit::component::ThemeMode::Dark)
                                .on_click(cx.listener(|_, _, window, cx| {
                                    gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, Some(window), cx);
                                    cx.notify();
                                }))),
                    ),
            )
            .child(
                v_flex().gap_2()
                    .child(div().font_bold().child(t.settings_language_heading))
                    .child(div().text_color(muted).child(t.settings_language_desc))
                    .child(language_row),
            )
            .child(
                v_flex().gap_1()
                    .child(div().font_bold().child(t.settings_about_heading))
                    .child(div().text_color(muted).child(t.settings_about_text)),
            )
            .into_any_element()
    }

    fn render_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.t();
        let mut list = v_flex().gap_2();
        if self.hosts.is_empty() {
            list = list.child(
                div().p_4().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
                    .child(t.hosts_empty),
            );
        }
        for index in 0..self.hosts.len() {
            let host = self.hosts[index].clone();
            let session = self.sessions.get(&host.address);
            let live = session.is_some_and(|s| s.terminal.connected || s.terminal.busy());
            // Connecting now stays on this page instead of jumping straight
            // into the (not-yet-live) workspace, so this row is the only
            // place that progress or a failure is visible — show it here.
            let connecting = session.filter(|s| s.terminal.busy() && !s.terminal.connected).map(|s| s.terminal.status.clone());
            let error = session.filter(|s| !s.terminal.busy() && !s.terminal.connected).and_then(|s| s.terminal.error.clone());
            let connect_button = if live {
                solid_danger(Button::new(("host-connect", index)).small()).label(t.terminal_disconnect)
                    .on_click(cx.listener({
                        let address = host.address.clone();
                        move |this, _, _, cx| { this.disconnect_host(&address); cx.notify(); }
                    }))
            } else {
                Button::new(("host-connect", index)).small().label(if host.saved { t.hosts_connect } else { t.hosts_fill_login })
                    .on_click(cx.listener(move |this, _, window, cx| this.select_host(index, window, cx)))
            };
            let mut info = v_flex().flex_1().gap_1()
                .child(div().font_bold().child(host.name.clone()))
                .child(div().text_xs().text_color(cx.theme().muted_foreground)
                    .child(format!("{}@{}:{}", host.user, host.address, host.port)))
                .child(div().text_xs().text_color(if host.saved { cx.theme().green } else { cx.theme().yellow })
                    .child(if host.saved { t.hosts_login_saved } else { t.hosts_login_not_saved }));
            if let Some(status) = connecting {
                info = info.child(div().text_xs().text_color(cx.theme().yellow).child(status));
            }
            if let Some(error) = error {
                info = info.child(div().text_xs().text_color(cx.theme().red).child(error));
            }
            list = list.child(
                h_flex().gap_3().items_center().p_3().rounded(cx.theme().radius)
                    .border_1().border_color(cx.theme().border)
                    .child(info)
                    .child(connect_button)
                    .child(solid_danger(Button::new(("host-delete", index)).small()).label(t.hosts_delete)
                        .on_click(cx.listener(move |this, _, window, cx| this.confirm_delete_host(index, window, cx)))),
            );
        }
        let _ = (window,);
        v_flex().w_full().p_4().gap_3()
            .child(
                h_flex().items_center().justify_between()
                    .child(div().text_lg().child("Hosts"))
                    .child(Button::new("add-host").primary().icon(IconName::Plus).label(t.add_new_host)
                        .on_click(cx.listener(|this, _, window, cx| this.open_add_host_dialog(window, cx)))),
            )
            .child(div().text_color(cx.theme().muted_foreground).child(t.hosts_page_subtitle))
            .child(list)
            .into_any_element()
    }

    fn render_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.t();
        let Some(host) = self.active_host().cloned() else {
            return v_flex().p_4().gap_2()
                .child(t.terminal_choose_host_first)
                .child(Button::new("go-hosts").label(t.terminal_open_hosts).on_click(cx.listener(|this, _, _, cx| { this.page = Page::Hosts; cx.notify(); })))
                .into_any_element();
        };

        let mut content = v_flex().size_full().gap_3().p_4();
        content = content.child(
            h_flex().gap_3().items_center()
                .child(div().child(format!("{}@{}:{}", host.user, host.address, host.port)))
                .child(div().text_color(if self.terminal().connected { cx.theme().green } else { cx.theme().yellow }).child(self.terminal().status.clone()))
                .when(self.terminal().busy(), |row| {
                    row.child(Button::new("disconnect").small().label(t.terminal_disconnect).on_click(cx.listener(|this, _, _, cx| {
                        if let Some(address) = this.active_address() { this.disconnect_host(&address); }
                        cx.notify();
                    })))
                }),
        );

        if !self.terminal().busy() {
            let mut form = v_flex().gap_2();
            if host.saved {
                form = form.child(Button::new("connect-saved").label(t.terminal_connect_saved).on_click({
                    let host = host.clone();
                    cx.listener(move |this, _, window, cx| this.start_session(&host, true, window, cx))
                }));
            }
            if host.key_auth {
                form = form.child(Field::new().label(t.terminal_key_path_label).child(Input::new(&self.key_path)));
            }
            form = form.child(Field::new().label(if host.key_auth { t.terminal_passphrase_label } else { t.terminal_password_label })
                .child(Input::new(&self.secret)
                    .content_type(gpui_kit::component::input::InputContentType::Password)
                    .mask_toggle()));
            if cfg!(windows) {
                form = form.child(
                    Checkbox::new("remember-login")
                        .label(t.terminal_remember_login)
                        .checked(self.terminal().remember_login)
                        .on_change(cx.listener(|this, value, _, cx| { this.terminal_mut().remember_login = *value; cx.notify(); })),
                );
            }
            form = form.child(Button::new("connect").primary().label(t.terminal_connect).on_click({
                let host = host.clone();
                cx.listener(move |this, _, window, cx| this.start_session(&host, false, window, cx))
            }));
            content = content.child(form);
        }

        if let Some(error) = &self.terminal().error {
            content = content.child(div().text_color(cx.theme().red).child(error.clone()));
        }

        if let Some(widget) = self.terminal_widget() {
            content = content.child(div().flex_1().w_full().child(widget.clone()));
        } else {
            content = content.child(div().text_color(cx.theme().muted_foreground).child(t.terminal_no_shell_hint));
        }
        let _ = window;
        content.into_any_element()
    }

    fn render_trust_prompt(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.t();
        let prompt = self.terminal().trust.as_ref()?;
        let text = i18n::trust_new_host(self.lang, &prompt.host, &prompt.algorithm, &prompt.fingerprint);
        let background = cx.theme().background;
        let radius = cx.theme().radius;
        let border = cx.theme().border;
        Some(
            div().absolute().inset_0().bg(gpui_kit::black().opacity(0.6)).flex().items_center().justify_center()
                .child(
                    v_flex().gap_3().p_4().w(px(460.)).bg(background).rounded(radius)
                        .border_1().border_color(border)
                        .child(div().font_bold().child(t.trust_title))
                        .child(div().child(text))
                        .child(h_flex().gap_2()
                            .child(Button::new("trust-no").label(t.trust_reject).on_click(cx.listener(|this, _, _, cx| {
                                if let Some(prompt) = this.terminal_mut().trust.take() { let _ = prompt.answer.send(false); }
                                cx.notify();
                            })))
                            .child(Button::new("trust-yes").primary().label(t.trust_accept).on_click(cx.listener(|this, _, _, cx| {
                                if let Some(prompt) = this.terminal_mut().trust.take() { let _ = prompt.answer.send(true); }
                                cx.notify();
                            })))),
                )
                .into_any_element(),
        )
    }

    fn render_summary(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.t();
        let lang = self.lang;
        let status = self.live_status_row(crate::live::Kind::Summary, cx);
        let Some(report) = self.terminal().live.data[crate::live::Kind::Summary.index()].as_ref() else {
            return v_flex().p_4().gap_3().child(status).into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let can_manage = self.terminal().can_maintain();
        let memory = capacity(report.value("memory_used"), report.value("memory_total"), t);
        let disk = capacity(report.value("disk_used"), report.value("disk_total"), t);
        let cpu = report.value("cpu").parse::<f32>().ok().map(|v| format!("{v:.1}%")).unwrap_or_else(|| t.not_available.into());
        let uptime = report.value("uptime").parse::<u64>().ok()
            .map(|s| i18n::uptime_value(lang, s / 86400, s / 3600 % 24, s / 60 % 60))
            .unwrap_or_else(|| t.not_available.into());

        let metric = |label: &str, value: String, note: String| -> AnyElement {
            v_flex().flex_1().gap_1().p_3().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
                .child(div().text_xs().text_color(muted).child(label.to_string()))
                .child(div().text_xl().font_bold().child(value))
                .child(div().text_xs().text_color(muted).child(note))
                .into_any_element()
        };

        let swap = capacity(report.value("swap_used"), report.value("swap_total"), t);

        let mut errors = v_flex().gap_1();
        for row in &report.rows {
            if row.first().is_some_and(|key| key.ends_with("_error")) {
                errors = errors.child(div().text_xs().text_color(cx.theme().yellow).child(row.join(": ")));
            }
        }

        let disk_rows: Vec<&Vec<String>> = report.rows.iter().filter(|row| row.first().map(String::as_str) == Some("disk")).collect();
        let mut disks = v_flex().gap_2().p_3().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
            .child(div().text_sm().font_bold().child(t.summary_disks_heading))
            .child(
                h_flex().gap_3().text_xs().font_bold().text_color(muted)
                    .child(div().flex_1().min_w_0().child(t.summary_disk_mount))
                    .child(div().w(px(140.)).child(t.summary_disk_device))
                    .child(div().w(px(90.)).child(t.summary_disk_fstype))
                    .child(div().w(px(140.)).child(t.summary_disk_usage)),
            );
        for row in &disk_rows {
            if row.len() != 6 { continue; }
            disks = disks.child(
                h_flex().gap_3().text_xs()
                    .child(div().flex_1().min_w_0().truncate().child(row[1].clone()))
                    .child(div().w(px(140.)).text_color(muted).truncate().child(row[2].clone()))
                    .child(div().w(px(90.)).text_color(muted).child(row[3].clone()))
                    .child(div().w(px(140.)).child(capacity(&row[4], &row[5], t))),
            );
        }

        let mut content = v_flex().w_full().p_4().gap_3()
            .child(status)
            .child(h_flex().gap_3()
                .child(metric(t.summary_cpu_usage, cpu, i18n::logical_cpus(lang, report.value("cores"))))
                .child(metric(t.summary_memory, memory, t.summary_memory_note.into()))
                .child(metric(t.summary_storage, disk, t.summary_storage_note.into())))
            .child(
                v_flex().gap_2().p_3().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
                    .child(labeled_row(t.summary_hostname, report.value("hostname"), muted))
                    .child(labeled_row(t.summary_os, report.value("os"), muted))
                    .child(labeled_row(t.summary_kernel, report.value("kernel"), muted))
                    .child(labeled_row(t.summary_cpu_model, report.value("cpu_model"), muted))
                    .child(labeled_row(t.summary_swap, &swap, muted))
                    .child(labeled_row(t.summary_uptime, &uptime, muted))
                    .child(labeled_row(t.summary_ssh, t.summary_ssh_connected, muted)),
            );
        if !disk_rows.is_empty() { content = content.child(disks); }

        if let Some(count) = report.value("updates_available").parse::<u32>().ok() {
            let mut updates = v_flex().gap_2().p_3().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
                .child(div().text_sm().font_bold().child(t.summary_updates_heading));
            updates = if count == 0 {
                updates.child(div().text_xs().text_color(cx.theme().green).child(t.summary_updates_none))
            } else {
                updates
                    .child(div().text_xs().text_color(cx.theme().yellow).child(i18n::updates_available(lang, count)))
                    .child(Button::new("summary-update-system").small().primary().label(t.control_update_system).disabled(!can_manage)
                        .on_click(cx.listener(|this, _, window, cx| { this.confirm_system_update(window, cx); })))
            };
            // Visible here too (not just on Server Control) so clicking
            // Update System from Summary still shows whether it's running
            // or finished, instead of looking like nothing happened.
            if self.terminal().update_running {
                updates = updates.child(div().text_xs().text_color(muted).child(i18n::maintenance_running(lang, &self.terminal().task_title)));
            } else if let Some(result) = &self.terminal().update_result {
                updates = updates.child(div().text_xs().text_color(muted).child(result.clone()));
            }
            if report.value("reboot_required") == "1" {
                updates = updates.child(div().text_xs().text_color(cx.theme().red).child(t.summary_reboot_required));
            }
            content = content.child(updates);
        }

        content = content.child(
            v_flex().gap_2().p_3().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
                .child(div().text_sm().font_bold().child(t.summary_power_heading))
                .child(
                    h_flex().gap_2()
                        .child(solid_danger(Button::new("summary-reboot").small()).label(t.control_reboot).disabled(!can_manage)
                            .on_click(cx.listener(|this, _, window, cx| { this.confirm_power_action(true, window, cx); })))
                        .child(solid_danger(Button::new("summary-shutdown").small()).label(t.control_shutdown).disabled(!can_manage)
                            .on_click(cx.listener(|this, _, window, cx| { this.confirm_power_action(false, window, cx); }))),
                ),
        );

        content.child(errors)
            .child(div().text_xs().text_color(muted).child(t.summary_refresh_note))
            .into_any_element()
    }

    fn render_firewall(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.t();
        let lang = self.lang;
        let status = self.live_status_row(crate::live::Kind::Ports, cx);
        let muted = cx.theme().muted_foreground;
        let can_manage = self.terminal().can_maintain();

        let mut content = v_flex().w_full().p_4().gap_3().child(status);

        // UFW rejects allow/deny while inactive anyway (see `live.rs`), so
        // surface an explicit "enable first" step at the very top — before
        // the raw rule dumps and manage controls — instead of letting the
        // user scroll past all of that only to hit a disabled button.
        let ufw_inactive = self.firewall_engine == "ufw" && self.terminal().live.data[crate::live::Kind::Ports.index()].as_ref().is_some_and(|report| {
            report.rows.iter().any(|row| row.len() == 3 && row[0] == "firewall" && row[1] == "ufw" && !row[2].trim_start().starts_with("Status: active"))
        });
        if ufw_inactive {
            // While UFW is inactive, the raw per-engine dumps below are
            // just noise in front of the one action that matters (enable
            // it first) — skip straight to that instead of showing them.
            content = content.child(div().text_xs().text_color(cx.theme().yellow).child(t.firewall_inactive_warning));
            content = content.child(
                Button::new("fw-enable").label(t.firewall_enable_button).disabled(!can_manage)
                    .on_click(cx.listener(|this, _, window, cx| this.confirm_firewall_toggle(true, window, cx))),
            );
            return content.into_any_element();
        }

        content = content.child(div().text_sm().font_bold().child(t.ports_manage_heading));
        let detected = self.terminal().live.data[crate::live::Kind::Ports.index()].as_ref().is_some_and(|report| {
            report.rows.iter().any(|row| row.len() == 3 && row[0] == "firewall" && row[1] == self.firewall_engine)
        });
        let mut engine_row = h_flex().gap_2().items_center()
            .child(Button::new("fw-ufw").small().selected(self.firewall_engine == "ufw").label("UFW")
                .on_click(cx.listener(|this, _, _, cx| { this.firewall_engine = "ufw".into(); cx.notify(); })))
            .child(Button::new("fw-firewalld").small().selected(self.firewall_engine == "firewalld").label("firewalld (runtime)")
                .on_click(cx.listener(|this, _, _, cx| { this.firewall_engine = "firewalld".into(); cx.notify(); })));
        if detected {
            // Reaching this point means the selected engine is already
            // running (UFW's inactive case returns earlier above), so
            // offer the mirror action to the enable button shown there.
            // Labeled with the engine name so it reads distinctly from the
            // generic "Reload" button above (that one re-fetches this
            // page's live data; this one runs `ufw reload`/`firewall-cmd
            // --reload` on the server).
            let reload_label = SharedString::from(format!("{} {}", if self.firewall_engine == "ufw" { "UFW" } else { "firewalld" }, t.firewall_reload_button));
            engine_row = engine_row.child(
                Button::new("fw-reload").small().label(reload_label).disabled(!can_manage)
                    .on_click(cx.listener(|this, _, window, cx| this.confirm_firewall_reload(window, cx))),
            );
            engine_row = engine_row.child(
                solid_danger(Button::new("fw-disable")).small().label(t.firewall_disable_button).disabled(!can_manage)
                    .on_click(cx.listener(|this, _, window, cx| this.confirm_firewall_toggle(false, window, cx))),
            );
        }
        content = content.child(engine_row);
        content = content.child(
            h_flex().gap_2().items_center()
                .child(Input::new(&self.firewall_port).w(px(90.)))
                .child(Checkbox::new("fw-udp").label(t.ports_udp_checkbox).checked(self.firewall_udp)
                    .on_change(cx.listener(|this, value, _, cx| { this.firewall_udp = *value; cx.notify(); }))),
        );
        let port_value = self.firewall_port.read(cx).value().to_string().parse::<u16>().ok().filter(|port| *port != 0);
        let allow_label = t.ports_allow_label;
        let deny_label = if self.firewall_engine == "ufw" { t.ports_deny_label_ufw } else { t.ports_deny_label_other };
        content = content.child(
            h_flex().gap_2()
                .child(Button::new("fw-allow").label(allow_label).disabled(!(detected && port_value.is_some() && can_manage))
                    .on_click(cx.listener(move |this, _, window, cx| this.confirm_firewall_action(port_value, true, window, cx))))
                .child(solid_danger(Button::new("fw-deny")).label(deny_label).disabled(!(detected && port_value.is_some() && can_manage))
                    .on_click(cx.listener(move |this, _, window, cx| this.confirm_firewall_action(port_value, false, window, cx)))),
        );
        content = content.child(div().text_xs().text_color(muted).child(t.ports_footnote));

        if let Some(report) = self.terminal().live.data[crate::live::Kind::Ports.index()].as_ref() {
            let mut found = false;
            for row in &report.rows {
                if row.first().is_some_and(|key| key == "firewall_error") {
                    content = content.child(div().text_xs().text_color(cx.theme().yellow).child(row[1..].join(": ")));
                } else if row.first().is_some_and(|key| key == "firewall") && row.len() == 3 {
                    found = true;
                    if row[1] == "ufw" {
                        let rules = parse_ufw_rules(&row[2]);
                        let mut card = v_flex().gap_1().p_2().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
                            .child(div().text_sm().font_bold().child(i18n::firewall_rules_heading(lang, &row[1])));
                        if rules.is_empty() {
                            card = card.child(div().text_xs().text_color(muted).child(t.firewall_no_rules));
                        } else {
                            let green = cx.theme().green;
                            let red = cx.theme().red;
                            let delete_label = t.firewall_delete_rule_button;
                            card = card.children(rules.into_iter().map(|(number, to, action, from)| {
                                let action_color = if action.to_uppercase().starts_with("ALLOW") { green } else { red };
                                let to_for_click = to.clone();
                                let action_for_click = action.clone();
                                h_flex().gap_3().items_center().text_xs().p_1()
                                    .child(div().w(px(170.)).font_family("monospace").child(to))
                                    .child(div().w(px(90.)).text_color(action_color).child(action))
                                    .child(div().flex_1().text_color(muted).child(from))
                                    .child(solid_danger(Button::new(("fw-rule-delete", number as usize))).small().label(delete_label).disabled(!can_manage)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.confirm_firewall_delete_rule(number, to_for_click.clone(), action_for_click.clone(), window, cx);
                                        })))
                                    .into_any_element()
                            }));
                        }
                        content = content.child(card);
                    } else {
                        content = content.child(
                            v_flex().gap_1().p_2().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
                                .child(div().text_sm().font_bold().child(i18n::firewall_rules_heading(lang, &row[1])))
                                .child(div().text_xs().font_family("monospace").child(row[2].clone())),
                        );
                    }
                }
            }
            if !found && report.rows.iter().all(|row| row.first().is_none_or(|key| key != "firewall_error")) {
                content = content.child(div().text_xs().text_color(muted).child(t.firewall_none_detected));
            }
        }

        content.into_any_element()
    }

    fn render_tools(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.t();
        let lang = self.lang;
        let muted = cx.theme().muted_foreground;
        let connected = self.terminal().connected;
        let can_manage = self.terminal().can_maintain();

        let mut content = v_flex().w_full().p_4().gap_3().when(self.package_tab == 1, |content| content.size_full()).child(
            h_flex().gap_3().items_center()
                .child(div().text_xs().font_bold().text_color(if connected { cx.theme().green } else { muted })
                    .child(if connected { t.live_badge } else { t.offline_badge }))
                .child(Button::new("reload-packages").small().label(t.tools_reload_status).disabled(!can_manage)
                    .on_click(cx.listener(|this, _, _, cx| { this.terminal_mut().refresh_packages(); cx.notify(); }))),
        );

        if !connected {
            content = content.child(div().text_color(muted).child(t.tools_offline_hint));
            let mut list = v_flex().gap_2();
            for app in crate::packages::Software::ALL {
                list = list.child(
                    h_flex().gap_2().items_center().p_2().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
                        .child(div().flex_1().child(app.name()))
                        .child(div().text_xs().text_color(muted).child(t.tools_unknown))
                        .child(Button::new(SharedString::from(format!("install-{}", app.id()))).small().label(t.tools_install).disabled(true)),
                );
            }
            content = content.child(list);
            return content.into_any_element();
        }

        if self.terminal().update_running {
            content = content.child(div().text_color(cx.theme().yellow).child(i18n::maintenance_running(lang, &self.terminal().task_title)));
        }
        if self.terminal().inventory_scanning {
            content = content.child(div().text_color(muted).child(t.tools_scanning));
        }
        if let Some(error) = &self.terminal().package_error {
            content = content.child(div().text_color(cx.theme().red).child(error.clone()));
        }
        if let Some(result) = &self.terminal().update_result {
            content = content.child(div().text_sm().child(result.clone()));
        }

        let Some(inventory) = self.terminal().inventory.as_ref() else { return content.into_any_element(); };
        content = content.child(div().text_xs().text_color(muted).child(i18n::inventory_summary(lang, &inventory.manager, inventory.packages.len())));
        content = content.child(
            h_flex().gap_2()
                .child(Button::new("tab-catalog").small().selected(self.package_tab == 0).label(t.tools_catalog_tab)
                    .on_click(cx.listener(|this, _, _, cx| { this.package_tab = 0; cx.notify(); })))
                .child(Button::new("tab-installed").small().selected(self.package_tab == 1).label(t.tools_installed_tab)
                    .on_click(cx.listener(|this, _, _, cx| { this.package_tab = 1; cx.notify(); }))),
        );

        if self.package_tab == 0 {
            let mut list = v_flex().gap_2();
            for app in crate::packages::Software::ALL {
                let installed = app.installed(inventory);
                let manual = installed.is_empty() && inventory.binaries.iter().any(|id| *id == app.id());
                let (status_label, status_color) = if !installed.is_empty() {
                    (t.tools_status_installed, cx.theme().green)
                } else if manual {
                    (t.tools_status_detected_path, cx.theme().yellow)
                } else {
                    (t.tools_status_not_installed, muted)
                };
                let mut card = v_flex().gap_2().p_3().rounded(cx.theme().radius).border_1().border_color(cx.theme().border);
                let mut header = h_flex().gap_2().items_center()
                    .child(div().text_sm().font_bold().flex_1().min_w_0().child(app.name()))
                    .child(div().text_xs().text_color(status_color).child(status_label));
                if installed.is_empty() {
                    header = header.child(
                        Button::new(SharedString::from(format!("install-{}", app.id()))).small().label(t.tools_install).disabled(!can_manage || manual)
                            .on_click(cx.listener(move |this, _, window, cx| this.confirm_install(app, window, cx))),
                    );
                }
                card = card.child(header);
                card = card.child(div().text_xs().text_color(muted).child(app.description()));
                for package in &installed {
                    let name = package.name.clone();
                    card = card.child(
                        h_flex().gap_2().items_center()
                            .child(div().text_xs().font_family("monospace").flex_1().child(format!("{} · {}", package.name, package.version)))
                            .child(
                                solid_danger(Button::new(SharedString::from(format!("uninstall-{name}"))).small()).label(t.tools_uninstall).disabled(!can_manage)
                                    .on_click(cx.listener(move |this, _, window, cx| this.confirm_uninstall(name.clone(), window, cx))),
                            ),
                    );
                }
                if manual {
                    card = card.child(div().text_xs().text_color(muted).child(t.tools_manual_detect_note));
                }
                list = list.child(card);
            }
            content = content.child(list);
            content = content.child(div().text_xs().text_color(muted).child(t.tools_installed_note));
        } else {
            content = content.child(Input::new(&self.package_search));
            content = content.child(div().text_xs().text_color(muted).child(t.tools_package_search_note));
            let needle = self.package_search.read(cx).value().to_string().to_lowercase();
            let rows: Vec<(SharedString, SharedString)> = inventory.packages.iter()
                .filter(|package| needle.is_empty() || package.name.to_lowercase().contains(&needle) || package.version.to_lowercase().contains(&needle))
                .map(|package| (package.name.clone().into(), package.version.clone().into()))
                .collect();
            content = content.child(div().text_xs().text_color(muted).child(i18n::matching_packages(lang, rows.len())));
            // Virtualized: only the rows in view are built each frame, so
            // hundreds of packages (each with its own button) scroll smoothly.
            let count = rows.len();
            let rows = std::rc::Rc::new(rows);
            let entity = cx.entity();
            let uninstall = t.tools_uninstall;
            let divider = cx.theme().border;
            let list = uniform_list("installed-packages", count, move |range, _, _| {
                range.filter_map(|index| rows.get(index).cloned()).map(|(name, version)| {
                    let entity = entity.clone();
                    let target = name.to_string();
                    // `w_full`: list items are sized to their content by
                    // default, which left the columns ragged; fixed-width
                    // version/button columns then line up on every row.
                    h_flex().w_full().h(px(36.)).gap_3().items_center().text_xs().px_2()
                        .border_b_1().border_color(divider)
                        .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                        .child(div().w(px(200.)).flex_shrink_0().font_family("monospace").text_color(muted).truncate().child(version))
                        .child(div().w(px(96.)).flex_shrink_0().flex().justify_end().child(
                            solid_danger(Button::new(SharedString::from(format!("uninstall-{name}"))).small()).label(uninstall).disabled(!can_manage)
                                .on_click(move |_, window, cx| entity.update(cx, |this, cx| this.confirm_uninstall(target.clone(), window, cx))),
                        ))
                }).collect::<Vec<_>>()
            })
            .size_full()
            .track_scroll(&self.package_list_scroll);
            let header = h_flex().w_full().h(px(32.)).flex_shrink_0().gap_3().items_center().px_2().text_xs().font_bold().text_color(muted)
                .bg(cx.theme().muted).border_b_1().border_color(divider)
                .child(div().flex_1().min_w_0().child(t.tools_column_package))
                .child(div().w(px(200.)).flex_shrink_0().child(t.tools_column_version))
                .child(div().w(px(96.)).flex_shrink_0());
            content = content.child(
                v_flex().flex_1().min_h_0().w_full().rounded(cx.theme().radius).border_1().border_color(divider).overflow_hidden()
                    .child(header)
                    .child(
                        div().relative().flex_1().min_h_0().w_full()
                            .child(list)
                            .vertical_scrollbar(&self.package_list_scroll),
                    ),
            );
        }

        content.into_any_element()
    }

    fn confirm_install(&mut self, app: crate::packages::Software, window: &mut Window, cx: &mut Context<Self>) {
        let description = if let Some(inventory) = &self.terminal().inventory {
            i18n::install_desc_with_candidates(self.lang, &inventory.manager, app.candidates(&inventory.manager))
        } else {
            self.t().tools_install_desc_fallback.into()
        };
        self.confirm_maintenance(
            i18n::install_title(self.lang, app.name()),
            description,
            self.t().tools_install_ok,
            false,
            crate::packages::Maintenance::Install(app),
            window, cx,
        );
    }

    fn confirm_uninstall(&mut self, package: String, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm_maintenance(
            i18n::uninstall_title(self.lang, &package),
            self.t().tools_uninstall_desc,
            self.t().tools_uninstall_ok,
            true,
            crate::packages::Maintenance::Remove(package),
            window, cx,
        );
    }

    fn editor_host_string(&self) -> Option<String> {
        self.active_host().map(|host| format!("{}@[{}]:{}", host.user, host.address, host.port))
    }

    /// Routes a finished file operation to the explorer (directory listing)
    /// or the editor (read/save), based on which one is waiting — mirrors
    /// the old `Editor::finish`.
    fn handle_file_result(&mut self, result: Result<crate::remote_file::Reply, String>, window: &mut Window, cx: &mut Context<Self>) {
        use crate::remote_file::Reply;
        let was_browsing = self.explorer_browsing;
        let was_saving = self.editor_saving;
        self.explorer_browsing = false;
        self.editor_saving = false;
        match result {
            Ok(Reply::Directory(directory)) => {
                self.explorer_error = None;
                if self.explorer_pending.take().as_deref() == Some("~") {
                    self.explorer_expanded.remove("~");
                    self.explorer_expanded.insert(directory.path.clone());
                    self.explorer_root = directory.path.clone();
                }
                self.explorer_nodes.insert(directory.path.clone(), directory);
                self.continue_tree_fetch(window, cx);
            }
            Ok(Reply::Read { path, revision, text }) => {
                let language = language_for_path(&path);
                self.editor_path = path.clone();
                self.editor_path_input.update(cx, |state, cx| state.set_value(path, window, cx));
                self.editor_host = self.editor_pending_host.clone();
                self.editor_original = text.clone();
                self.editor_text.update(cx, |state, cx| {
                    state.set_value(text, window, cx);
                    state.set_highlighter(language, cx);
                });
                self.editor_revision = Some(revision);
                self.editor_message = Some(self.t().editor_opened_success.into());
                self.editor_failed = false;
            }
            Ok(Reply::Saved { revision }) => {
                self.editor_original = self.editor_text.read(cx).value().to_string();
                self.editor_revision = Some(revision);
                self.editor_message = Some(self.t().editor_saved_success.into());
                self.editor_failed = false;
            }
            Err(error) => {
                if was_browsing {
                    if let Some(path) = self.explorer_pending.take() {
                        self.explorer_expanded.remove(&path);
                        // Else `continue_tree_fetch` would retry it forever.
                        if let Some(picker) = self.move_picker.as_mut() { picker.expanded.remove(&path); }
                    }
                    self.explorer_error = Some(error);
                    self.continue_tree_fetch(window, cx);
                } else {
                    if was_saving { self.editor_revision = None; }
                    self.editor_message = Some(error);
                    self.editor_failed = true;
                }
            }
        }
        cx.notify();
    }

    /// Sends a `Request::List` for one tree node. Only one file operation
    /// can be in flight at a time (shared SSH exec channel with the editor
    /// read/save flow), so callers that need several nodes refreshed queue
    /// through [`Self::continue_tree_fetch`] instead of firing in parallel.
    fn fetch_tree_node(&mut self, path: String, query: String, window: &mut Window, cx: &mut Context<Self>) {
        if !self.terminal().connected || self.terminal().file_busy { return; }
        let show_hidden = self.explorer_show_hidden;
        match self.terminal_mut().request_file(crate::remote_file::Request::List { path: path.clone(), query, show_hidden }) {
            Ok(()) => { self.explorer_error = None; self.explorer_browsing = true; self.explorer_pending = Some(path); }
            Err(error) => self.explorer_error = Some(error),
        }
        let _ = window;
        cx.notify();
    }

    /// After a listing finishes, fetches the next expanded-but-uncached
    /// branch (if any and the channel is free), so toggling "tampilkan file
    /// tersembunyi" or Refresh fills back in already-expanded branches
    /// one at a time instead of leaving them stuck on "Memuat...".
    fn continue_tree_fetch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.terminal().connected || self.terminal().file_busy { return; }
        // Folders opened in the Move picker load the same way, so one that
        // was opened while the channel was busy still fills in.
        let picker = self.move_picker.iter().flat_map(|picker| picker.expanded.iter());
        if let Some(path) = self.explorer_expanded.iter().chain(picker).find(|p| !self.explorer_nodes.contains_key(p.as_str())).cloned() {
            self.fetch_tree_node(path, String::new(), window, cx);
        }
    }

    fn toggle_tree_node(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.explorer_expanded.remove(&path) {
            cx.notify();
            return;
        }
        self.explorer_expanded.insert(path.clone());
        if self.explorer_nodes.contains_key(&path) {
            cx.notify();
        } else {
            self.fetch_tree_node(path, String::new(), window, cx);
        }
    }

    /// Clears the whole cache and re-fetches the root with the current
    /// search query; branches the user had expanded before refill lazily
    /// afterwards via `continue_tree_fetch`.
    fn refresh_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.terminal().connected || self.terminal().file_busy { return; }
        self.explorer_nodes.clear();
        let query = self.explorer_query.read(cx).value().to_string();
        let root = if self.explorer_root.is_empty() { "~".to_string() } else { self.explorer_root.clone() };
        self.explorer_expanded.insert(root.clone());
        self.fetch_tree_node(root, query, window, cx);
    }

    fn open_remote_file(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(host) = self.editor_host_string() else { return; };
        match self.terminal_mut().request_file(crate::remote_file::Request::Read { path }) {
            Ok(()) => {
                self.editor_pending_host = host;
                self.editor_saving = false;
                self.explorer_browsing = false;
                self.editor_message = None;
            }
            Err(error) => { self.editor_message = Some(error); self.editor_failed = true; }
        }
        cx.notify();
    }

    fn select_remote_file(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal().file_busy { return; }
        let dirty = self.editor_text.read(cx).value().to_string() != self.editor_original;
        if !dirty {
            self.open_remote_file(path, cx);
            return;
        }
        let entity = cx.entity();
        let lang = self.lang;
        let t = self.t();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let entity = entity.clone();
            let path = path.clone();
            alert
                .title(t.editor_discard_draft_title)
                .description(i18n::discard_draft_desc(lang, &path))
                .confirm()
                .ok_text(t.editor_discard_draft_ok)
                .ok_variant(gpui_kit::component::button::ButtonVariant::Danger)
                .on_ok(move |_, _, cx| {
                    entity.update(cx, |this, cx| this.open_remote_file(path.clone(), cx));
                    true
                })
        });
    }

    fn save_draft(&mut self, cx: &mut Context<Self>) {
        let Some(revision) = self.editor_revision.clone() else { return; };
        let text = self.editor_text.read(cx).value().to_string();
        let request = crate::remote_file::Request::Save { path: self.editor_path.clone(), revision, text };
        match self.terminal_mut().request_file(request) {
            Ok(()) => { self.editor_saving = true; self.editor_message = None; }
            Err(error) => { self.editor_message = Some(error); self.editor_failed = true; }
        }
        cx.notify();
    }

    /// Folder the Explorer toolbar acts on: the selected folder, the selected
    /// file's folder, or the tree root when nothing is selected.
    fn explorer_target_dir(&self) -> String {
        match &self.explorer_selected {
            Some((path, true)) => path.clone(),
            Some((path, false)) => match path.rsplit_once('/') {
                Some(("", _)) => "/".into(),
                Some((parent, _)) => parent.into(),
                None => path.clone(),
            },
            None if self.explorer_root.is_empty() => "~".into(),
            None => self.explorer_root.clone(),
        }
    }

    fn set_fs_info(&mut self, text: impl Into<String>) {
        self.fs_message = Some(text.into());
        self.fs_message_error = false;
    }

    fn set_fs_error(&mut self, text: impl Into<String>) {
        self.fs_message = Some(text.into());
        self.fs_message_error = true;
    }

    /// Routes a finished file-manager operation (or one step of an Import
    /// batch) back into the Explorer. An Import batch only refreshes the
    /// tree once, when the whole batch finishes — not per item — both to
    /// avoid dozens of redundant listings for one multi-file import and so
    /// the tree does not visibly flicker/reset while it is still running.
    fn handle_fs_result(&mut self, result: Result<crate::remote_fs::Reply, String>, window: &mut Window, cx: &mut Context<Self>) {
        if self.import_total > 0 {
            if result.is_err() { self.import_failed += 1; }
            if self.import_queue.is_empty() {
                let done = self.import_total.saturating_sub(self.import_failed);
                let summary = i18n::import_summary(self.lang, done, self.import_failed);
                if self.import_failed > 0 { self.set_fs_error(summary); } else { self.set_fs_info(summary); }
                self.import_total = 0;
                self.import_failed = 0;
                // Expand the import's destination and every newly created
                // top-level folder so the result is immediately visible
                // instead of sitting collapsed in a long listing.
                for path in self.import_reveal.drain(..) {
                    self.explorer_expanded.insert(path);
                }
                self.refresh_tree(window, cx);
            } else {
                self.advance_import_queue(cx);
            }
            return;
        }
        // A selected path that was just deleted/renamed (or sits inside one)
        // no longer exists; drop it so the toolbar doesn't target it.
        if let Ok(crate::remote_fs::Reply::Deleted { path } | crate::remote_fs::Reply::Renamed { from: path, .. }) = &result {
            let inside = format!("{}/", path.trim_end_matches('/'));
            if self.explorer_selected.as_ref().is_some_and(|(selected, _)| selected == path || selected.starts_with(&inside)) {
                self.explorer_selected = None;
            }
        }
        match result {
            Ok(reply) => self.set_fs_info(i18n::fs_success_message(self.lang, &reply)),
            Err(error) => self.set_fs_error(error),
        }
        self.refresh_tree(window, cx);
    }

    /// Sends the next queued Import step (a remote `mkdir` or one local
    /// file's bytes), one at a time — the fs channel only runs a single
    /// operation at once. A local file that fails to read is skipped
    /// (tallied in `import_failed`) rather than aborting the whole batch;
    /// a request that fails to even start (disconnected, queue full) aborts
    /// the remaining queue with a visible error instead of silently dropping it.
    fn advance_import_queue(&mut self, cx: &mut Context<Self>) {
        while !self.import_queue.is_empty() && !self.terminal().fs_busy {
            let task = self.import_queue.remove(0);
            let request = match task {
                ImportTask::EnsureDir(path) => crate::remote_fs::Request::CreateFolder { path },
                ImportTask::UploadFile { remote, local } => match std::fs::read(&local) {
                    Ok(data) => crate::remote_fs::Request::Upload { path: remote, data },
                    Err(_) => { self.import_failed += 1; continue; }
                },
            };
            match self.terminal_mut().request_fs(request) {
                Ok(()) => { cx.notify(); return; }
                Err(error) => {
                    self.import_queue.clear();
                    self.import_total = 0;
                    self.import_failed = 0;
                    self.import_reveal.clear();
                    self.set_fs_error(error);
                    break;
                }
            }
        }
        cx.notify();
    }

    /// Opens the native OS file/folder picker and, once the user confirms a
    /// selection, hands the chosen local path(s) to [`Self::begin_import`].
    fn start_import(&mut self, parent: String, folder: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.terminal().connected || self.terminal().fs_busy || !self.import_queue.is_empty() { return; }
        let options = gpui_kit::PathPromptOptions {
            files: !folder, directories: folder, multiple: !folder, prompt: None,
        };
        let receiver = cx.prompt_for_paths(options);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else { return; };
            let _ = this.update_in(cx, |app, window, cx| app.begin_import(parent, folder, paths, window, cx));
        }).detach();
    }

    /// Builds the Import queue: for a folder, walks it recursively (local
    /// metadata only — file bytes are read lazily, one at a time, by
    /// `advance_import_queue`) so a large tree never holds more than one
    /// file's content in memory at once. Unreadable local entries are
    /// skipped (counted, not silently dropped) rather than aborting.
    fn begin_import(&mut self, parent: String, folder: bool, paths: Vec<std::path::PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        const MAX_IMPORT_ENTRIES: usize = 2000;
        let mut queue = Vec::new();
        let mut skipped = 0usize;
        // Expanded once the batch finishes so the import's own result is
        // immediately visible instead of sitting collapsed in a long listing.
        let mut reveal = vec![parent.clone()];
        if folder {
            for root in paths {
                let Some(root_name) = root.file_name().and_then(|n| n.to_str()) else { skipped += 1; continue; };
                let remote_root = child_path(&parent, root_name);
                reveal.push(remote_root.clone());
                queue.push(ImportTask::EnsureDir(remote_root.clone()));
                let mut stack = vec![(root.clone(), remote_root)];
                while let Some((local_dir, remote_dir)) = stack.pop() {
                    if queue.len() >= MAX_IMPORT_ENTRIES { break; }
                    let Ok(entries) = std::fs::read_dir(&local_dir) else { skipped += 1; continue; };
                    for entry in entries.flatten() {
                        if queue.len() >= MAX_IMPORT_ENTRIES { break; }
                        let Ok(file_type) = entry.file_type() else { skipped += 1; continue; };
                        let Some(name) = entry.file_name().to_str().map(str::to_owned) else { skipped += 1; continue; };
                        let remote_path = child_path(&remote_dir, &name);
                        if file_type.is_dir() {
                            queue.push(ImportTask::EnsureDir(remote_path.clone()));
                            stack.push((entry.path(), remote_path));
                        } else if file_type.is_file() {
                            queue.push(ImportTask::UploadFile { remote: remote_path, local: entry.path() });
                        } else {
                            skipped += 1;
                        }
                    }
                }
            }
        } else {
            for path in paths {
                if queue.len() >= MAX_IMPORT_ENTRIES { skipped += 1; continue; }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else { skipped += 1; continue; };
                queue.push(ImportTask::UploadFile { remote: child_path(&parent, name), local: path });
            }
        }
        if queue.is_empty() {
            let t = self.t();
            self.set_fs_info(if skipped > 0 { t.fs_import_nothing } else { t.fs_import_cancelled });
            cx.notify();
            return;
        }
        self.import_total = queue.len();
        self.import_failed = 0;
        self.import_queue = queue;
        self.import_reveal = reveal;
        self.set_fs_info(i18n::import_started(self.lang, self.import_total, skipped));
        self.advance_import_queue(cx);
        let _ = window;
    }

    fn copy_entry(&mut self, path: String, is_dir: bool, cx: &mut Context<Self>) {
        self.fs_clipboard = Some(FsClipboardEntry { path, is_dir });
        cx.notify();
    }

    /// Copies the clipboard entry into `parent`, auto-renaming to "name
    /// (copy)", "name (copy 2)", ... if that exact name is already cached
    /// for the destination folder (the common case: paste into the same
    /// folder as the original). The server still refuses an existing
    /// destination either way if the cache is stale.
    fn paste_into(&mut self, parent: String, cx: &mut Context<Self>) {
        let Some(entry) = &self.fs_clipboard else { return; };
        let name = entry.path.rsplit('/').next().unwrap_or(&entry.path).to_string();
        let is_dir = entry.is_dir;
        let from = entry.path.clone();
        let mut destination = child_path(&parent, &name);
        if let Some(existing) = self.explorer_nodes.get(&parent) {
            let taken: std::collections::HashSet<&str> = existing.entries.iter().map(|e| e.name.as_str()).collect();
            if taken.contains(name.as_str()) {
                let (stem, ext) = if is_dir { (name.as_str(), "") } else {
                    name.rsplit_once('.').unwrap_or((name.as_str(), ""))
                };
                let ext = if ext.is_empty() { String::new() } else { format!(".{ext}") };
                let mut candidate_name = format!("{stem} (copy){ext}");
                let mut attempt = 2;
                while taken.contains(candidate_name.as_str()) {
                    candidate_name = format!("{stem} (copy {attempt}){ext}");
                    attempt += 1;
                }
                destination = child_path(&parent, &candidate_name);
            }
        }
        let request = crate::remote_fs::Request::Copy { from, to: destination, recursive: is_dir };
        match self.terminal_mut().request_fs(request) {
            Ok(()) => self.fs_message = None,
            Err(error) => self.set_fs_error(error),
        }
        cx.notify();
    }

    fn confirm_delete_entry(&mut self, path: String, is_dir: bool, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        let t = self.t();
        let lang = self.lang;
        window.open_alert_dialog(cx, move |alert, _, _| {
            let entity = entity.clone();
            let path = path.clone();
            alert
                .title(t.fs_delete_title)
                .description(i18n::fs_delete_desc(lang, &path, is_dir))
                .confirm()
                .ok_text(t.fs_delete_ok)
                .ok_variant(gpui_kit::component::button::ButtonVariant::Danger)
                .on_ok(move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        let request = crate::remote_fs::Request::Delete { path: path.clone(), recursive: is_dir };
                        match this.terminal_mut().request_fs(request) {
                            Ok(()) => this.fs_message = None,
                            Err(error) => this.set_fs_error(error),
                        }
                        cx.notify();
                    });
                    true
                })
        });
    }

    fn open_new_file_dialog(&mut self, parent: String, window: &mut Window, cx: &mut Context<Self>) {
        self.fs_pending = Some(FsPending::NewFile { parent: parent.clone() });
        let value = String::new();
        self.fs_name_input.update(cx, |state, cx| state.set_value(value, window, cx));
        let t = self.t();
        self.open_fs_name_dialog(t.fs_new_file_title, t.fs_new_file_label, t.fs_new_file_submit, Some(parent), window, cx);
    }

    fn open_new_folder_dialog(&mut self, parent: String, window: &mut Window, cx: &mut Context<Self>) {
        self.fs_pending = Some(FsPending::NewFolder { parent: parent.clone() });
        let value = String::new();
        self.fs_name_input.update(cx, |state, cx| state.set_value(value, window, cx));
        let t = self.t();
        self.open_fs_name_dialog(t.fs_new_folder_title, t.fs_new_folder_label, t.fs_new_folder_submit, Some(parent), window, cx);
    }

    fn open_rename_dialog(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        self.fs_pending = Some(FsPending::Rename { from: path });
        self.fs_name_input.update(cx, |state, cx| state.set_value(name, window, cx));
        let t = self.t();
        self.open_fs_name_dialog(t.fs_rename_title, t.fs_rename_label, t.fs_rename_submit, None, window, cx);
    }

    /// Move opens a folder picker over the Explorer's own cached tree: click
    /// the destination folder, then Move. Typing a path stays available
    /// ("Type a path") for places outside the tree, e.g. /var/www.
    fn open_move_dialog(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let root = if self.explorer_root.is_empty() { "~".to_string() } else { self.explorer_root.clone() };
        let parent = parent_dir(&path);
        // Opens down to the item's current folder (where already listed).
        let mut expanded = std::collections::HashSet::from([root.clone()]);
        let mut folder = parent.clone();
        while folder.len() > root.len() && folder.starts_with(&root) {
            if self.explorer_nodes.contains_key(&folder) { expanded.insert(folder.clone()); }
            folder = parent_dir(&folder);
        }
        self.move_picker = Some(MovePicker { from: path, dest: parent, expanded });
        let entity = cx.entity();
        let t = self.t();
        window.open_dialog(cx, move |dialog, _, cx| {
            let app = entity.read(cx);
            let Some(picker) = app.move_picker.as_ref() else { return dialog.title(t.fs_move_title); };
            let theme = cx.theme();
            let (muted, radius, active, hover) = (theme.muted_foreground, theme.radius, theme.list_active, theme.sidebar_accent);
            let name = picker.from.rsplit('/').next().unwrap_or(&picker.from).to_string();
            let current = parent_dir(&picker.from);
            let target = child_path(&picker.dest, &name);
            let unchanged = target == picker.from;
            let mut list = v_flex().gap_0p5();
            for row in app.move_picker_rows() {
                list = list.child(match row {
                    MoveRow::Folder { path, name, depth, expanded } => {
                        let selected = picker.dest == path;
                        let indent = px(4. + depth as f32 * 16.);
                        let select_entity = entity.clone();
                        let toggle_entity = entity.clone();
                        let select_path = path.clone();
                        let toggle_path = path.clone();
                        h_flex().id(SharedString::from(format!("move-{path}"))).w_full().gap_1().items_center().pl(indent).pr_2().py_0p5()
                            .rounded(radius).cursor_pointer()
                            .when(selected, |row| row.bg(active))
                            .when(!selected, |row| row.hover(move |style| style.bg(hover)))
                            .on_click(move |_, window, cx| {
                                select_entity.update(cx, |this, cx| this.move_picker_select(select_path.clone(), window, cx));
                            })
                            .child(
                                div().id(SharedString::from(format!("move-chevron-{path}"))).flex_shrink_0()
                                    .child(Icon::new(if expanded { Lucide::ChevronDown } else { Lucide::ChevronRight }).small().text_color(muted))
                                    .on_click(move |_, window, cx| {
                                        cx.stop_propagation();
                                        toggle_entity.update(cx, |this, cx| this.move_picker_toggle(toggle_path.clone(), window, cx));
                                    }),
                            )
                            .child(Icon::new(if expanded { Lucide::FolderOpen } else { Lucide::Folder }).small().text_color(cx.theme().blue).flex_shrink_0())
                            .child(div().flex_1().min_w_0().text_sm().truncate().child(name))
                            .when(path == current, |row| row.child(div().flex_shrink_0().text_xs().text_color(muted).child(t.fs_move_current)))
                            .into_any_element()
                    }
                    MoveRow::Note { depth, loading } => div().pl(px(28. + depth as f32 * 16.)).text_xs().text_color(muted)
                        .child(if loading { t.fs_move_loading } else { t.fs_move_no_folders })
                        .into_any_element(),
                });
            }
            let submit_entity = entity.clone();
            let manual_entity = entity.clone();
            let manual_from = picker.from.clone();
            dialog
                .title(i18n::fs_move_picker_title(app.lang, &name))
                .w(px(460.))
                .child(
                    v_flex().gap_2()
                        .child(div().text_xs().text_color(muted).child(t.fs_move_pick_hint))
                        .child(div().h(px(280.)).w_full().p_1().rounded(radius).border_1().border_color(theme.border).overflow_y_scrollbar().child(list))
                        .child(icon_note(Lucide::FolderInput, if unchanged { muted } else { theme.foreground }, format!("{} {target}", t.fs_move_target)))
                        .child(h_flex().child(Button::new("move-manual").small().ghost().icon(Lucide::TextCursorInput).label(t.fs_move_type_path)
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                manual_entity.update(cx, |this, cx| this.open_move_path_dialog(manual_from.clone(), window, cx));
                            }))),
                )
                .footer(
                    gpui_kit::component::dialog::DialogFooter::new()
                        .gap_2()
                        .child(gpui_kit::component::dialog::DialogClose::new().child(Button::new("move-cancel").label(t.fs_dialog_cancel)))
                        .child(gpui_kit::component::dialog::DialogAction::new().child(
                            Button::new("move-submit").primary().icon(Lucide::FolderInput).label(t.fs_move_submit).disabled(unchanged)
                                .on_click(move |_, window, cx| {
                                    submit_entity.update(cx, |this, cx| this.submit_move_picker(cx));
                                    window.close_dialog(cx);
                                }),
                        )),
                )
        });
    }

    /// Permissions as a clickable Owner/Group/Others × Read/Write/Execute
    /// grid with presets, so nobody has to know octal to use it.
    fn open_chmod_dialog(&mut self, path: String, is_dir: bool, mode: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.chmod_ui = Some(ChmodUi { path, is_dir, original: mode, mode, recursive: false });
        let entity = cx.entity();
        let t = self.t();
        window.open_dialog(cx, move |dialog, _, cx| {
            let app = entity.read(cx);
            let Some(ui) = app.chmod_ui.as_ref() else { return dialog.title(t.fs_chmod_title); };
            let theme = cx.theme();
            let (muted, radius, border) = (theme.muted_foreground, theme.radius, theme.border);
            let name = ui.path.rsplit('/').next().unwrap_or(&ui.path).to_string();
            let mode = ui.mode;

            let mut grid = v_flex().w_full().rounded(radius).border_1().border_color(border).overflow_hidden()
                .child(
                    h_flex().w_full().px_3().py_1p5().gap_2().bg(theme.muted).text_xs().font_bold().text_color(muted)
                        .child(div().flex_1())
                        .children([t.fs_chmod_read, t.fs_chmod_write, t.fs_chmod_execute].map(|label| div().w(px(80.)).flex().justify_center().child(label))),
                );
            for (row, who) in [t.fs_chmod_owner, t.fs_chmod_group, t.fs_chmod_others].into_iter().enumerate() {
                let mut line = h_flex().w_full().px_3().py_1p5().gap_2().items_center().border_t_1().border_color(border)
                    .child(div().flex_1().text_sm().child(who));
                for column in 0..3 {
                    let bit = 1u32 << (8 - (row * 3 + column));
                    let entity = entity.clone();
                    line = line.child(div().w(px(80.)).flex().justify_center().child(
                        Checkbox::new(SharedString::from(format!("chmod-{row}-{column}"))).checked(mode & bit != 0)
                            .on_change(move |value: &bool, _, cx| {
                                let value = *value;
                                entity.update(cx, |this, cx| {
                                    if let Some(ui) = this.chmod_ui.as_mut() { if value { ui.mode |= bit } else { ui.mode &= !bit } }
                                    cx.notify();
                                });
                            }),
                    ));
                }
                grid = grid.child(line);
            }

            let mut presets = h_flex().flex_wrap().gap_1();
            for (preset, hint) in [(0o644, t.fs_chmod_preset_644), (0o755, t.fs_chmod_preset_755), (0o600, t.fs_chmod_preset_600), (0o700, t.fs_chmod_preset_700)] {
                let entity = entity.clone();
                presets = presets.child(
                    Button::new(SharedString::from(format!("chmod-preset-{preset:o}"))).small().label(format!("{preset:o}")).tooltip(hint)
                        .selected(mode == preset)
                        .on_click(move |_, _, cx| entity.update(cx, |this, cx| {
                            if let Some(ui) = this.chmod_ui.as_mut() { ui.mode = preset; }
                            cx.notify();
                        })),
                );
            }

            let mut body = v_flex().gap_3()
                .child(div().text_xs().font_family("monospace").text_color(muted).truncate().child(ui.path.clone()))
                .child(
                    h_flex().gap_3().items_center()
                        .child(div().text_xl().font_bold().font_family("monospace").child(format!("{mode:03o}")))
                        .child(div().text_sm().font_family("monospace").text_color(muted).child(mode_symbolic(mode)))
                        .when(mode != ui.original, |row| row.child(div().text_xs().text_color(muted).child(i18n::fs_chmod_was(app.lang, ui.original)))),
                )
                .child(grid)
                .child(h_flex().gap_2().items_center().flex_wrap().child(div().text_xs().text_color(muted).child(t.fs_chmod_presets)).child(presets));
            if ui.is_dir {
                let recursive_entity = entity.clone();
                body = body.child(
                    v_flex().gap_1()
                        .child(Checkbox::new("chmod-recursive").label(t.fs_chmod_recursive).checked(ui.recursive)
                            .on_change(move |value: &bool, _, cx| {
                                let value = *value;
                                recursive_entity.update(cx, |this, cx| {
                                    if let Some(ui) = this.chmod_ui.as_mut() { ui.recursive = value; }
                                    cx.notify();
                                });
                            }))
                        .child(div().pl_6().text_xs().text_color(muted).child(t.fs_chmod_recursive_note)),
                );
                // Without execute a folder cannot be entered, even by its owner.
                if mode & 0o100 == 0 {
                    body = body.child(notice_row(Lucide::TriangleAlert, theme.yellow, radius, t.fs_chmod_dir_no_exec));
                }
            }
            if mode & 0o002 != 0 {
                body = body.child(notice_row(Lucide::TriangleAlert, theme.yellow, radius, t.fs_chmod_world_writable));
            }

            let unchanged = mode == ui.original && !ui.recursive;
            let submit_entity = entity.clone();
            dialog
                .title(i18n::fs_chmod_dialog_title(app.lang, &name))
                .w(px(480.))
                .child(body)
                .footer(
                    gpui_kit::component::dialog::DialogFooter::new()
                        .gap_2()
                        .child(gpui_kit::component::dialog::DialogClose::new().child(Button::new("chmod-cancel").label(t.fs_dialog_cancel)))
                        .child(gpui_kit::component::dialog::DialogAction::new().child(
                            Button::new("chmod-apply").primary().icon(Lucide::ShieldCheck).label(t.fs_chmod_apply).disabled(unchanged)
                                .on_click(move |_, window, cx| {
                                    submit_entity.update(cx, |this, cx| this.submit_chmod(cx));
                                    window.close_dialog(cx);
                                }),
                        )),
                )
        });
    }

    fn submit_chmod(&mut self, cx: &mut Context<Self>) {
        let Some(ui) = self.chmod_ui.take() else { return; };
        let request = crate::remote_fs::Request::Chmod { path: ui.path, mode: ui.mode, recursive: ui.is_dir && ui.recursive };
        match self.terminal_mut().request_fs(request) {
            Ok(()) => self.fs_message = None,
            Err(error) => self.set_fs_error(error),
        }
        cx.notify();
    }

    /// Picker rows: folders only (depth-first, expanded branches inline),
    /// leaving out the item being moved so a folder can't go into itself.
    fn move_picker_rows(&self) -> Vec<MoveRow> {
        let Some(picker) = &self.move_picker else { return Vec::new(); };
        let root = if self.explorer_root.is_empty() { "~".to_string() } else { self.explorer_root.clone() };
        let mut rows = Vec::new();
        let expanded = picker.expanded.contains(&root);
        rows.push(MoveRow::Folder { path: root.clone(), name: root.clone(), depth: 0, expanded });
        if expanded { push_move_rows(&self.explorer_nodes, picker, &root, 1, &mut rows); }
        rows
    }

    /// Clicking a folder makes it the destination and opens it, so its
    /// subfolders can be picked next.
    fn move_picker_select(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.move_picker.as_mut() else { return; };
        picker.dest = path.clone();
        if picker.expanded.insert(path.clone()) && !self.explorer_nodes.contains_key(&path) {
            self.fetch_tree_node(path, String::new(), window, cx);
        }
        cx.notify();
    }

    fn move_picker_toggle(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.move_picker.as_mut() else { return; };
        if !picker.expanded.remove(&path) {
            picker.expanded.insert(path.clone());
            if !self.explorer_nodes.contains_key(&path) { self.fetch_tree_node(path, String::new(), window, cx); }
        }
        cx.notify();
    }

    fn submit_move_picker(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = self.move_picker.take() else { return; };
        let t = self.t();
        if picker.dest == picker.from || picker.dest.starts_with(&format!("{}/", picker.from)) {
            self.set_fs_error(t.fs_move_into_self);
            cx.notify();
            return;
        }
        let name = picker.from.rsplit('/').next().unwrap_or(&picker.from).to_string();
        let to = child_path(&picker.dest, &name);
        if to == picker.from { return; }
        match self.terminal_mut().request_fs(crate::remote_fs::Request::Rename { from: picker.from, to }) {
            Ok(()) => self.fs_message = None,
            Err(error) => self.set_fs_error(error),
        }
        cx.notify();
    }

    fn open_move_path_dialog(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.move_picker = None;
        self.fs_pending = Some(FsPending::Move { from: path.clone() });
        self.fs_name_input.update(cx, |state, cx| state.set_value(path, window, cx));
        let t = self.t();
        self.open_fs_name_dialog(t.fs_move_title, t.fs_move_label, t.fs_move_submit, None, window, cx);
    }

    /// Shared single-field dialog (New File/New Folder/Rename/Move all just
    /// need one text value) — which request submitting it builds is decided
    /// by `self.fs_pending`, set by the caller right before this opens.
    /// `location`, for New File/Folder, is shown so the user can see which
    /// folder the new entry goes into before creating it.
    fn open_fs_name_dialog(&mut self, title: &'static str, label: &'static str, submit: &'static str, location: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        let input = self.fs_name_input.clone();
        let cancel_label = self.t().fs_dialog_cancel;
        let location = location.map(|path| format!("{} {path}", self.t().fs_dialog_location));
        let muted = cx.theme().muted_foreground;
        window.open_dialog(cx, move |dialog, _, _| {
            let entity = entity.clone();
            dialog
                .title(title)
                .children(location.clone().map(|text| {
                    icon_note(Lucide::FolderOpen, muted, text).into_any_element()
                }))
                .child(Form::new().child(Field::new().label(label).child(Input::new(&input))))
                .footer(
                    gpui_kit::component::dialog::DialogFooter::new()
                        .gap_2()
                        .child(gpui_kit::component::dialog::DialogClose::new().child(Button::new("fs-dialog-cancel").label(cancel_label)))
                        .child(gpui_kit::component::dialog::DialogAction::new().child(
                            Button::new("fs-dialog-submit").primary().label(submit).on_click(move |_, window, cx| {
                                entity.update(cx, |this, cx| this.submit_fs_name(window, cx));
                                window.close_dialog(cx);
                            }),
                        )),
                )
        });
    }

    fn submit_fs_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self.fs_pending.take() else { return; };
        let value = self.fs_name_input.read(cx).value().to_string();
        let t = self.t();
        let request = match pending {
            FsPending::NewFile { parent } => match validate_fs_name(&value) {
                Ok(name) => Some(crate::remote_fs::Request::CreateFile { path: child_path(&parent, &name) }),
                Err(()) => { self.set_fs_error(t.fs_invalid_name); None }
            },
            FsPending::NewFolder { parent } => match validate_fs_name(&value) {
                Ok(name) => Some(crate::remote_fs::Request::CreateFolder { path: child_path(&parent, &name) }),
                Err(()) => { self.set_fs_error(t.fs_invalid_name); None }
            },
            FsPending::Rename { from } => match validate_fs_name(&value) {
                Ok(name) => {
                    let parent = from.rsplit_once('/').map(|(p, _)| p).filter(|p| !p.is_empty()).unwrap_or("/").to_string();
                    let to = child_path(&parent, &name);
                    Some(crate::remote_fs::Request::Rename { from, to })
                }
                Err(()) => { self.set_fs_error(t.fs_invalid_name); None }
            },
            FsPending::Move { from } => {
                let to = value.trim().to_string();
                if to.is_empty() || !to.starts_with('/') || to.chars().any(char::is_control) {
                    self.set_fs_error(t.fs_invalid_path);
                    None
                } else {
                    Some(crate::remote_fs::Request::Rename { from, to })
                }
            }
        };
        if let Some(request) = request {
            match self.terminal_mut().request_fs(request) {
                Ok(()) => self.fs_message = None,
                Err(error) => self.set_fs_error(error),
            }
        }
        let _ = window;
        cx.notify();
    }

    fn open_git_clone_dialog(&mut self, parent: String, window: &mut Window, cx: &mut Context<Self>) {
        self.git_url_input.update(cx, |state, cx| state.set_value(String::new(), window, cx));
        self.git_dir_input.update(cx, |state, cx| state.set_value(String::new(), window, cx));
        self.ensure_git_auth_checked(cx);
        let entity = cx.entity();
        let url_input = self.git_url_input.clone();
        let dir_input = self.git_dir_input.clone();
        let t = self.t();
        let lang = self.lang;
        let muted = cx.theme().muted_foreground;
        window.open_dialog(cx, move |dialog, _, cx| {
            // Live: the account line updates once the background check and
            // any wizard steps finish, without reopening this dialog.
            let account = git_account_line(&entity, t, lang, cx);
            // An HTTPS GitHub/GitLab URL bypasses the SSH login entirely, so
            // offer its SSH form as the user types instead of letting the
            // clone fail with "could not read Username".
            let typed_url = url_input.read(cx).value().to_string();
            let ssh_suggestion = crate::packages::https_to_ssh_url(&typed_url).map(|ssh_url| {
                let border = cx.theme().border;
                let radius = cx.theme().radius;
                let input = url_input.clone();
                let replacement = ssh_url.clone();
                v_flex().gap_2().p_2().rounded(radius).border_1().border_color(border)
                    .child(icon_note(Lucide::Info, muted, t.git_clone_https_hint))
                    .child(div().text_xs().font_family("monospace").child(ssh_url))
                    .child(
                        Button::new("git-clone-use-ssh").small().icon(Lucide::KeyRound).label(t.git_clone_use_ssh)
                            .on_click(move |_, window, cx| {
                                input.update(cx, |state, cx| state.set_value(replacement.clone(), window, cx));
                            }),
                    )
            });
            let entity = entity.clone();
            let parent = parent.clone();
            dialog
                .title(t.git_clone_title)
                .child(
                    v_flex().gap_3()
                        .child(account)
                        .child(
                            Form::new()
                                .child(Field::new().label(t.git_clone_url_label).child(Input::new(&url_input)))
                                .child(Field::new().label(t.git_clone_dir_label).child(Input::new(&dir_input))),
                        )
                        .children(ssh_suggestion)
                        .child(div().text_xs().text_color(muted).child(t.git_clone_note)),
                )
                .footer(
                    gpui_kit::component::dialog::DialogFooter::new()
                        .gap_2()
                        .child(gpui_kit::component::dialog::DialogClose::new().child(Button::new("git-clone-cancel").label(t.fs_dialog_cancel)))
                        .child(gpui_kit::component::dialog::DialogAction::new().child(
                            Button::new("git-clone-submit").primary().icon(Lucide::GitBranch).label(t.git_clone_submit).on_click(move |_, window, cx| {
                                entity.update(cx, |this, cx| this.submit_git_clone(parent.clone(), cx));
                                window.close_dialog(cx);
                            }),
                        )),
                )
        });
    }

    fn submit_git_clone(&mut self, parent: String, cx: &mut Context<Self>) {
        let t = self.t();
        let url = self.git_url_input.read(cx).value().trim().to_string();
        let typed_dir = self.git_dir_input.read(cx).value().to_string();
        if !crate::packages::valid_git_url(&url) {
            self.set_fs_error(t.git_clone_invalid_url);
            cx.notify();
            return;
        }
        let dir = if typed_dir.trim().is_empty() {
            crate::packages::git_repo_name(&url)
        } else {
            validate_fs_name(&typed_dir).ok()
        };
        let Some(dir) = dir else {
            self.set_fs_error(t.fs_invalid_name);
            cx.notify();
            return;
        };
        if !parent.starts_with('/') {
            self.set_fs_error(t.fs_invalid_path);
            cx.notify();
            return;
        }
        let target = child_path(&parent, &dir);
        let action = crate::packages::Maintenance::GitClone { url, parent: parent.clone(), dir };
        if self.terminal_mut().start_maintenance(action) {
            self.git_clone_target = Some((parent, target));
            self.fs_message = None;
            // Progress streams into the terminal panel, which starts closed.
            self.terminal_footer_open = true;
        } else {
            self.set_fs_error(t.git_clone_busy);
        }
        cx.notify();
    }

    /// Moves the interactive shell into `path` and shows/focuses the terminal.
    fn open_folder_in_terminal(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.run_in_terminal(format!("cd -- {}", shell_quote(&path)), window, cx);
    }

    /// Types `command_line` + Enter into the interactive shell (so its output
    /// is visible and Ctrl+C works), then shows and focuses the terminal.
    fn run_in_terminal(&mut self, command_line: String, window: &mut Window, cx: &mut Context<Self>) {
        let t = self.t();
        let Some(widget) = self.terminal_widget().cloned() else {
            self.set_fs_error(t.terminal_not_ready);
            cx.notify();
            return;
        };
        // The text is typed into whatever owns the screen; inside vim/htop
        // it would become editor/program commands instead of a shell line.
        if widget.read(cx).in_alternate_screen() {
            self.set_fs_error(t.terminal_fullscreen_app);
            cx.notify();
            return;
        }
        // Ctrl+U first clears anything half-typed at the prompt, so it
        // can't be glued onto the front of the command.
        let typed = format!("\x15{command_line}\r");
        if !widget.read(cx).send_input(typed.into_bytes()) {
            self.set_fs_error(t.terminal_not_ready);
            cx.notify();
            return;
        }
        self.terminal_footer_open = true;
        self.terminal_footer_fullscreen = false;
        let focus = widget.read(cx).focus_handle().clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Runs `git <subcommand>` in repository `path` through the interactive
    /// shell, so its output shows and any credential prompt can be answered.
    fn run_git_in(&mut self, path: String, subcommand: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.run_in_terminal(format!("cd -- {} && git {subcommand}", shell_quote(&path)), window, cx);
    }

    /// Push publishes commits to a shared remote, so it is confirmed first.
    fn confirm_git_push(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let host = self.active_host().map(|host| format!("{} ({}@{})", host.name, host.user, host.address)).unwrap_or_default();
        let entity = cx.entity();
        let t = self.t();
        let lang = self.lang;
        window.open_alert_dialog(cx, move |alert, _, _| {
            let entity = entity.clone();
            let path = path.clone();
            alert
                .title(t.git_push_title)
                .description(i18n::git_push_desc(lang, &host, &path))
                .confirm()
                .ok_text(t.git_push)
                .on_ok(move |_, window, cx| {
                    entity.update(cx, |this, cx| this.run_git_in(path.clone(), "push", window, cx));
                    true
                })
        });
    }

    /// First step of "Run docker compose up": checks Docker access in the
    /// background; `handle_docker_result` then opens the right dialog.
    fn start_compose_up(&mut self, path: String, cx: &mut Context<Self>) {
        match self.terminal_mut().request_docker_check() {
            Ok(()) => {
                self.compose_pending = Some(path);
                self.set_fs_info(self.t().compose_checking);
            }
            Err(error) => self.set_fs_error(error),
        }
        cx.notify();
    }

    fn handle_docker_result(&mut self, result: Result<crate::docker_access::Access, String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.compose_pending.take() else { return; };
        let t = self.t();
        self.fs_message = None;
        match result {
            Err(error) => self.set_fs_error(error),
            Ok(access) if !access.installed => self.set_fs_error(t.docker_missing),
            Ok(access) if !access.compose => self.set_fs_error(t.compose_missing),
            Ok(access) if access.member => self.confirm_compose_up(path, None, window, cx),
            Ok(access) => self.confirm_compose_up(path, Some(access.user), window, cx),
        }
        cx.notify();
    }

    /// Asks before running `docker compose up -d --build` for `path`:
    /// `--build` can recreate running containers (a brief outage), so the
    /// target host and the exact command are shown first. With
    /// `setup_for = Some(user)`, the account isn't in the `docker` group yet,
    /// so the one-time setup is explained and run in the same step.
    fn confirm_compose_up(&mut self, path: String, setup_for: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some((folder, file)) = path.rsplit_once('/') else { return; };
        let folder = if folder.is_empty() { "/" } else { folder };
        let compose = format!("docker compose -f {} up -d --build", shell_quote(file));
        // `sg docker -c` applies the docker group to this one command, so it
        // works right away — even in a shell that logged in before the user
        // was added (where `docker info` still fails), with no reconnect.
        let command_line = match &setup_for {
            Some(_) => format!("cd -- {} && sudo usermod -aG docker \"$USER\" && sg docker -c {}", shell_quote(folder), shell_quote(&compose)),
            None => format!("cd -- {} && if docker info >/dev/null 2>&1; then {compose}; else sg docker -c {}; fi", shell_quote(folder), shell_quote(&compose)),
        };
        let shown = format!("cd -- {} && {compose}", shell_quote(folder));
        let host = self.active_host().map(|host| format!("{} ({}@{})", host.name, host.user, host.address)).unwrap_or_default();
        let entity = cx.entity();
        let t = self.t();
        let lang = self.lang;
        window.open_alert_dialog(cx, move |alert, _, _| {
            let entity = entity.clone();
            let command_line = command_line.clone();
            let (title, description, ok) = match &setup_for {
                Some(user) => (t.compose_setup_title, i18n::compose_setup_desc(lang, &host, user, &shown), t.compose_setup_ok),
                None => (t.compose_up_title, i18n::compose_up_desc(lang, &host, &shown), t.compose_up_ok),
            };
            alert
                .title(title)
                .description(description)
                .confirm()
                .ok_text(ok)
                .on_ok(move |_, window, cx| {
                    entity.update(cx, |this, cx| this.run_in_terminal(command_line.clone(), window, cx));
                    true
                })
        });
    }

    fn run_docker_action(&mut self, action: crate::docker::Action, cx: &mut Context<Self>) {
        let title = action.title();
        if let Err(error) = self.terminal_mut().run_docker(action) {
            let docker = &mut self.terminal_mut().docker;
            if !docker.running {
                docker.begin(title);
                docker.running = false;
                docker.result = Some(Err(error));
            }
        }
        cx.notify();
    }

    /// Actions that can interrupt a service (restart/stop/rebuild/down) show
    /// the target host and command first; read-only ones run directly.
    fn confirm_docker_action(&mut self, action: crate::docker::Action, window: &mut Window, cx: &mut Context<Self>) {
        if !action.disruptive() {
            self.run_docker_action(action, cx);
            return;
        }
        use gpui_kit::component::button::ButtonVariant;
        let t = self.t();
        // Stop/Down take the service offline until started again, so their
        // button is red; restart/rebuild only interrupt it briefly.
        let (ok, variant) = match &action {
            crate::docker::Action::Restart(_) => (t.docker_restart_ok, ButtonVariant::Warning),
            crate::docker::Action::Stop(_) => (t.docker_stop_ok, ButtonVariant::Danger),
            crate::docker::Action::ComposeDown(_) => (t.docker_down_ok, ButtonVariant::Danger),
            _ => (t.docker_rebuild_ok, ButtonVariant::Warning),
        };
        let title = i18n::docker_confirm_title(self.lang, &action);
        let host = self.active_host().map(|host| format!("{} ({}@{})", host.name, host.user, host.address)).unwrap_or_default();
        let description = i18n::docker_confirm_desc(self.lang, &host, &action);
        let entity = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let entity = entity.clone();
            let action = action.clone();
            alert
                .title(title.clone())
                .description(description.clone())
                .confirm()
                .ok_text(ok)
                .ok_variant(variant.clone())
                .cancel_text(t.fs_dialog_cancel)
                .on_ok(move |_, _, cx| {
                    entity.update(cx, |this, cx| this.run_docker_action(action.clone(), cx));
                    true
                })
        });
    }

    /// Asks where to save the dump on this computer (the native Save dialog,
    /// starting in Downloads), then streams it there over SSH. Nothing is
    /// written on the server. The dump runs inside the container with its
    /// own root password variable, so no password passes through this app.
    fn start_docker_backup(&mut self, container: crate::docker::Container, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = container.db_kind() else { return; };
        if self.terminal().docker.running { return; }
        let file_name = crate::docker::backup_file_name(&container.name);
        let receiver = cx.prompt_for_new_path(&downloads_dir(), Some(&file_name));
        let action = crate::docker::Action::Backup { container: container.name.clone(), kind };
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else { return; };
            let _ = this.update_in(cx, |app, _, cx| {
                let title = action.title();
                if let Err(error) = app.terminal_mut().download_docker(action, path) {
                    let docker = &mut app.terminal_mut().docker;
                    if !docker.running {
                        docker.begin(title);
                        docker.running = false;
                        docker.result = Some(Err(error));
                    }
                }
                cx.notify();
            });
        }).detach();
    }

    fn open_docker_exec_dialog(&mut self, container: String, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.docker_exec_input.clone();
        let entity = cx.entity();
        let t = self.t();
        let lang = self.lang;
        let muted = cx.theme().muted_foreground;
        window.open_dialog(cx, move |dialog, _, _| {
            let entity = entity.clone();
            let input = input.clone();
            let container = container.clone();
            let submit_input = input.clone();
            dialog
                .title(i18n::docker_exec_title(lang, &container))
                .child(
                    v_flex().gap_2()
                        .child(Form::new().child(Field::new().label(t.docker_exec_label).child(Input::new(&input))))
                        .child(div().text_xs().text_color(muted).child(t.docker_exec_note)),
                )
                .footer(
                    gpui_kit::component::dialog::DialogFooter::new()
                        .gap_2()
                        .child(gpui_kit::component::dialog::DialogClose::new().child(Button::new("docker-exec-cancel").label(t.fs_dialog_cancel)))
                        .child(gpui_kit::component::dialog::DialogAction::new().child(
                            Button::new("docker-exec-run").primary().icon(Lucide::Play).label(t.docker_exec_run).on_click(move |_, window, cx| {
                                let command = submit_input.read(cx).value().trim().to_string();
                                if command.is_empty() { return; }
                                let action = crate::docker::Action::Exec { container: container.clone(), command };
                                entity.update(cx, |this, cx| this.run_docker_action(action, cx));
                                window.close_dialog(cx);
                            }),
                        )),
                )
        });
    }

    fn open_docker_network_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.docker_network_input.clone();
        let entity = cx.entity();
        let t = self.t();
        let muted = cx.theme().muted_foreground;
        window.open_dialog(cx, move |dialog, _, _| {
            let entity = entity.clone();
            let submit_input = input.clone();
            dialog
                .title(t.docker_network_title)
                .child(
                    v_flex().gap_2()
                        .child(Form::new().child(Field::new().label(t.docker_network_label).child(Input::new(&input))))
                        .child(div().text_xs().text_color(muted).child(t.docker_network_note)),
                )
                .footer(
                    gpui_kit::component::dialog::DialogFooter::new()
                        .gap_2()
                        .child(gpui_kit::component::dialog::DialogClose::new().child(Button::new("docker-network-cancel").label(t.fs_dialog_cancel)))
                        .child(gpui_kit::component::dialog::DialogAction::new().child(
                            Button::new("docker-network-create").primary().icon(Lucide::Network).label(t.docker_network_create).on_click(move |_, window, cx| {
                                let name = submit_input.read(cx).value().trim().to_string();
                                if name.is_empty() { return; }
                                entity.update(cx, |this, cx| this.run_docker_action(crate::docker::Action::NetworkCreate(name), cx));
                                window.close_dialog(cx);
                            }),
                        )),
                )
        });
    }

    fn open_db_browser(&mut self, container: String, kind: crate::docker::DbKind, cx: &mut Context<Self>) {
        let terminal = self.terminal_mut();
        terminal.db_browser.open(crate::docker_db::Target { container, kind });
        if terminal.db_browser.databases.is_none() {
            terminal.request_db(crate::docker_db::Query::Databases);
        }
        self.page = Page::DbBrowser;
        cx.notify();
    }

    fn db_toggle_database(&mut self, db: String, cx: &mut Context<Self>) {
        let terminal = self.terminal_mut();
        terminal.db_browser.sql_db = Some(db.clone());
        if !terminal.db_browser.expanded.remove(&db) {
            terminal.db_browser.expanded.insert(db.clone());
            if !terminal.db_browser.tables.contains_key(&db) {
                terminal.request_db(crate::docker_db::Query::Tables(db));
            }
        }
        cx.notify();
    }

    fn db_open_page(&mut self, db: String, table: crate::docker_db::Table, offset: usize, cx: &mut Context<Self>) {
        self.db_tab = 0;
        let terminal = self.terminal_mut();
        terminal.db_browser.sql_db = Some(db.clone());
        terminal.db_browser.selected = Some((db.clone(), table.clone()));
        terminal.db_browser.offset = offset;
        terminal.request_db(crate::docker_db::Query::Rows { db, table, offset });
        cx.notify();
    }

    /// Puts a starter statement in the SQL editor, using the selected
    /// table (and its columns, when a page is loaded) where it fits.
    fn db_insert_template(&mut self, which: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let browser = &self.terminal().db_browser;
        let Some(target) = &browser.target else { return; };
        let columns: Vec<String> = browser.rows.as_ref().filter(|_| browser.rows_current()).map(|rows| rows.columns.clone()).unwrap_or_default();
        let table = browser.selected.as_ref().map(|(_, table)| table.clone());
        let text = crate::docker_db::template(target.kind, which, table.as_ref().map(|table| (table, columns.as_slice())));
        self.db_sql_editor.update(cx, |state, cx| state.set_value(text, window, cx));
        cx.notify();
    }

    /// Read statements run straight away (read-only session). Anything else
    /// changes the database, so it is confirmed first with the target and
    /// the SQL shown, and a stronger warning for DROP/TRUNCATE/DELETE/ALTER.
    fn db_run_sql(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use gpui_kit::component::button::ButtonVariant;
        let sql = self.db_sql_editor.read(cx).value().to_string();
        if sql.trim().is_empty() { return; }
        let db = self.terminal().db_browser.sql_db.clone();
        if crate::docker_db::is_read_query(&sql) {
            self.terminal_mut().request_db(crate::docker_db::Query::Sql { db, sql, write: false });
            cx.notify();
            return;
        }
        let Some(container) = self.terminal().db_browser.target.as_ref().map(|target| target.container.clone()) else { return; };
        let t = self.t();
        let destructive = crate::docker_db::is_destructive(&sql);
        let host = self.active_host().map(|host| format!("{} ({}@{})", host.name, host.user, host.address)).unwrap_or_default();
        let shown: String = if sql.chars().count() > 600 { sql.chars().take(600).collect::<String>() + "…" } else { sql.clone() };
        let description = i18n::db_sql_confirm_desc(self.lang, &host, &container, db.as_deref(), destructive, shown.trim());
        let entity = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let entity = entity.clone();
            let (db, sql) = (db.clone(), sql.clone());
            alert
                .title(if destructive { t.db_sql_confirm_destructive_title } else { t.db_sql_confirm_title })
                .description(description.clone())
                .confirm()
                .ok_text(t.db_sql_run)
                .ok_variant(if destructive { ButtonVariant::Danger } else { ButtonVariant::Warning })
                .cancel_text(t.fs_dialog_cancel)
                .on_ok(move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.terminal_mut().request_db(crate::docker_db::Query::Sql { db: db.clone(), sql: sql.clone(), write: true });
                        cx.notify();
                    });
                    true
                })
        });
    }

    fn db_back_button(&self, cx: &mut Context<Self>) -> Button {
        Button::new("db-back").small().ghost().icon(Lucide::ArrowLeft).label(self.t().db_browser_back)
            .on_click(cx.listener(|this, _, _, cx| { this.page = Page::Docker; cx.notify(); }))
    }

    fn db_browser_empty(&self, description: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let (border, radius) = (cx.theme().border, cx.theme().radius);
        v_flex().size_full().p_4().gap_3().child(h_flex().child(self.db_back_button(cx))).child(
            div().w_full().p_4().rounded(radius).border_1().border_color(border).child(
                Empty::new().header(
                    EmptyHeader::new()
                        .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(Icon::new(Lucide::Database)))
                        .title(EmptyTitle::new().child(self.t().db_browser_heading))
                        .description(EmptyDescription::new().child(description)),
                ),
            ),
        ).into_any_element()
    }

    fn render_db_browser(&mut self, cx: &mut Context<Self>) -> AnyElement {
        use crate::docker::DbKind;
        let t = self.t();
        let lang = self.lang;
        let theme = cx.theme();
        let (muted, border, radius, blue, red, accent, active) =
            (theme.muted_foreground, theme.border, theme.radius, theme.blue, theme.red, theme.sidebar_accent, theme.list_active);
        if !self.terminal().connected {
            return self.db_browser_empty(t.docker_offline_hint, cx);
        }
        let Some(target) = self.terminal().db_browser.target.clone() else {
            return self.db_browser_empty(t.db_browser_pick_container, cx);
        };

        // New page of rows → reload the grid once (not every frame).
        let revision = self.terminal().db_browser.revision;
        if revision != self.db_grid_revision {
            self.db_grid_revision = revision;
            let grid = self.terminal().db_browser.rows.as_ref().map(DbGrid::from_rows).unwrap_or_default();
            self.db_grid.update(cx, |state, cx| {
                let has_rows = !grid.rows.is_empty();
                // A selected cell from the previous page may be out of range.
                state.clear_selection(cx);
                *state.delegate_mut() = grid;
                state.refresh(cx);
                if has_rows { state.scroll_to_row(0, cx); }
                cx.notify();
            });
        }
        let sql_revision = self.terminal().db_browser.sql_revision;
        if sql_revision != self.db_sql_revision {
            self.db_sql_revision = sql_revision;
            let grid = match &self.terminal().db_browser.sql_result {
                Some(crate::docker_db::SqlResult::Grid { columns, rows, .. }) => DbGrid::from_parts(columns, rows),
                _ => DbGrid::default(),
            };
            self.db_sql_grid.update(cx, |state, cx| {
                let has_rows = !grid.rows.is_empty();
                state.clear_selection(cx);
                *state.delegate_mut() = grid;
                state.refresh(cx);
                if has_rows { state.scroll_to_row(0, cx); }
                cx.notify();
            });
        }

        let browser = &self.terminal().db_browser;
        let loading = browser.loading;
        let engine = match target.kind { DbKind::MySql => "MariaDB / MySQL", DbKind::Postgres => "PostgreSQL" };
        let header = h_flex().w_full().gap_2().items_center().flex_wrap()
            .child(self.db_back_button(cx))
            .child(Icon::new(Lucide::Database).small().text_color(blue))
            .child(div().text_sm().font_bold().child(target.container.clone()))
            .child(Tag::secondary().small().child(engine))
            .child(Tag::success().small().child(t.db_browser_read_only))
            .child(div().flex_1())
            .when(loading, |row| row.child(Spinner::new().small().color(muted)))
            .child(Button::new("db-reload").small().icon(Lucide::RefreshCw).label(t.live_reload).disabled(loading)
                .on_click(cx.listener(|this, _, _, cx| {
                    let terminal = this.terminal_mut();
                    terminal.db_browser.databases = None;
                    terminal.db_browser.tables.clear();
                    terminal.request_db(crate::docker_db::Query::Databases);
                    if let Some((db, table)) = terminal.db_browser.selected.clone() {
                        let offset = terminal.db_browser.offset;
                        terminal.request_db(crate::docker_db::Query::Tables(db.clone()));
                        terminal.request_db(crate::docker_db::Query::Rows { db, table, offset });
                    }
                    cx.notify();
                })));

        // Left: databases, each opening to its tables.
        let mut tree = v_flex().w_full().gap_0p5().p_2();
        match &browser.databases {
            None => tree = tree.child(div().p_2().text_xs().text_color(muted).child(t.db_browser_loading)),
            Some(databases) if databases.is_empty() => tree = tree.child(div().p_2().text_xs().text_color(muted).child(t.db_browser_no_databases)),
            Some(databases) => {
                for db in databases {
                    let expanded = browser.expanded.contains(db);
                    let system = matches!(db.as_str(), "information_schema" | "performance_schema" | "mysql" | "sys" | "postgres");
                    let toggle = db.clone();
                    tree = tree.child(
                        h_flex().id(SharedString::from(format!("db-{db}"))).w_full().gap_1p5().items_center().px_1().py_0p5().rounded(radius)
                            .cursor_pointer().hover(move |style| style.bg(accent))
                            .on_click(cx.listener(move |this, _, _, cx| this.db_toggle_database(toggle.clone(), cx)))
                            .child(Icon::new(if expanded { Lucide::ChevronDown } else { Lucide::ChevronRight }).small().text_color(muted))
                            .child(Icon::new(Lucide::Database).small().text_color(if system { muted } else { blue }))
                            .child(div().flex_1().min_w_0().truncate().text_sm().when(system, |el| el.text_color(muted)).child(db.clone())),
                    );
                    if !expanded { continue; }
                    match browser.tables.get(db) {
                        None => tree = tree.child(div().pl(px(34.)).py_0p5().text_xs().text_color(muted).child(t.db_browser_loading)),
                        Some(tables) if tables.is_empty() => tree = tree.child(div().pl(px(34.)).py_0p5().text_xs().text_color(muted).child(t.db_browser_no_tables)),
                        Some(tables) => {
                            for table in tables {
                                let selected = browser.selected.as_ref().is_some_and(|(sel_db, sel)| sel_db == db && sel.schema == table.schema && sel.name == table.name);
                                let (open_db, open_table) = (db.clone(), table.clone());
                                tree = tree.child(
                                    h_flex().id(SharedString::from(format!("db-{db}-{}", table.label()))).w_full().gap_1p5().items_center().pl(px(30.)).pr_1().py_0p5()
                                        .rounded(radius).cursor_pointer()
                                        .when(selected, |el| el.bg(active))
                                        .when(!selected, |el| el.hover(move |style| style.bg(accent)))
                                        .on_click(cx.listener(move |this, _, _, cx| this.db_open_page(open_db.clone(), open_table.clone(), 0, cx)))
                                        .child(Icon::new(if table.view { Lucide::Eye } else { Lucide::Table }).small().text_color(muted))
                                        .child(div().flex_1().min_w_0().truncate().text_sm().child(table.label()))
                                        .children(table.estimate.map(|rows| div().flex_shrink_0().text_xs().text_color(muted).child(compact_count(rows)))),
                                );
                            }
                        }
                    }
                }
            }
        }

        // Right: the selected table's current page.
        let data: AnyElement = match &browser.selected {
            None => div().size_full().p_4().child(
                Empty::new().header(
                    EmptyHeader::new()
                        .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(Icon::new(Lucide::Table)))
                        .title(EmptyTitle::new().child(t.db_browser_pick_table))
                        .description(EmptyDescription::new().child(t.db_browser_pick_table_desc)),
                ),
            ).into_any_element(),
            Some((db, table)) => {
                let current = browser.rows_current();
                let page = browser.rows.as_ref().filter(|_| current);
                let offset = browser.offset;
                let count = page.map_or(0, |page| page.rows.len());
                let more = page.is_some_and(|page| page.more);
                let (prev_db, prev_table) = (db.clone(), table.clone());
                let (next_db, next_table) = (db.clone(), table.clone());
                let bar = h_flex().w_full().gap_2().items_center().flex_wrap().px_3().py_2().border_b_1().border_color(border)
                    .child(Icon::new(if table.view { Lucide::Eye } else { Lucide::Table }).small().text_color(muted))
                    .child(div().text_sm().font_bold().child(format!("{db} › {}", table.label())))
                    .child(div().flex_1())
                    .child(div().text_xs().text_color(muted).child(if page.is_some() {
                        i18n::db_browser_range(lang, offset, count)
                    } else {
                        t.db_browser_loading.to_string()
                    }))
                    .child(Button::new("db-prev").small().icon(Lucide::ChevronLeft).disabled(offset == 0 || loading)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let offset = offset.saturating_sub(crate::docker_db::PAGE);
                            this.db_open_page(prev_db.clone(), prev_table.clone(), offset, cx);
                        })))
                    .child(Button::new("db-next").small().icon(Lucide::ChevronRight).disabled(!more || loading)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.db_open_page(next_db.clone(), next_table.clone(), offset + crate::docker_db::PAGE, cx);
                        })));
                let body: AnyElement = match page {
                    Some(page) if page.columns.is_empty() || page.rows.is_empty() => div().p_4().text_sm().text_color(muted).child(t.db_browser_no_rows).into_any_element(),
                    Some(_) => grid_with_shift_scroll(&self.db_grid),
                    None => div().p_4().child(Spinner::new().small().color(muted)).into_any_element(),
                };
                v_flex().size_full().min_w_0()
                    .child(bar)
                    .child(div().flex_1().min_h_0().min_w_0().w_full().child(body))
                    .child(div().px_3().py_1().border_t_1().border_color(border).text_xs().text_color(muted).child(t.db_browser_shift_hint))
                    .into_any_element()
            }
        };

        // SQL tab: an editor for any statement, with starters.
        let sql_panel: AnyElement = {
            let db_label = browser.sql_db.clone();
            let running = browser.sql_running;
            let mut templates = h_flex().gap_1().flex_wrap();
            for (id, label, which) in [("db-tpl-select", "SELECT", "select"), ("db-tpl-create", "CREATE TABLE", "create"), ("db-tpl-insert", "INSERT", "insert"), ("db-tpl-update", "UPDATE", "update"), ("db-tpl-delete", "DELETE", "delete")] {
                templates = templates.child(Button::new(id).xsmall().ghost().label(label).tooltip(t.db_sql_template_tooltip)
                    .on_click(cx.listener(move |this, _, window, cx| this.db_insert_template(which, window, cx))));
            }
            let bar = h_flex().w_full().gap_2().items_center().flex_wrap().px_3().py_2().border_b_1().border_color(border)
                .child(Icon::new(Lucide::Database).small().text_color(muted))
                .child(match &db_label {
                    Some(db) => Tag::info().small().child(db.clone()).into_any_element(),
                    None => div().text_xs().text_color(muted).child(t.db_sql_no_database).into_any_element(),
                })
                .child(templates)
                .child(div().flex_1())
                .when(running, |row| row.child(Spinner::new().small().color(muted)))
                .child(Button::new("db-sql-run").small().primary().icon(Lucide::Play).label(t.db_sql_run).disabled(running)
                    .on_click(cx.listener(|this, _, window, cx| this.db_run_sql(window, cx))));
            let result: AnyElement = match (&browser.sql_error, &browser.sql_result) {
                _ if running => h_flex().p_4().gap_2().items_center().child(Spinner::new().small().color(muted))
                    .child(div().text_sm().text_color(muted).child(t.db_sql_running)).into_any_element(),
                (Some(error), _) => div().p_3().child(notice_row(Lucide::TriangleAlert, red, radius, error.clone())).into_any_element(),
                (None, Some(crate::docker_db::SqlResult::Message(message))) => div().p_3()
                    .child(notice_row(Lucide::CircleCheck, cx.theme().green, radius, message.clone())).into_any_element(),
                (None, Some(crate::docker_db::SqlResult::Grid { columns, rows, truncated })) if !columns.is_empty() => v_flex().size_full().min_w_0()
                    .child(div().px_3().py_1().text_xs().text_color(muted).child(i18n::db_sql_rows(lang, rows.len(), *truncated)))
                    .child(div().flex_1().min_h_0().min_w_0().w_full().child(grid_with_shift_scroll(&self.db_sql_grid)))
                    .into_any_element(),
                (None, Some(_)) => div().p_4().text_sm().text_color(muted).child(t.db_sql_no_result).into_any_element(),
                (None, None) => div().p_4().text_xs().text_color(muted).child(t.db_sql_hint).into_any_element(),
            };
            v_flex().size_full().min_w_0()
                .child(bar)
                .child(div().h(px(200.)).flex_shrink_0().w_full().border_b_1().border_color(border)
                    .child(gpui_kit::component::input::Editor::new(&self.db_sql_editor).h_full()))
                .child(div().flex_1().min_h_0().min_w_0().w_full().child(result))
                .into_any_element()
        };

        let tabs = h_flex().w_full().gap_1().px_2().pt_2()
            .child(Button::new("db-tab-data").small().ghost().icon(Lucide::Table).label(t.db_tab_data).selected(self.db_tab == 0)
                .on_click(cx.listener(|this, _, _, cx| { this.db_tab = 0; cx.notify(); })))
            .child(Button::new("db-tab-sql").small().ghost().icon(Lucide::SquareTerminal).label(t.db_tab_sql).selected(self.db_tab == 1)
                .on_click(cx.listener(|this, _, _, cx| { this.db_tab = 1; cx.notify(); })));
        let right = v_flex().size_full().min_w_0()
            .child(tabs)
            .child(div().flex_1().min_h_0().min_w_0().w_full().child(if self.db_tab == 0 { data } else { sql_panel }));

        let mut content = v_flex().size_full().p_4().gap_3().child(header);
        if let Some(error) = &browser.error {
            content = content.child(notice_row(Lucide::TriangleAlert, red, radius, error.clone()));
        }
        content
            .child(
                div().flex_1().min_h_0().w_full().child(
                    h_resizable("db-browser-split")
                        .child(resizable_panel().size(px(280.)).size_range(px(200.)..px(520.)).child(
                            div().size_full().rounded(radius).border_1().border_color(border).overflow_y_scrollbar().child(tree),
                        ))
                        // `min_w_0` lets the panel be narrower than a wide
                        // table, so the table scrolls inside it instead of
                        // being cut off at the edge.
                        .child(resizable_panel().child(
                            div().size_full().min_w_0().rounded(radius).border_1().border_color(border).overflow_hidden().child(right),
                        )),
                ),
            )
            .into_any_element()
    }

    fn render_docker(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.t();
        let lang = self.lang;
        let muted = cx.theme().muted_foreground;
        let radius = cx.theme().radius;
        let connected = self.terminal().connected;
        if !connected {
            return v_flex().size_full().p_4().child(
                div().w_full().p_4().rounded(radius).border_1().border_color(cx.theme().border).child(
                    Empty::new().header(
                        EmptyHeader::new()
                            .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(Icon::new(Lucide::Container)))
                            .title(EmptyTitle::new().child(t.docker_heading))
                            .description(EmptyDescription::new().child(t.docker_offline_hint)),
                    ),
                ),
            ).into_any_element();
        }
        {
            let docker = &self.terminal().docker;
            let need_list = docker.containers.is_none() && docker.list_error.is_none() && !docker.listing;
            let need_stats = docker.stats_at.is_none() && !docker.stats_loading;
            if need_list { self.terminal_mut().request_docker_list(); }
            // One sample when the page opens; Auto-refresh keeps it current.
            if need_stats { self.terminal_mut().request_docker_stats(); }
        }
        let revision = self.terminal().docker.revision;
        if revision != self.docker_seen_revision {
            self.docker_seen_revision = revision;
            self.docker_output_scroll.scroll_to_bottom();
        }

        let docker = &self.terminal().docker;
        let busy = docker.running;
        let listing = docker.listing;
        let stats = docker.stats.clone();
        let needle = self.docker_search.read(cx).value().trim().to_lowercase();

        let mut header = h_flex().w_full().gap_2().items_center().flex_wrap()
            .child(div().text_xs().font_bold().text_color(cx.theme().green).child(t.live_badge))
            .child(Button::new("docker-refresh").small().icon(Lucide::RefreshCw).label(t.live_reload).disabled(listing)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.terminal_mut().request_docker_list();
                    this.terminal_mut().request_docker_stats();
                    cx.notify();
                })))
            .when(listing || docker.stats_loading, |row| row.child(Spinner::new().small().color(muted)))
            .child(div().w(px(260.)).child(Input::new(&self.docker_search)))
            .child(Button::new("docker-network").small().icon(Lucide::Network).label(t.docker_network_create).disabled(busy)
                .tooltip(t.docker_network_tooltip)
                .on_click(cx.listener(|this, _, window, cx| this.open_docker_network_dialog(window, cx))));
        if let Some(containers) = &docker.containers {
            let running = containers.iter().filter(|c| c.running()).count();
            header = header.child(div().text_xs().text_color(muted).child(i18n::docker_count(lang, containers.len(), running)));
        }

        // Totals and ordering, so the heaviest containers stand out.
        let mut usage_bar = h_flex().w_full().gap_2().items_center().flex_wrap();
        let mut sort = h_flex().gap_1().items_center().child(div().text_xs().text_color(muted).child(t.docker_sort_label));
        for (value, label) in [(0u8, t.docker_sort_project), (1, "CPU"), (2, "RAM")] {
            sort = sort.child(Button::new(SharedString::from(format!("docker-sort-{value}"))).xsmall().ghost().label(label)
                .selected(self.docker_sort == value)
                .on_click(cx.listener(move |this, _, _, cx| { this.docker_sort = value; cx.notify(); })));
        }
        usage_bar = usage_bar.child(sort);
        if let Some(stats) = &stats {
            let cpu: f32 = stats.usage.values().map(|usage| usage.cpu).sum();
            let mem: u64 = stats.usage.values().map(|usage| usage.mem_bytes).sum();
            usage_bar = usage_bar.child(div().text_xs().text_color(muted).child(
                i18n::docker_usage_total(lang, cpu, stats.cores * 100, &crate::docker::size_text(mem)),
            ));
        }
        if let Some(error) = &docker.stats_error {
            usage_bar = usage_bar.child(div().flex_1().min_w_0().text_xs().text_color(cx.theme().red).truncate().child(error.clone()));
        } else if !self.live_auto_refresh {
            usage_bar = usage_bar.child(div().text_xs().text_color(muted).child(t.docker_stats_manual));
        }

        let mut list = v_flex().w_full().gap_3();
        if docker.permission_denied() {
            list = list.child(
                v_flex().gap_2()
                    .child(notice_row(Lucide::CircleAlert, cx.theme().yellow, radius, t.docker_permission_hint))
                    .child(h_flex().child(Button::new("docker-setup-group").small().icon(Lucide::KeyRound).label(t.docker_permission_setup)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run_in_terminal("sudo usermod -aG docker \"$USER\" && echo 'Akses Docker siap. Klik Muat ulang di halaman Docker.'".into(), window, cx);
                        })))),
            );
        } else if let Some(error) = &docker.list_error {
            list = list.child(notice_row(Lucide::CircleAlert, cx.theme().red, radius, error.clone()));
        }

        match &docker.containers {
            None if listing => list = list.child(div().text_sm().text_color(muted).child(t.docker_loading)),
            None => {}
            Some(containers) if containers.is_empty() => list = list.child(div().text_sm().text_color(muted).child(t.docker_empty)),
            Some(containers) => {
                let containers: Vec<_> = containers.iter()
                    .filter(|c| needle.is_empty() || [&c.name, &c.image, &c.project, &c.service].iter().any(|v| v.to_lowercase().contains(&needle)))
                    .cloned().collect();
                if containers.is_empty() {
                    list = list.child(div().text_sm().text_color(muted).child(t.docker_no_match));
                }
                if self.docker_sort == 0 {
                    // Already sorted by project, so each group is one contiguous run.
                    let mut start = 0;
                    while start < containers.len() {
                        let project = containers[start].project.clone();
                        let end = containers[start..].iter().position(|c| c.project != project).map_or(containers.len(), |n| start + n);
                        list = list.child(self.render_docker_group(&containers[start..end], busy, stats.as_ref(), cx));
                        start = end;
                    }
                } else if !containers.is_empty() {
                    // One flat list, heaviest first (stopped containers last).
                    let mut sorted = containers;
                    let usage = |c: &crate::docker::Container| stats.as_ref().and_then(|stats| stats.usage.get(&c.name));
                    if self.docker_sort == 1 {
                        sorted.sort_by(|a, b| usage(b).map_or(-1., |u| u.cpu).total_cmp(&usage(a).map_or(-1., |u| u.cpu)));
                    } else {
                        sorted.sort_by_key(|c| std::cmp::Reverse(usage(c).map_or(0, |u| u.mem_bytes)));
                    }
                    let mut card = v_flex().w_full().gap_2().p_3().rounded(radius).border_1().border_color(cx.theme().border)
                        .child(h_flex().gap_2().items_center()
                            .child(Icon::new(Lucide::Activity).small().text_color(muted))
                            .child(div().text_sm().font_bold().child(if self.docker_sort == 1 { t.docker_sorted_cpu } else { t.docker_sorted_ram })));
                    for container in &sorted {
                        card = card.child(self.render_docker_container(container, busy, stats.as_ref(), true, cx));
                    }
                    list = list.child(card);
                }
            }
        }

        let output = self.render_docker_output(cx);
        v_flex().size_full().p_4().gap_3()
            .child(header)
            .child(usage_bar)
            .child(div().flex_1().min_h_0().w_full().overflow_y_scrollbar().child(list))
            .children(output)
            .into_any_element()
    }

    /// One Compose project (or the standalone containers) as a card.
    #[inline(never)]
    fn render_docker_group(&self, containers: &[crate::docker::Container], busy: bool, stats: Option<&crate::docker::Stats>, cx: &mut Context<Self>) -> AnyElement {
        let t = self.t();
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let radius = cx.theme().radius;
        let project = containers.iter().find_map(|c| c.project());
        let mut card = v_flex().w_full().gap_2().p_3().rounded(radius).border_1().border_color(border);
        let mut title = h_flex().w_full().gap_2().items_center().flex_wrap()
            .child(Icon::new(if project.is_some() { Lucide::Layers } else { Lucide::Container }).small().text_color(muted))
            .child(div().text_sm().font_bold().child(match &project {
                Some(project) => project.name.clone(),
                None if containers[0].project.is_empty() => t.docker_standalone.to_string(),
                None => containers[0].project.clone(),
            }));
        if let Some(project) = project {
            title = title.child(div().flex_1().min_w_0().text_xs().font_family("monospace").text_color(muted).truncate().child(project.working_dir.clone()));
            let up = crate::docker::Action::ComposeUp { project: project.clone(), service: None };
            let down = crate::docker::Action::ComposeDown(project.clone());
            title = title
                .child(Button::new(SharedString::from(format!("dk-up-{}", project.name))).small().icon(Lucide::Hammer).label(t.docker_rebuild).disabled(busy)
                    .tooltip(t.docker_rebuild_tooltip)
                    .on_click(cx.listener(move |this, _, window, cx| this.confirm_docker_action(up.clone(), window, cx))))
                .child(Button::new(SharedString::from(format!("dk-down-{}", project.name))).small().danger().icon(Lucide::PowerOff).label(t.docker_down).disabled(busy)
                    .on_click(cx.listener(move |this, _, window, cx| this.confirm_docker_action(down.clone(), window, cx))));
        }
        card = card.child(title);
        for container in containers {
            card = card.child(self.render_docker_container(container, busy, stats, false, cx));
        }
        card.into_any_element()
    }

    /// `show_project`: in the flat CPU/RAM-sorted list, each row names its
    /// Compose project since the project cards are not shown.
    #[inline(never)]
    fn render_docker_container(&self, container: &crate::docker::Container, busy: bool, stats: Option<&crate::docker::Stats>, show_project: bool, cx: &mut Context<Self>) -> AnyElement {
        use crate::docker::Action;
        let t = self.t();
        let muted = cx.theme().muted_foreground;
        let theme = cx.theme();
        let color = match container.state.as_str() {
            "running" => theme.green,
            "restarting" | "paused" | "created" => theme.yellow,
            "exited" | "dead" => theme.red,
            _ => muted,
        };
        let name = container.name.clone();
        let running = container.running();
        let id = |action: &str| SharedString::from(format!("dk-{action}-{name}"));

        let mut actions = h_flex().gap_1().flex_wrap().flex_shrink_0();
        let logs = Action::Logs(name.clone());
        actions = actions.child(Button::new(id("logs")).small().ghost().icon(Lucide::ScrollText).tooltip(t.docker_logs).disabled(busy)
            .on_click(cx.listener(move |this, _, _, cx| this.run_docker_action(logs.clone(), cx))));
        if running {
            let restart = Action::Restart(name.clone());
            let stop = Action::Stop(name.clone());
            let exec_name = name.clone();
            actions = actions
                .child(Button::new(id("restart")).small().ghost().icon(Lucide::RotateCw).tooltip(t.docker_restart).disabled(busy)
                    .on_click(cx.listener(move |this, _, window, cx| this.confirm_docker_action(restart.clone(), window, cx))))
                .child(Button::new(id("stop")).small().ghost().icon(Lucide::Square).tooltip(t.docker_stop).disabled(busy)
                    .on_click(cx.listener(move |this, _, window, cx| this.confirm_docker_action(stop.clone(), window, cx))))
                .child(Button::new(id("exec")).small().ghost().icon(Lucide::SquareTerminal).tooltip(t.docker_exec).disabled(busy)
                    .on_click(cx.listener(move |this, _, window, cx| this.open_docker_exec_dialog(exec_name.clone(), window, cx))));
            if let Some(command) = container.reload_command() {
                let reload = Action::Exec { container: name.clone(), command: command.into() };
                actions = actions.child(Button::new(id("reload")).small().ghost().icon(Lucide::RefreshCw).tooltip(i18n::docker_reload_tooltip(self.lang, command)).disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| this.run_docker_action(reload.clone(), cx))));
            }
            if let Some(kind) = container.db_kind() {
                let browse_name = name.clone();
                actions = actions.child(Button::new(id("browse")).small().ghost().icon(Lucide::Table).tooltip(t.docker_browse_data)
                    .on_click(cx.listener(move |this, _, _, cx| this.open_db_browser(browse_name.clone(), kind, cx))));
                let target = container.clone();
                actions = actions.child(Button::new(id("backup")).small().ghost().icon(Lucide::DatabaseBackup).tooltip(t.docker_backup).disabled(busy)
                    .on_click(cx.listener(move |this, _, window, cx| this.start_docker_backup(target.clone(), window, cx))));
            }
        } else {
            let start = Action::Start(name.clone());
            actions = actions.child(Button::new(id("start")).small().ghost().icon(Lucide::Play).tooltip(t.docker_start).disabled(busy)
                .on_click(cx.listener(move |this, _, _, cx| this.run_docker_action(start.clone(), cx))));
        }
        if let (Some(project), false) = (container.project(), container.service.is_empty()) {
            let rebuild = Action::ComposeUp { project, service: Some(container.service.clone()) };
            actions = actions.child(Button::new(id("rebuild")).small().ghost().icon(Lucide::Hammer).tooltip(i18n::docker_rebuild_service(self.lang, &container.service)).disabled(busy)
                .on_click(cx.listener(move |this, _, window, cx| this.confirm_docker_action(rebuild.clone(), window, cx))));
        }

        let mut details = v_flex().flex_1().min_w_0().gap_0p5()
            .child(h_flex().gap_2().items_center().flex_wrap()
                .child(div().w(px(8.)).h(px(8.)).rounded(px(4.)).bg(color).flex_shrink_0())
                .child(div().text_sm().font_bold().child(container.name.clone()))
                .when(show_project && !container.project.is_empty(), |row| row.child(Tag::secondary().small().child(container.project.clone())))
                .child(div().text_xs().text_color(color).child(container.status.clone())))
            .child(div().text_xs().font_family("monospace").text_color(muted).truncate().child(container.image.clone()));
        if !container.ports.is_empty() {
            details = details.child(div().text_xs().font_family("monospace").text_color(muted).child(container.ports.clone()));
        }
        // CPU is per core in Docker (up to 100% × cores), so its bar is
        // scaled to the whole host; RAM is against the container's limit.
        if let (true, Some(stats)) = (running, stats) {
            if let Some(usage) = stats.usage.get(&container.name) {
                let capacity = (stats.cores * 100) as f32;
                details = details.child(
                    h_flex().pt_1().gap_4().items_center().flex_wrap()
                        .child(usage_meter("CPU", usage.cpu / capacity, format!("{:.1}%", usage.cpu), cx))
                        .child(usage_meter("RAM", usage.mem_percent / 100., format!("{} ({:.1}%)", usage.mem_text, usage.mem_percent), cx))
                        .child(div().text_xs().text_color(muted).child(i18n::docker_io(self.lang, &usage.net, &usage.block))),
                );
            }
        }
        h_flex().w_full().gap_3().items_start().p_2().rounded(theme.radius).bg(theme.muted.opacity(0.35))
            .child(details)
            .child(actions)
            .into_any_element()
    }

    /// Docked output of the current/last action; hidden until one runs.
    fn render_docker_output(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.t();
        let docker = &self.terminal().docker;
        if docker.title.is_empty() { return None; }
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let mut bar = h_flex().w_full().gap_2().items_center()
            .child(Icon::new(Lucide::SquareTerminal).small().text_color(muted))
            .child(div().flex_1().min_w_0().text_xs().font_family("monospace").truncate().child(docker.title.clone()));
        bar = match &docker.result {
            // Logs only stream, so Close stops them and hides the panel.
            _ if docker.running && docker.follow => bar
                .child(Spinner::new().small().color(muted))
                .child(div().text_xs().text_color(muted).child(t.docker_following))
                .child(Button::new("docker-close-logs").small().icon(Lucide::X).label(t.docker_close)
                    .on_click(cx.listener(|this, _, _, cx| { this.terminal_mut().close_docker_output(); cx.notify(); }))),
            _ if docker.running => bar
                .child(Spinner::new().small().color(muted))
                .child(div().text_xs().text_color(muted).child(match (docker.stopping, docker.downloaded) {
                    (true, _) => t.docker_stopping.to_string(),
                    (false, Some(bytes)) => i18n::docker_downloading(self.lang, &crate::docker::size_text(bytes)),
                    (false, None) => t.docker_running.to_string(),
                }))
                .child(Button::new("docker-stop-task").small().danger().icon(Lucide::CircleStop).label(t.docker_stop_task).disabled(docker.stopping)
                    .on_click(cx.listener(|this, _, _, cx| { this.terminal_mut().stop_docker(); cx.notify(); }))),
            // A finished backup: open its folder in the system file manager.
            Some(Ok(())) if docker.saved_to.is_some() => {
                let path = docker.saved_to.clone().unwrap_or_default();
                bar.child(Tag::success().small().child(t.docker_done))
                    .child(Button::new("docker-reveal-backup").small().icon(Lucide::FolderOpen).label(t.docker_show_backup)
                        .on_click(move |_, _, cx| cx.reveal_path(&path)))
            }
            Some(Ok(())) => bar.child(Tag::success().small().child(t.docker_done)),
            Some(Err(error)) => bar.child(div().max_w(px(360.)).text_xs().text_color(theme.red).truncate().child(error.clone())),
            None => bar,
        };
        if !docker.running {
            bar = bar.child(Button::new("docker-clear-output").small().ghost().icon(Lucide::X).tooltip(t.docker_close_output)
                .on_click(cx.listener(|this, _, _, cx| { this.terminal_mut().close_docker_output(); cx.notify(); })));
        }
        let text = if docker.output.is_empty() { t.docker_no_output.to_string() } else { docker.output.clone() };
        Some(
            v_flex().w_full().h(px(300.)).flex_shrink_0().gap_2().p_3().rounded(theme.radius).border_1().border_color(theme.border).bg(theme.secondary)
                .child(bar)
                .child(
                    div().relative().flex_1().min_h_0().w_full()
                        .child(
                            div().id("docker-output").size_full().overflow_y_scroll().track_scroll(&self.docker_output_scroll)
                                .child(div().w_full().text_xs().font_family("monospace").text_color(if docker.output.is_empty() { muted } else { theme.foreground }).child(text)),
                        )
                        .vertical_scrollbar(&self.docker_output_scroll),
                )
                .into_any_element(),
        )
    }

    fn request_git_auth(&mut self, request: crate::git_auth::Request, cx: &mut Context<Self>) {
        self.git_auth.error = None;
        if let Err(error) = self.terminal_mut().request_git(request) {
            self.git_auth.error = Some(error);
        }
        cx.notify();
    }

    fn handle_git_result(&mut self, result: Result<crate::git_auth::Reply, String>, cx: &mut Context<Self>) {
        use crate::git_auth::Reply;
        match result {
            // A reply for a provider the user has since switched away from is
            // stale and would show the wrong account.
            Ok(Reply::Status(status)) if status.provider == self.git_auth.provider => {
                if status.known_host { self.git_auth.fingerprint = None; }
                self.git_auth.status = Some(status);
                self.git_auth.error = None;
            }
            Ok(Reply::Fingerprint(provider, fingerprint)) if provider == self.git_auth.provider => {
                self.git_auth.fingerprint = Some(fingerprint);
                self.git_auth.error = None;
            }
            Ok(_) => {}
            Err(error) => self.git_auth.error = Some(error),
        }
        cx.notify();
    }

    fn set_git_provider(&mut self, provider: crate::git_auth::Provider, cx: &mut Context<Self>) {
        if self.git_auth.provider == provider { return; }
        self.git_auth = GitAuthUi { provider, ..GitAuthUi::default() };
        self.request_git_auth(crate::git_auth::Request::Status(provider), cx);
    }

    /// Checks the current provider's login once per host (the result is
    /// cached in `git_auth` until the host changes or the user re-checks).
    fn ensure_git_auth_checked(&mut self, cx: &mut Context<Self>) {
        if self.git_auth.status.is_none() && self.git_auth.error.is_none() && !self.terminal().git_busy && self.terminal().connected {
            let provider = self.git_auth.provider;
            self.request_git_auth(crate::git_auth::Request::Status(provider), cx);
        }
    }

    fn open_git_account_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_git_auth_checked(cx);
        let entity = cx.entity();
        let t = self.t();
        let lang = self.lang;
        window.open_dialog(cx, move |dialog, _, cx| {
            // Rebuilt every frame while open, so it follows each wizard step
            // (scan → trust → keygen → test) as the server replies arrive.
            let body = git_account_body(&entity, t, lang, cx);
            dialog
                .title(t.git_account_title)
                .width(px(580.))
                .child(body)
                .footer(
                    gpui_kit::component::dialog::DialogFooter::new()
                        .child(gpui_kit::component::dialog::DialogClose::new().child(Button::new("git-account-close").label(t.git_account_close))),
                )
        });
    }

    fn render_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let host = self.editor_host_string();
        let online = self.terminal().connected && host.is_some();
        let owner = host.clone().unwrap_or_default();
        if online {
            if self.explorer_owner != owner {
                self.explorer_initialized = false;
                self.explorer_nodes.clear();
                self.explorer_expanded.clear();
                self.explorer_error = None;
                self.explorer_owner = owner;
                self.fs_clipboard = None;
                self.fs_message = None;
                self.import_queue.clear();
                self.import_total = 0;
                self.import_reveal.clear();
                self.git_clone_target = None;
                self.git_auth = GitAuthUi::default();
                self.explorer_selected = None;
                self.compose_pending = None;
            }
            if !self.explorer_initialized && !self.terminal().file_busy {
                self.explorer_initialized = true;
                self.explorer_expanded.insert("~".into());
                self.fetch_tree_node("~".into(), String::new(), window, cx);
            }
        } else {
            self.explorer_initialized = false;
            self.explorer_nodes.clear();
            self.explorer_expanded.clear();
        }

        // `start_maintenance` sets `update_running` synchronously, so this
        // only fires once the clone's channel has actually ended (success,
        // failure, or disconnect); waits for the listing channel to be free
        // so the refresh below is not silently skipped.
        if self.git_clone_target.is_some() && !self.terminal().update_running && !self.terminal().file_busy {
            if let Some((parent, target)) = self.git_clone_target.take() {
                let result = self.terminal().update_result.clone().unwrap_or_default();
                if self.terminal().update_ok {
                    self.set_fs_info(result);
                    self.explorer_expanded.insert(target);
                } else {
                    // Only the success path expands the target: on failure
                    // the folder usually does not exist, and listing it
                    // surfaced as a confusing "No such file" Explorer error.
                    self.set_fs_error(format!("{result} {}", self.t().git_clone_failed_hint));
                }
                self.explorer_expanded.insert(parent);
                self.refresh_tree(window, cx);
            }
        }

        // The Move/Chmod dialog was closed (Cancel, X, or overlay click).
        if (self.move_picker.is_some() || self.chmod_ui.is_some()) && !window.has_active_dialog(cx) {
            self.move_picker = None;
            self.chmod_ui = None;
        }

        let panel = self.render_explorer_panel(cx);
        let document = self.render_editor_document(online, host.as_deref(), window, cx);

        v_flex().size_full().p_4().gap_3()
            .child(
                div().flex_1().min_h_0().w_full().child(
                    h_resizable("file-editor-split")
                        .child(resizable_panel().size(px(320.)).size_range(px(220.)..px(560.)).child(v_flex().h_full().overflow_y_scrollbar().child(panel)))
                        .child(resizable_panel().child(div().flex_1().min_w_0().h_full().child(document))),
                ),
            )
            .into_any_element()
    }

    fn render_explorer_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.t();
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let radius = cx.theme().radius;
        if !self.terminal().connected {
            return div().w_full().p_4().rounded(radius).border_1().border_color(border)
                .child(
                    Empty::new().header(
                        EmptyHeader::new()
                            .media(
                                EmptyMedia::new().with_variant(EmptyMediaVariant::Icon)
                                    .child(Icon::new(Lucide::FolderOpen)),
                            )
                            .title(EmptyTitle::new().child(t.editor_explorer_heading))
                            .description(EmptyDescription::new().child(t.editor_connect_hint)),
                    ),
                )
                .into_any_element();
        }
        let enabled = !self.terminal().file_busy;
        // The file-manager toolbar/context menu run on their own SSH channel
        // (`Terminal::fs_busy`), independent of the tree-navigation/open-file
        // channel above, so they get their own enabled flag rather than
        // blocking on (or being blocked by) an unrelated read/save.
        let fs_enabled = !self.terminal().fs_busy;
        // Git clone runs on the maintenance channel (Update System, app
        // installs), which allows only one operation at a time.
        let can_clone = self.terminal().can_maintain();
        let root_path = if self.explorer_root.is_empty() { "~".to_string() } else { self.explorer_root.clone() };
        let header = h_flex().items_center().gap_2()
            .child(Icon::new(Lucide::FolderTree).text_color(cx.theme().blue))
            .child(div().flex_1().text_sm().font_bold().child(t.editor_explorer_heading))
            .child(Button::new("exp-refresh").small().ghost().disabled(!enabled).icon(Lucide::RefreshCw).tooltip(t.live_reload)
                .on_click(cx.listener(|this, _, window, cx| this.refresh_tree(window, cx))));
        // Where New file / New folder / Import / Git clone / Paste will land.
        // Shown in the breadcrumb (last 3 segments, so deep paths still fit);
        // clicking it goes back to targeting the root.
        let target_dir = self.explorer_target_dir();
        let segments: Vec<&str> = target_dir.split('/').filter(|segment| !segment.is_empty()).collect();
        let shown: Vec<SharedString> = if segments.len() > 3 {
            std::iter::once(SharedString::from("…"))
                .chain(segments[segments.len() - 3..].iter().map(|segment| SharedString::from(segment.to_string())))
                .collect()
        } else {
            segments.iter().map(|segment| SharedString::from(segment.to_string())).collect()
        };
        let target_tooltip = i18n::explorer_target_tooltip(self.lang, &target_dir);
        let root_row = h_flex().id("explorer-target").gap_2().items_center().px_2().py_1().rounded(radius).bg(cx.theme().muted)
            .cursor_pointer()
            .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(target_tooltip.clone()).build(window, cx))
            .on_click(cx.listener(|this, _, _, cx| { this.explorer_selected = None; cx.notify(); }))
            .child(Icon::new(if self.explorer_selected.is_some() { Lucide::FolderOpen } else { Lucide::HardDrive }).small().text_color(muted))
            .child(
                div().flex_1().min_w_0().child(
                    Breadcrumb::new().text_xs().font_family("monospace").children(shown.into_iter().map(BreadcrumbItem::new)),
                ),
            );
        let toolbar_root = target_dir.clone();
        let paste_root = target_dir.clone();
        // Icon-only with tooltips so all actions fit on one line of a narrow
        // panel; `flex_wrap` still lets them flow if it gets narrower still.
        let mut toolbar = h_flex().w_full().flex_wrap().items_center().gap_1()
            .child(Button::new("exp-new-file").small().ghost().icon(Lucide::FilePlus).tooltip(t.fs_toolbar_new_file).disabled(!fs_enabled)
                .on_click(cx.listener({
                    let parent = toolbar_root.clone();
                    move |this, _, window, cx| this.open_new_file_dialog(parent.clone(), window, cx)
                })))
            .child(Button::new("exp-new-folder").small().ghost().icon(Lucide::FolderPlus).tooltip(t.fs_toolbar_new_folder).disabled(!fs_enabled)
                .on_click(cx.listener({
                    let parent = toolbar_root.clone();
                    move |this, _, window, cx| this.open_new_folder_dialog(parent.clone(), window, cx)
                })))
            .child(Button::new("exp-import-file").small().ghost().icon(Lucide::Upload).tooltip(t.fs_toolbar_import_file).disabled(!fs_enabled)
                .on_click(cx.listener({
                    let parent = toolbar_root.clone();
                    move |this, _, window, cx| this.start_import(parent.clone(), false, window, cx)
                })))
            .child(Button::new("exp-import-folder").small().ghost().icon(Lucide::FolderUp).tooltip(t.fs_toolbar_import_folder).disabled(!fs_enabled)
                .on_click(cx.listener({
                    let parent = toolbar_root.clone();
                    move |this, _, window, cx| this.start_import(parent.clone(), true, window, cx)
                })))
            .child(Button::new("exp-git-clone").small().ghost().icon(Lucide::GitBranch).tooltip(t.git_clone_toolbar).disabled(!can_clone)
                .on_click(cx.listener({
                    let parent = toolbar_root.clone();
                    move |this, _, window, cx| this.open_git_clone_dialog(parent.clone(), window, cx)
                })));
        if self.fs_clipboard.is_some() {
            toolbar = toolbar.child(
                Button::new("exp-paste").small().ghost().icon(Lucide::ClipboardPaste).tooltip(t.fs_toolbar_paste).disabled(!fs_enabled)
                    .on_click(cx.listener(move |this, _, _, cx| this.paste_into(paste_root.clone(), cx))),
            );
        }
        let search_row = h_flex().gap_2()
            .child(div().flex_1().min_w_0().child(Input::new(&self.explorer_query).prefix(Icon::new(Lucide::Search).small().text_color(muted))))
            .child(Button::new("exp-search").small().disabled(!enabled).icon(Lucide::Search).label(t.editor_search)
                .on_click(cx.listener(|this, _, window, cx| this.refresh_tree(window, cx))));
        let hidden_row = Checkbox::new("exp-hidden").label(t.editor_show_hidden).checked(self.explorer_show_hidden)
            .on_change(cx.listener(|this, value, window, cx| {
                this.explorer_show_hidden = *value;
                this.refresh_tree(window, cx);
            }));

        // `flex_1` (inside the flex-column scroll content, which is at least
        // as tall as the column) stretches the panel down to the bottom, so
        // the blank space under the tree is part of the clickable tree area.
        // A percentage height (`min_h_full`) does not resolve there.
        let mut panel = v_flex().w_full().flex_1().gap_3().p_3().rounded(radius).border_1().border_color(border)
            .child(header).child(root_row)
            .child(toolbar)
            .child(search_row).child(hidden_row);

        if self.git_clone_target.is_some() && self.terminal().update_running {
            panel = panel.child(
                h_flex().gap_1().items_center()
                    .child(Spinner::new().small().color(muted))
                    .child(div().flex_1().min_w_0().text_xs().text_color(muted).child(i18n::maintenance_running(self.lang, &self.terminal().task_title))),
            );
        }
        if let Some(message) = &self.fs_message {
            panel = panel.child(if self.fs_message_error {
                notice_row(Lucide::TriangleAlert, cx.theme().red, radius, message.clone()).into_any_element()
            } else {
                icon_note(Lucide::Info, muted, message.clone()).into_any_element()
            });
        }
        if let Some(error) = &self.explorer_error {
            panel = panel.child(notice_row(Lucide::TriangleAlert, cx.theme().red, radius, error.clone()));
        }

        let rows = self.render_tree_node(&root_path, 0, enabled, muted, cx);
        // Clicking empty space (not a row) drops the selection, so New
        // file/folder, Import and Git clone target the root folder again.
        panel = panel.child(
            v_flex().id("explorer-tree").flex_1().gap_1().pt_2().border_t_1().border_color(border)
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.explorer_selected.take().is_some() { cx.notify(); }
                }))
                .children(rows),
        );
        panel.into_any_element()
    }

    /// Renders one tree branch: the cached listing at `path` (or a few
    /// skeleton bars if its `Request::List` hasn't come back yet) plus, for
    /// each expanded subfolder, its own branch recursively.
    ///
    /// Deliberately thin: this is the only function that recurses (once per
    /// open folder level), so every row's element building lives in
    /// `render_tree_row`/`tree_note_row`, which return before the next level
    /// is visited. With the row code inlined here, each level's frame was
    /// large enough in a debug build that three open levels overflowed the
    /// 1 MB Windows main-thread stack.
    fn render_tree_node(&mut self, path: &str, depth: usize, enabled: bool, muted: Hsla, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(directory) = self.explorer_nodes.get(path).cloned() else {
            return tree_loading_rows(depth);
        };
        let t = self.t();
        let lang = self.lang;
        let mut rows: Vec<AnyElement> = Vec::new();
        if !directory.query.is_empty() {
            rows.push(tree_note_row(depth, Lucide::Search, muted, i18n::search_query_note(lang, &directory.query)));
        }
        if directory.entries.is_empty() {
            rows.push(tree_note_row(depth, Lucide::FolderOpen, muted, t.editor_empty_folder));
        }
        for entry in &directory.entries {
            let target_path = child_path(&directory.path, &entry.name);
            let expanded = entry.directory() && self.explorer_expanded.contains(&target_path);
            rows.push(self.render_tree_row(entry, &target_path, depth, expanded, enabled, muted, cx));
            if expanded {
                rows.extend(self.render_tree_node(&target_path, depth + 1, enabled, muted, cx));
            }
        }
        if directory.truncated {
            rows.push(tree_note_row(depth, Lucide::TriangleAlert, cx.theme().yellow, t.editor_truncated_note));
        }
        if directory.skipped > 0 {
            rows.push(tree_note_row(depth, Lucide::Info, muted, i18n::non_utf8_skipped(lang, directory.skipped)));
        }
        rows
    }

    /// One Explorer row (icon, name, size/kind, click + right-click menu).
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn render_tree_row(&mut self, entry: &crate::remote_directory::Entry, target_path: &str, depth: usize, expanded: bool, enabled: bool, muted: Hsla, cx: &mut Context<Self>) -> AnyElement {
        let t = self.t();
        let lang = self.lang;
        let fs_enabled = !self.terminal().fs_busy;
        let can_clone = self.terminal().can_maintain();
        let indent = px(depth as f32 * 20.0 + 4.0);
        let target_path = target_path.to_string();
        {
            let is_dir = entry.directory();
            let openable = entry.openable();
            let size = size_label(entry, t);
            let kind = entry.kind.clone();
            let detail = i18n::entry_detail(lang, &entry.permissions, &size, &entry.modified, if entry.target.is_empty() { "-" } else { &entry.target });
            let row_id = SharedString::from(format!("tree-{target_path}"));
            let is_open_file = !is_dir && self.editor_path == target_path;
            let is_open_dirty = is_open_file && self.editor_text.read(cx).value().to_string() != self.editor_original;
            let ext = entry.name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
            let (type_icon, type_color) = if is_dir {
                (if expanded { Lucide::FolderOpen } else { Lucide::Folder }, cx.theme().blue)
            } else if entry.kind == "link" {
                (Lucide::FileSymlink, cx.theme().magenta)
            } else {
                match ext.as_deref() {
                    Some("txt" | "md" | "markdown" | "log" | "csv" | "tsv" | "rst") => (Lucide::FileText, muted),
                    Some(_) => (Lucide::FileCode, cx.theme().cyan),
                    None => (Lucide::File, muted),
                }
            };
            let chevron = is_dir.then_some(if expanded { Lucide::ChevronDown } else { Lucide::ChevronRight });
            let is_selected = self.explorer_selected.as_ref().is_some_and(|(selected, _)| *selected == target_path);
            let mut row = h_flex().id(row_id).gap_2().items_center().pl(indent).pr_1().rounded(cx.theme().radius)
                .when(is_open_file, |el| el.bg(cx.theme().blue.opacity(0.12)))
                .when(is_selected, |el| el.bg(cx.theme().list_active))
                .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(detail.clone()).build(window, cx));
            row = row.when(openable && enabled, |el| {
                let target_path = target_path.clone();
                el.cursor_pointer().hover(|s| s.bg(cx.theme().sidebar_accent)).on_click(cx.listener(move |this, _, window, cx| {
                    // Keeps the tree area's "click empty space = root" handler
                    // from clearing the selection this click just made.
                    cx.stop_propagation();
                    this.explorer_selected = Some((target_path.clone(), is_dir));
                    if is_dir { this.toggle_tree_node(target_path.clone(), window, cx); } else { this.select_remote_file(target_path.clone(), window, cx); }
                    cx.notify();
                }))
            });
            let menu_path = target_path.clone();
            let menu_entity = cx.entity();
            let clipboard_set = self.fs_clipboard.is_some();
            let is_compose = !is_dir && entry.kind == "file" && is_compose_file(&entry.name);
            let is_git = is_dir && entry.git;
            let is_link = entry.kind == "link" || entry.kind == "dirlink";
            let current_mode = mode_from_filemode(&entry.permissions);
            // A plain `let` closure (not inlined into `.context_menu(...)`)
            // because `ContextMenuExt::context_menu` returns a wrapper type
            // (`ContextMenu<_>`, not `Self`), which `row = row.when(...)`
            // reassignment cannot accept — this builder is applied once,
            // last, right when `row` turns into `AnyElement` below.
            let menu_builder = move |menu: PopupMenu, window: &mut Window, cx: &mut Context<PopupMenu>| -> PopupMenu {
                let mut menu = menu;
                if is_dir {
                    menu = menu
                        .item(PopupMenuItem::new(t.fs_menu_open_terminal).icon(Lucide::SquareTerminal).on_click({
                            let entity = menu_entity.clone();
                            let path = menu_path.clone();
                            move |_, window, cx| entity.update(cx, |this, cx| this.open_folder_in_terminal(path.clone(), window, cx))
                        }));
                    // Only on repository roots (the listing reports `.git`).
                    if is_git {
                        let git_entity = menu_entity.clone();
                        let git_path = menu_path.clone();
                        menu = menu.submenu_with_icon(Some(Icon::new(Lucide::GitBranch)), t.git_menu, window, cx, move |submenu, _, _| {
                            submenu
                                .item(PopupMenuItem::new(t.git_status).icon(Lucide::Info).on_click({
                                    let entity = git_entity.clone();
                                    let path = git_path.clone();
                                    move |_, window, cx| entity.update(cx, |this, cx| this.run_git_in(path.clone(), "status", window, cx))
                                }))
                                .item(PopupMenuItem::new(t.git_pull).icon(Lucide::ArrowDown).on_click({
                                    let entity = git_entity.clone();
                                    let path = git_path.clone();
                                    move |_, window, cx| entity.update(cx, |this, cx| this.run_git_in(path.clone(), "pull", window, cx))
                                }))
                                .item(PopupMenuItem::new(t.git_push).icon(Lucide::ArrowUp).on_click({
                                    let entity = git_entity.clone();
                                    let path = git_path.clone();
                                    move |_, window, cx| entity.update(cx, |this, cx| this.confirm_git_push(path.clone(), window, cx))
                                }))
                        });
                    }
                    menu = menu
                        .item(PopupMenuItem::separator())
                        .item(PopupMenuItem::new(t.fs_menu_new_file).icon(Lucide::FilePlus).disabled(!fs_enabled).on_click({
                            let entity = menu_entity.clone();
                            let parent = menu_path.clone();
                            move |_, window, cx| entity.update(cx, |this, cx| this.open_new_file_dialog(parent.clone(), window, cx))
                        }))
                        .item(PopupMenuItem::new(t.fs_menu_new_folder).icon(Lucide::FolderPlus).disabled(!fs_enabled).on_click({
                            let entity = menu_entity.clone();
                            let parent = menu_path.clone();
                            move |_, window, cx| entity.update(cx, |this, cx| this.open_new_folder_dialog(parent.clone(), window, cx))
                        }))
                        .item(PopupMenuItem::new(t.git_clone_menu).icon(Lucide::GitBranch).disabled(!can_clone).on_click({
                            let entity = menu_entity.clone();
                            let parent = menu_path.clone();
                            move |_, window, cx| entity.update(cx, |this, cx| this.open_git_clone_dialog(parent.clone(), window, cx))
                        }));
                    if clipboard_set {
                        menu = menu.item(PopupMenuItem::new(t.fs_menu_paste).icon(Lucide::ClipboardPaste).disabled(!fs_enabled).on_click({
                            let entity = menu_entity.clone();
                            let parent = menu_path.clone();
                            move |_, _, cx| entity.update(cx, |this, cx| this.paste_into(parent.clone(), cx))
                        }));
                    }
                    menu = menu.item(PopupMenuItem::separator());
                } else if is_compose {
                    menu = menu
                        .item(PopupMenuItem::new(t.compose_up_menu).icon(Lucide::Container).on_click({
                            let entity = menu_entity.clone();
                            let path = menu_path.clone();
                            move |_, _, cx| entity.update(cx, |this, cx| this.start_compose_up(path.clone(), cx))
                        }))
                        .item(PopupMenuItem::separator());
                }
                menu.item(PopupMenuItem::new(t.fs_menu_copy).icon(Lucide::Copy).disabled(!fs_enabled).on_click({
                        let entity = menu_entity.clone();
                        let path = menu_path.clone();
                        move |_, _, cx| entity.update(cx, |this, cx| this.copy_entry(path.clone(), is_dir, cx))
                    }))
                    .item(PopupMenuItem::new(t.fs_menu_rename).icon(Lucide::PencilLine).disabled(!fs_enabled).on_click({
                        let entity = menu_entity.clone();
                        let path = menu_path.clone();
                        move |_, window, cx| entity.update(cx, |this, cx| this.open_rename_dialog(path.clone(), window, cx))
                    }))
                    .item(PopupMenuItem::new(t.fs_menu_move).icon(Lucide::Move).disabled(!fs_enabled).on_click({
                        let entity = menu_entity.clone();
                        let path = menu_path.clone();
                        move |_, window, cx| entity.update(cx, |this, cx| this.open_move_dialog(path.clone(), window, cx))
                    }))
                    // A symlink's own bits are always rwxrwxrwx and chmod
                    // would change its target instead, so links are excluded.
                    .item(PopupMenuItem::new(t.fs_menu_chmod).icon(Lucide::ShieldCheck).disabled(!fs_enabled || is_link).on_click({
                        let entity = menu_entity.clone();
                        let path = menu_path.clone();
                        move |_, window, cx| entity.update(cx, |this, cx| this.open_chmod_dialog(path.clone(), is_dir, current_mode, window, cx))
                    }))
                    .item(PopupMenuItem::separator())
                    .item(PopupMenuItem::new(t.fs_menu_delete).icon(Lucide::Trash).disabled(!fs_enabled).on_click({
                        let entity = menu_entity.clone();
                        let path = menu_path.clone();
                        move |_, window, cx| entity.update(cx, |this, cx| this.confirm_delete_entry(path.clone(), is_dir, window, cx))
                    }))
            };
            row = row
                .child(div().w(px(12.)).flex_shrink_0().children(chevron.map(|c| Icon::new(c).small().text_color(muted))))
                .child(Icon::new(type_icon).small().text_color(type_color).flex_shrink_0())
                .child(div().flex_1().min_w_0().truncate().when(is_open_file, |el| el.font_bold()).child(entry.name.clone()))
                .when(is_open_dirty, |el| el.child(Icon::new(Lucide::CircleDot).small().text_color(cx.theme().yellow)))
                .child(div().text_xs().text_color(muted).child(format!("{size} · {kind}")));
            row.context_menu(menu_builder).into_any_element()
        }
    }

    /// Kept compact on purpose (one toolbar row + one thin status row, plus
    /// warning lines only when something actually needs attention) so the
    /// code editor itself keeps most of the panel's vertical space.
    fn render_editor_document(&mut self, online: bool, host: Option<&str>, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.t();
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let radius = cx.theme().radius;
        let busy = self.terminal().file_busy;

        let open_icon = if self.editor_path.is_empty() { Lucide::FolderOpen } else { Lucide::RefreshCw };
        let mut toolbar = v_flex().gap_2().p_3().rounded(radius).border_1().border_color(border)
            .child(
                h_flex().gap_2()
                    .child(div().flex_1().min_w_0().child(Input::new(&self.editor_path_input).prefix(Icon::new(Lucide::File).small().text_color(muted))))
                    .child(Button::new("editor-open").small().disabled(!(online && !busy)).icon(open_icon).label(if self.editor_path.is_empty() { t.editor_open_button } else { t.editor_reload_button })
                        .on_click(cx.listener(|this, _, window, cx| {
                            let path = this.editor_path_input.read(cx).value().to_string();
                            this.select_remote_file(path, window, cx);
                        }))),
            );
        if busy && !self.explorer_browsing {
            toolbar = toolbar.child(
                h_flex().gap_1().items_center()
                    .child(Spinner::new().small().color(muted))
                    .child(div().text_xs().text_color(muted).child(if self.editor_saving { t.editor_saving_note } else { t.editor_reading_note })),
            );
        }
        // Errors stay on screen (actionable); a success confirmation isn't —
        // the loaded/saved content is already visible as proof.
        if let Some(message) = &self.editor_message {
            if self.editor_failed {
                toolbar = toolbar.child(notice_row(Lucide::TriangleAlert, cx.theme().red, radius, message.clone()));
            }
        }

        let mut doc = v_flex().size_full().gap_3().child(toolbar);

        if self.editor_path.is_empty() {
            return doc
                .child(
                    div().flex_1().min_h_0().p_4().rounded(radius).border_1().border_color(border)
                        .child(
                            Empty::new().header(
                                EmptyHeader::new()
                                    .media(
                                        EmptyMedia::new().with_variant(EmptyMediaVariant::Icon)
                                            .child(Icon::new(Lucide::FileCode)),
                                    )
                                    .title(EmptyTitle::new().child(t.editor_no_file_heading))
                                    .description(EmptyDescription::new().child(t.editor_no_file_desc)),
                            ),
                        ),
                )
                .into_any_element();
        }
        let same_host = host == Some(self.editor_host.as_str());
        let current_text = self.editor_text.read(cx).value().to_string();
        let dirty = current_text != self.editor_original;
        let text_len = current_text.len();
        let can_save = online && same_host && !busy && dirty && self.editor_revision.is_some() && text_len <= crate::remote_file::MAX_BYTES;
        let status_chip = if dirty {
            Tag::warning().small().child(t.editor_unsaved_badge)
        } else {
            Tag::success().small().child(t.editor_saved_badge)
        };
        // Two lines rather than one: the path gets its own line (truncated
        // with "…"), and the fixed-size pieces — tag, byte count, Copy/Save —
        // share the second, which wraps the buttons onto a further line when
        // even that is too narrow. On one line, a narrow pane squeezed the
        // path to nothing and the fixed pieces were drawn on top of each other.
        let path_line = div().w_full().min_w_0().truncate().text_xs().font_family("monospace").text_color(muted)
            .child(format!("{} · {}", self.editor_host, self.editor_path));
        let status = h_flex().flex_shrink_0().gap_2().items_center().text_xs()
            .child(status_chip)
            .child(div().text_color(muted).child(i18n::byte_count(self.lang, text_len)));
        let markdown = is_markdown_file(&self.editor_path);
        let preview = markdown && self.editor_preview;
        let actions = Toolbar::new("editor-actions").small()
            .when(markdown, |toolbar| toolbar.child(
                Button::new("editor-preview").icon(if preview { Lucide::Pencil } else { Lucide::Eye })
                    .label(if preview { t.editor_edit } else { t.editor_preview }).selected(preview)
                    .on_click(cx.listener(|this, _, _, cx| { this.editor_preview = !this.editor_preview; cx.notify(); })),
            ))
            .child(Button::new("editor-copy").icon(Lucide::Copy).label(t.editor_copy)
                .on_click(cx.listener(|this, _, _, cx| {
                    let text = this.editor_text.read(cx).value().to_string();
                    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text));
                })))
            .child(Button::new("editor-save").primary().icon(Lucide::Save).label(t.editor_save).disabled(!can_save)
                .on_click(cx.listener(|this, _, _, cx| this.save_draft(cx))));
        let mut info = v_flex().gap_2().p_3().rounded(radius).border_1().border_color(border)
            .child(path_line)
            .child(
                h_flex().w_full().flex_wrap().items_center().gap_2()
                    .child(status)
                    .child(div().flex_1())
                    .child(actions),
            );
        let mut warnings = h_flex().flex_wrap().gap_2();
        let mut has_warnings = false;
        if !same_host {
            has_warnings = true;
            warnings = warnings.child(notice_row(Lucide::TriangleAlert, cx.theme().yellow, radius, t.editor_wrong_host_warning));
        }
        if self.editor_revision.is_none() {
            has_warnings = true;
            warnings = warnings.child(notice_row(Lucide::TriangleAlert, cx.theme().yellow, radius, t.editor_no_revision_warning));
        }
        if text_len > crate::remote_file::MAX_BYTES {
            has_warnings = true;
            warnings = warnings.child(notice_row(Lucide::TriangleAlert, cx.theme().yellow, radius, t.editor_oversized_warning));
        }
        if has_warnings {
            info = info.child(warnings);
        }
        doc = doc.child(info);
        doc = doc.child(if preview {
            // Rendered from the current draft, so unsaved edits show too.
            // Keyed by path so each file keeps its own parse/scroll state.
            div().flex_1().min_h_0().rounded(radius).border_1().border_color(border).overflow_hidden().bg(cx.theme().background)
                .child(
                    gpui_kit::component::text::TextView::markdown(SharedString::from(format!("md-preview-{}", self.editor_path)), current_text)
                        .selectable(true)
                        .scrollable(true)
                        .size_full()
                        .px_6()
                        .py_4(),
                )
                .into_any_element()
        } else {
            div().flex_1().min_h_0().rounded(radius).border_1().border_color(border).overflow_hidden()
                .child(gpui_kit::component::input::Editor::new(&self.editor_text).h_full())
                .into_any_element()
        });
        let _ = window;
        doc.into_any_element()
    }

    /// `Action::Server`/`Action::Firewall`/`Action::Power` run and report
    /// pass/fail through `self.terminal.update_result`; their live output
    /// streams into the terminal widget as it runs (`ssh::Event::UpdateOutput`,
    /// see `terminal.rs::handle_ssh_event`), same as an interactive shell.
    fn confirm_maintenance(
        &mut self,
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
        ok_text: impl Into<SharedString>,
        danger: bool,
        action: crate::packages::Maintenance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity();
        let title = title.into();
        let description = description.into();
        let ok_text = ok_text.into();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let entity = entity.clone();
            let action = action.clone();
            let mut alert = alert.title(title.clone()).description(description.clone()).confirm().ok_text(ok_text.clone());
            if danger {
                alert = alert.ok_variant(gpui_kit::component::button::ButtonVariant::Danger);
            }
            alert.on_ok(move |_, _, cx| {
                entity.update(cx, |this, cx| {
                    this.terminal_mut().start_maintenance(action.clone());
                    cx.notify();
                });
                true
            })
        });
    }

    fn confirm_system_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.active_host().cloned() else { return; };
        let t = self.t();
        self.confirm_maintenance(
            t.confirm_update_title,
            i18n::update_confirm_desc(self.lang, &host.name, &host.user, &host.address, host.port),
            t.confirm_update_ok,
            false,
            crate::packages::Maintenance::SystemUpdate,
            window, cx,
        );
    }

    /// Reboot/shutdown gets its own dialog instead of going through
    /// `confirm_maintenance`: the SSH connection is about to die along with
    /// the server, so waiting on the usual `update_running`/`UpdateEnd`
    /// flow for a pass/fail would mean sitting there until the connection
    /// eventually times out (what was happening before — the UI looked like
    /// nothing was happening, then got stuck). Instead this fires a
    /// notification right away and proactively disconnects the session
    /// itself shortly after, so the host visibly goes back to "not
    /// connected" instead of hanging.
    fn confirm_power_action(&mut self, reboot: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.active_host().cloned() else { return; };
        let t = self.t();
        let lang = self.lang;
        let entity = cx.entity();
        let address = host.address.clone();
        let title = if reboot { t.confirm_reboot_title } else { t.confirm_shutdown_title };
        let description = i18n::power_confirm_desc(lang, &host.name, &host.user, &host.address, host.port);
        let ok_text = if reboot { t.confirm_reboot_ok } else { t.confirm_shutdown_ok };
        window.open_alert_dialog(cx, move |alert, _, _| {
            let entity = entity.clone();
            let address = address.clone();
            alert
                .title(title)
                .description(description.clone())
                .confirm()
                .ok_text(ok_text)
                .ok_variant(gpui_kit::component::button::ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    window.push_notification(i18n::power_action_notice(lang, reboot), cx);
                    entity.update(cx, |app, cx| {
                        app.terminal_mut().start_maintenance(crate::packages::Maintenance::Server(crate::live::Action::Power { reboot }));
                        let address = address.clone();
                        // A couple of seconds to let the command actually
                        // reach the server before we tear our own side
                        // down — we are not waiting for it to answer back,
                        // since by design it won't.
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
                            let _ = this.update(cx, |app, cx| {
                                app.disconnect_host(&address);
                                cx.notify();
                            });
                        }).detach();
                        cx.notify();
                    });
                    true
                })
        });
    }

    fn confirm_firewall_action(&mut self, port: Option<u16>, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(port) = port else { return; };
        let t = self.t();
        let protocol = if self.firewall_udp { "udp" } else { "tcp" }.to_string();
        let engine = self.firewall_engine.clone();
        let warn_ssh_port = !open && self.active_host().is_some_and(|host| host.port == port);
        let action_label = if open { t.confirm_firewall_action_allow } else { t.confirm_firewall_action_close };
        let mut description = i18n::firewall_confirm_desc(self.lang, action_label, port, &protocol, &engine);
        if warn_ssh_port {
            description.push_str(t.confirm_firewall_ssh_warning);
        }
        self.confirm_maintenance(
            if open { t.confirm_firewall_open_title } else if engine == "ufw" { t.confirm_firewall_deny_title_ufw } else { t.confirm_firewall_deny_title_other },
            description,
            if open { t.confirm_firewall_allow_ok } else { t.confirm_firewall_apply_ok },
            !open,
            crate::packages::Maintenance::Server(crate::live::Action::Firewall { engine, port, protocol, open }),
            window, cx,
        );
    }

    fn confirm_firewall_delete_rule(&mut self, number: u32, to: String, action: String, window: &mut Window, cx: &mut Context<Self>) {
        let t = self.t();
        let lang = self.lang;
        let warn_ssh_port = action.to_uppercase().starts_with("ALLOW")
            && to.split('/').next().and_then(|port| port.trim().parse::<u16>().ok())
                .is_some_and(|port| self.active_host().is_some_and(|host| host.port == port));
        let mut description = i18n::firewall_delete_rule_desc(lang, &action, &to);
        if warn_ssh_port {
            description.push_str(t.confirm_firewall_ssh_warning);
        }
        self.confirm_maintenance(
            t.confirm_firewall_delete_title,
            description,
            t.confirm_firewall_delete_ok,
            true,
            crate::packages::Maintenance::Server(crate::live::Action::FirewallDeleteRule { number }),
            window, cx,
        );
    }

    fn confirm_firewall_toggle(&mut self, enable: bool, window: &mut Window, cx: &mut Context<Self>) {
        let lang = self.lang;
        let engine = self.firewall_engine.clone();
        self.confirm_maintenance(
            i18n::firewall_toggle_title(lang, &engine, enable),
            i18n::firewall_toggle_warning(lang, enable),
            i18n::firewall_toggle_ok(lang, enable),
            true,
            crate::packages::Maintenance::Server(crate::live::Action::FirewallToggle { engine, enable }),
            window, cx,
        );
    }

    fn confirm_firewall_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let t = self.t();
        let lang = self.lang;
        let engine = self.firewall_engine.clone();
        self.confirm_maintenance(
            t.confirm_firewall_reload_title,
            i18n::firewall_reload_desc(lang, &engine),
            t.confirm_firewall_reload_ok,
            false,
            crate::packages::Maintenance::Server(crate::live::Action::FirewallReload { engine }),
            window, cx,
        );
    }

    /// Whether there is a persistent terminal panel to show at all — a live
    /// session for the active host, and not already on the full Terminal
    /// page (which shows the same widget by itself).
    fn has_terminal_footer(&self) -> bool {
        self.page != Page::Terminal && self.terminal_widget().is_some()
    }

    /// Tab strip (one per connected host, so switching which one's shell
    /// you're looking at doesn't require leaving the current page) plus
    /// fullscreen/close controls. Shared by the docked and fullscreen
    /// terminal panel renderers.
    fn render_terminal_tabs(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.t();
        let border = cx.theme().border;
        let mut tabs = h_flex().id("terminal-footer-tabs").gap_1().items_center().flex_1().min_w_0().overflow_x_scrollbar();
        for (index, host) in self.hosts.iter().enumerate() {
            let Some(session) = self.sessions.get(&host.address) else { continue; };
            if !(session.terminal.connected || session.terminal.busy()) { continue; }
            let is_active = self.active == Some(index);
            tabs = tabs.child(
                Button::new(("terminal-tab", index)).small().label(host.name.clone())
                    .selected(is_active)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let page = this.page;
                        this.switch_to(index, page, window, cx);
                    })),
            );
        }
        h_flex().items_center().justify_between().gap_2().px_3().py_1().border_t_1().border_b_1().border_color(border).bg(cx.theme().background)
            // The tab bar itself is the resize handle (no separate divider,
            // which would add a padded band above it). Buttons inside keep
            // their own pointer cursor and clicks.
            .when(!self.terminal_footer_fullscreen, |el| {
                el.cursor_row_resize().on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.terminal_footer_drag = Some((event.position.y, this.terminal_footer_height));
                    cx.notify();
                }))
            })
            .child(tabs)
            .child(
                h_flex().gap_1().items_center()
                    .child(Button::new("footer-terminal-expand").small().ghost().icon(IconName::ExternalLink)
                        .tooltip(t.editor_open_terminal)
                        .on_click(cx.listener(|this, _, _, cx| { this.page = Page::Terminal; this.terminal_footer_fullscreen = false; cx.notify(); })))
                    .child(Button::new("footer-fullscreen-toggle").small().ghost()
                        .icon(if self.terminal_footer_fullscreen { IconName::Minimize } else { IconName::Maximize })
                        .tooltip(if self.terminal_footer_fullscreen { t.terminal_footer_restore_tooltip } else { t.terminal_footer_fullscreen_tooltip })
                        .on_click(cx.listener(|this, _, _, cx| { this.terminal_footer_fullscreen = !this.terminal_footer_fullscreen; cx.notify(); })))
                    .child(Button::new("footer-close").small().ghost().icon(IconName::Close)
                        .tooltip(t.terminal_footer_close_tooltip)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.terminal_footer_open = false;
                            this.terminal_footer_fullscreen = false;
                            cx.notify();
                        }))),
            )
            .into_any_element()
    }

    /// Tabs + the active host's live terminal widget, filling whatever
    /// space the caller gives it — used both docked (inside the resizable
    /// split) and fullscreen (filling the whole content area).
    fn render_terminal_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let tabs = self.render_terminal_tabs(cx);
        let mut panel = v_flex().size_full().child(tabs);
        if let Some(widget) = self.terminal_widget().cloned() {
            panel = panel.child(div().flex_1().min_h_0().w_full().child(widget));
        }
        panel.into_any_element()
    }

    /// The slim strip shown instead of the terminal panel once the user has
    /// closed it — click it (or the chevron) to bring it back, rather than
    /// losing track of the session entirely.
    fn render_terminal_reopen_strip(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.t();
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        h_flex().id("terminal-footer-reopen").items_center().justify_between()
            .px_3().py_1().border_t_1().border_color(border).bg(cx.theme().background).cursor_pointer()
            .hover(|s| s.bg(cx.theme().sidebar_accent))
            .on_click(cx.listener(|this, _, _, cx| { this.terminal_footer_open = true; cx.notify(); }))
            .child(div().text_xs().text_color(muted).child(t.terminal_footer_reopen))
            .child(div().text_color(muted).child(IconName::ChevronUp))
            .into_any_element()
    }
}

impl Render for AwanApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(message) = self.host_store_message.take() { window.push_notification(message, cx); }
        if self.host_form_error.is_some() && self.page != Page::Hosts { /* shown via notification already */ }
        // The terminal's right-click menu labels follow the app language.
        if let Some(widget) = self.terminal_widget().cloned() {
            let t = self.t();
            widget.update(cx, |widget, _| widget.set_menu_labels(t.terminal_copy, t.terminal_paste));
        }

        let trust_overlay = self.render_trust_prompt(cx);
        let background = cx.theme().background;
        let border = cx.theme().border;
        let page_title = self.page.title(self.lang);
        let page_subtitle = self.page.subtitle(self.lang);
        let muted = cx.theme().muted_foreground;

        let sidebar = self.render_sidebar(cx);
        let toolbar = h_flex()
            .items_center().p_3()
            .border_b_1().border_color(border)
            .child(
                v_flex().gap_0()
                    .child(div().font_bold().child(page_title))
                    .child(div().text_xs().text_color(muted).child(page_subtitle)),
            );
        let has_terminal = self.has_terminal_footer();
        let content = if has_terminal && self.terminal_footer_fullscreen {
            // Fullscreen replaces the page body outright rather than giving
            // the terminal a 100%-size split panel: a split still reserves
            // a sliver for the other side and keeps its drag handle, neither
            // of which belongs once the point is to see nothing else.
            self.render_terminal_panel(cx)
        } else if has_terminal && self.terminal_footer_open {
            let body = self.render_body(window, cx);
            let panel = self.render_terminal_panel(cx);
            // Not `v_resizable`: its drag handle reserves a fixed ~9px padded
            // band (not configurable) that read as a stray gap above the
            // terminal's tab bar. The tab bar itself is the resize handle
            // instead (see `render_terminal_tabs` / `terminal_drag_tracker`).
            v_flex().size_full()
                .child(div().flex_1().min_h_0().w_full().child(body))
                .child(div().h(self.terminal_footer_height).w_full().child(panel))
                .into_any_element()
        } else if has_terminal {
            let body = self.render_body(window, cx);
            let reopen = self.render_terminal_reopen_strip(cx);
            v_flex().size_full()
                .child(div().flex_1().min_h_0().w_full().child(body))
                .child(reopen)
                .into_any_element()
        } else {
            let body = self.render_body(window, cx);
            div().size_full().child(body).into_any_element()
        };

        let main = v_flex().flex_1().min_w_0().size_full()
            .child(toolbar)
            .child(div().flex_1().min_h_0().w_full().child(content));
        let shell = h_flex().size_full().bg(background).child(sidebar).child(main);
        let drag_tracker = self.terminal_drag_tracker(cx);

        div()
            .size_full()
            .relative()
            .child(shell)
            .when_some(trust_overlay, |this, overlay| this.child(overlay))
            .children(drag_tracker)
    }
}

impl AwanApp {
    /// While the terminal tab bar is being dragged, a zero-size canvas that
    /// registers window-level mouse listeners — the same technique the
    /// component library's own resize handle uses — so the drag keeps
    /// tracking when the cursor moves over the terminal or editor below
    /// (an element-level listener would stop at whatever is under it).
    fn terminal_drag_tracker(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (start_y, start_height) = self.terminal_footer_drag?;
        let entity = cx.entity();
        let tracker = canvas(|_, _, _| {}, move |_, _, window, _| {
            window.on_mouse_event({
                let entity = entity.clone();
                move |event: &MouseMoveEvent, phase, window, cx| {
                    if !phase.bubble() { return; }
                    // Released outside the window: no mouse-up arrives, but
                    // the next move reports no pressed button.
                    if event.pressed_button != Some(MouseButton::Left) {
                        entity.update(cx, |this, cx| { this.terminal_footer_drag = None; cx.notify(); });
                        return;
                    }
                    let max = (window.viewport_size().height - px(160.)).max(px(120.));
                    let height = (start_height + (start_y - event.position.y)).max(px(120.)).min(max);
                    entity.update(cx, |this, cx| {
                        if this.terminal_footer_height != height {
                            this.terminal_footer_height = height;
                            cx.notify();
                        }
                    });
                }
            });
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase.bubble() {
                    entity.update(cx, |this, cx| { this.terminal_footer_drag = None; cx.notify(); });
                }
            });
        });
        Some(tracker.absolute().size_0().into_any_element())
    }
}

/// The component library's own `.danger()` variant renders as a soft,
/// low-contrast tint in this theme — too weak to read as clearly destructive
/// for actions like Disconnect/Delete/Reboot/Uninstall. `ButtonVariant::Custom`
/// looks even softer: the library always mixes 20% transparency into a
/// custom background (and fades it further still when disabled), so it was
/// never actually going to come out solid. Overriding `.bg()`/`.text_color()`
/// directly on top of `.danger()` does work — those calls refine the
/// button's style *after* the variant's own colors are computed, so they win.
fn solid_danger(button: Button) -> Button {
    button.danger().bg(rgb(0xDC2626)).text_color(white())
}

/// Parses `ufw status numbered` output into (rule number, to, action, from)
/// rows, so the Firewall page can show a scannable list of configured ports
/// (and delete a specific one by number) instead of a raw text blob. Columns
/// after the "[ N]" prefix are separated by runs of 2+ spaces (see
/// `split_columns`); everything before the "--  ------  ----" header
/// separator (title/logging/default-policy lines) is not a rule and is
/// skipped.
fn parse_ufw_rules(text: &str) -> Vec<(u32, String, String, String)> {
    let mut rules = Vec::new();
    let mut in_table = false;
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim_start().starts_with("--") {
            in_table = true;
            continue;
        }
        if !in_table || line.trim().is_empty() {
            continue;
        }
        let Some(rest) = line.trim_start().strip_prefix('[') else { continue; };
        let Some(bracket_end) = rest.find(']') else { continue; };
        let Ok(number) = rest[..bracket_end].trim().parse::<u32>() else { continue; };
        let columns = split_columns(&rest[bracket_end + 1..]);
        if columns.len() >= 2 {
            rules.push((number, columns[0].clone(), columns[1].clone(), columns.get(2).cloned().unwrap_or_default()));
        }
    }
    rules
}

/// Splits a line on runs of 2+ spaces, keeping single spaces inside a
/// column's text intact (e.g. the "ALLOW IN" action column).
fn split_columns(line: &str) -> Vec<String> {
    let mut columns = Vec::new();
    let mut current = String::new();
    let mut gap = 0u32;
    for ch in line.chars() {
        if ch == ' ' {
            gap += 1;
            if gap < 2 { current.push(ch); }
            else if gap == 2 && !current.trim().is_empty() { columns.push(std::mem::take(&mut current)); }
        } else {
            gap = 0;
            current.push(ch);
        }
    }
    if !current.trim().is_empty() { columns.push(current); }
    columns.into_iter().map(|c| c.trim().to_string()).collect()
}

fn capacity(used: &str, total: &str, t: &'static i18n::Strings) -> String {
    match (used.parse::<f64>(), total.parse::<f64>()) {
        (Ok(used), Ok(total)) if total > 0.0 => format!("{:.1} / {:.1} GiB", used / 1_073_741_824.0, total / 1_073_741_824.0),
        _ => t.not_available.into(),
    }
}

fn labeled_row(label: &str, value: &str, muted: Hsla) -> impl IntoElement {
    h_flex().gap_3()
        .child(div().w(px(90.)).text_xs().text_color(muted).child(label.to_string()))
        .child(div().text_sm().child(value.to_string()))
}

/// A bordered, icon-led notice line so warnings/errors stand out from the
/// surrounding text instead of blending in as just another colored line.
fn notice_row(icon: impl Into<Icon>, color: Hsla, radius: Pixels, text: impl Into<SharedString>) -> impl IntoElement {
    // `min_w_0` lets the text shrink below its one-line width so it wraps
    // inside a narrow panel instead of running past the card's edge.
    h_flex().w_full().items_start().gap_2().p_2().rounded(radius).bg(color.opacity(0.08))
        .border_1().border_color(color.opacity(0.35))
        .child(Icon::new(icon).small().text_color(color).flex_shrink_0())
        .child(div().flex_1().min_w_0().text_xs().text_color(color).child(text.into()))
}

/// A small inline icon + muted/colored note, for lighter-weight tree/status
/// annotations (loading, empty, truncated, search) that don't need
/// `notice_row`'s bordered card treatment.
/// Placeholder rows for a tree branch whose listing hasn't arrived yet. Kept
/// out of the recursive `render_tree_node` (see there) to keep its frame small.
#[inline(never)]
fn tree_loading_rows(depth: usize) -> Vec<AnyElement> {
    let indent = px(depth as f32 * 20.0 + 4.0);
    vec![
        div().pl(indent).child(Skeleton::new().h_4().w(relative(0.7))).into_any_element(),
        div().pl(indent).child(Skeleton::new().secondary().h_4().w(relative(0.5))).into_any_element(),
        div().pl(indent).child(Skeleton::new().h_4().w(relative(0.6))).into_any_element(),
    ]
}

/// An indented icon + text note inside the tree (empty folder, truncated
/// listing, ...). Kept out of the recursive `render_tree_node` for the same reason.
#[inline(never)]
fn tree_note_row(depth: usize, icon: Lucide, color: Hsla, text: impl Into<SharedString>) -> AnyElement {
    div().pl(px(depth as f32 * 20.0 + 4.0)).child(icon_note(icon, color, text)).into_any_element()
}

/// One numbered card of the Git login wizard, with a done/pending marker.
fn git_step(title: &'static str, done: bool, ok: Hsla, muted: Hsla, border: Hsla, radius: Pixels) -> Div {
    v_flex().w_full().gap_2().p_3().rounded(radius).border_1().border_color(border)
        .child(
            h_flex().gap_2().items_center()
                .child(Icon::new(if done { Lucide::CircleCheck } else { Lucide::Circle }).small().text_color(if done { ok } else { muted }))
                .child(div().text_sm().font_bold().child(title)),
        )
}

/// One-line Git login status for the Git clone dialog: the connected
/// account, or a prompt to set it up, plus a button into the wizard.
fn git_account_line(entity: &Entity<AwanApp>, t: &'static i18n::Strings, lang: Language, cx: &mut App) -> AnyElement {
    let (state, busy) = {
        let app = entity.read(cx);
        (app.git_auth.clone(), app.terminal().git_busy)
    };
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let radius = cx.theme().radius;
    let green = cx.theme().green;
    let user = state.status.as_ref().and_then(|status| match &status.auth {
        Some(crate::git_auth::Auth::Connected(user)) => Some(user.clone()),
        _ => None,
    });
    let (icon, color, text, action): (Lucide, Hsla, SharedString, &'static str) = match (&user, busy) {
        (Some(user), _) => (Lucide::CircleCheck, green, i18n::git_connected_as(lang, state.provider.name(), user).into(), t.git_account_manage),
        (None, true) => (Lucide::RefreshCw, muted, t.git_account_checking.into(), t.git_account_button),
        (None, false) => (Lucide::KeyRound, muted, t.git_account_missing.into(), t.git_account_button),
    };
    h_flex().w_full().items_center().gap_2().p_2().rounded(radius).border_1().border_color(border)
        .child(Icon::new(icon).small().text_color(color).flex_shrink_0())
        .child(div().flex_1().min_w_0().text_xs().text_color(color).child(text))
        .child(
            Button::new("git-account-open").small().icon(Lucide::KeyRound).label(action)
                .on_click({
                    let entity = entity.clone();
                    move |_, window, cx| entity.update(cx, |this, cx| this.open_git_account_dialog(window, cx))
                }),
        )
        .into_any_element()
}

/// Body of the Git login wizard. Reads the live `git_auth` state on every
/// call so the open dialog tracks each server reply.
fn git_account_body(entity: &Entity<AwanApp>, t: &'static i18n::Strings, lang: Language, cx: &mut App) -> AnyElement {
    use crate::git_auth::{Auth, Provider, Request};
    let (state, busy) = {
        let app = entity.read(cx);
        (app.git_auth.clone(), app.terminal().git_busy)
    };
    let muted = cx.theme().muted_foreground;
    let muted_bg = cx.theme().muted;
    let border = cx.theme().border;
    let radius = cx.theme().radius;
    let green = cx.theme().green;
    let red = cx.theme().red;
    let provider = state.provider;

    let mut providers = h_flex().gap_1();
    for option in Provider::ALL {
        providers = providers.child(
            Button::new(SharedString::from(format!("git-provider-{}", option.name()))).small()
                .label(option.name()).selected(option == provider).disabled(busy)
                .on_click({
                    let entity = entity.clone();
                    move |_, _, cx| entity.update(cx, |this, cx| this.set_git_provider(option, cx))
                }),
        );
    }
    let recheck = Button::new("git-recheck").small().ghost().icon(Lucide::RefreshCw).tooltip(t.git_account_recheck).disabled(busy)
        .on_click({
            let entity = entity.clone();
            move |_, _, cx| entity.update(cx, |this, cx| this.request_git_auth(Request::Status(provider), cx))
        });

    let mut body = v_flex().w_full().gap_3()
        .child(div().text_xs().text_color(muted).child(t.git_account_intro))
        .child(h_flex().items_center().gap_2().child(providers).child(div().flex_1()).child(recheck));
    if busy {
        body = body.child(
            h_flex().gap_1().items_center()
                .child(Spinner::new().small().color(muted))
                .child(div().text_xs().text_color(muted).child(t.git_account_checking)),
        );
    }
    if let Some(error) = &state.error {
        body = body.child(notice_row(Lucide::TriangleAlert, red, radius, error.clone()));
    }
    let Some(status) = state.status else { return body.into_any_element(); };

    body = body.child(match &status.auth {
        Some(Auth::Connected(user)) => notice_row(Lucide::CircleCheck, green, radius, i18n::git_connected_as(lang, provider.name(), user)).into_any_element(),
        _ => icon_note(Lucide::Info, muted, t.git_account_not_connected).into_any_element(),
    });

    // 1. Provider host key, confirmed by the user before it is trusted.
    let mut host_step = git_step(t.git_step_host_title, status.known_host, green, muted, border, radius);
    if status.known_host {
        host_step = host_step.child(div().text_xs().text_color(muted).child(t.git_step_host_done));
    } else if let Some(fingerprint) = state.fingerprint {
        host_step = host_step
            .child(div().text_xs().text_color(muted).child(i18n::git_compare_fingerprint(lang, provider.name())))
            .child(div().p_2().rounded(radius).bg(muted_bg).text_xs().font_family("monospace").child(fingerprint.clone()))
            .child(
                Button::new("git-trust").small().primary().icon(Lucide::ShieldCheck).label(t.git_step_host_trust).disabled(busy)
                    .on_click({
                        let entity = entity.clone();
                        move |_, _, cx| entity.update(cx, |this, cx| this.request_git_auth(Request::Trust(provider, fingerprint.clone()), cx))
                    }),
            );
    } else {
        host_step = host_step
            .child(div().text_xs().text_color(muted).child(t.git_step_host_pending))
            .child(
                Button::new("git-scan").small().icon(Lucide::Search).label(t.git_step_host_scan).disabled(busy)
                    .on_click({
                        let entity = entity.clone();
                        move |_, _, cx| entity.update(cx, |this, cx| this.request_git_auth(Request::Scan(provider), cx))
                    }),
            );
    }

    // 2. The server's SSH key, which the user adds to their provider account.
    let mut key_step = git_step(t.git_step_key_title, status.key.is_some(), green, muted, border, radius);
    match status.key.clone() {
        Some(key) => {
            let public = key.public.clone();
            key_step = key_step
                .child(div().text_xs().text_color(muted).font_family("monospace").child(key.path))
                .child(div().w_full().min_w_0().p_2().rounded(radius).bg(muted_bg).text_xs().font_family("monospace").child(key.public))
                .child(
                    Button::new("git-copy-key").small().icon(Lucide::Copy).label(t.git_step_key_copy)
                        .on_click(move |_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(public.clone()))),
                )
                .child(div().text_xs().text_color(muted).child(i18n::git_add_key_hint(lang, provider)));
        }
        None => {
            key_step = key_step
                .child(div().text_xs().text_color(muted).child(t.git_step_key_none))
                .child(
                    Button::new("git-keygen").small().primary().icon(Lucide::KeyRound).label(t.git_step_key_generate).disabled(busy)
                        .on_click({
                            let entity = entity.clone();
                            move |_, _, cx| entity.update(cx, |this, cx| this.request_git_auth(Request::Keygen(provider), cx))
                        }),
                );
        }
    }

    // 3. Test: re-checks status, which runs `ssh -T` once the host is trusted.
    let connected = matches!(status.auth, Some(Auth::Connected(_)));
    let mut test_step = git_step(t.git_step_test_title, connected, green, muted, border, radius);
    match &status.auth {
        Some(Auth::Denied) => test_step = test_step.child(notice_row(Lucide::TriangleAlert, red, radius, i18n::git_denied(lang, provider.name()))),
        Some(Auth::Failed(message)) if !message.is_empty() => test_step = test_step.child(notice_row(Lucide::TriangleAlert, red, radius, message.clone())),
        _ => {}
    }
    let can_test = status.known_host && status.key.is_some();
    test_step = test_step.child(
        Button::new("git-test").small().icon(Lucide::LogIn).label(t.git_step_test_button).disabled(busy || !can_test)
            .on_click({
                let entity = entity.clone();
                move |_, _, cx| entity.update(cx, |this, cx| this.request_git_auth(Request::Status(provider), cx))
            }),
    );

    body.child(host_step).child(key_step).child(test_step).into_any_element()
}

fn icon_note(icon: impl Into<Icon>, color: Hsla, text: impl Into<SharedString>) -> impl IntoElement {
    h_flex().gap_1().items_start()
        .child(Icon::new(icon).small().text_color(color).flex_shrink_0())
        .child(div().flex_1().min_w_0().text_xs().text_color(color).child(text.into()))
}

/// Maps a remote file's extension (or well-known filename) to a tree-sitter
/// language name for the editor's syntax highlighter. Most extensions are
/// passed straight through: gpui-component's own `language_name()` already
/// resolves common aliases (`rs`→rust, `py`→python, `md`→markdown, `yml`→yaml,
/// `sh`→bash, ...); only the handful it doesn't cover are mapped here. An
/// unrecognized name simply renders without syntax colors.
fn language_for_path(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.eq_ignore_ascii_case("Makefile") { return "make".into(); }
    if name.eq_ignore_ascii_case("CMakeLists.txt") { return "cmake".into(); }
    let Some((_, ext)) = name.rsplit_once('.') else { return "plaintext".into(); };
    match ext.to_ascii_lowercase().as_str() {
        "jsx" | "mjs" | "cjs" => "javascript".into(),
        "mts" | "cts" => "typescript".into(),
        "h" | "hh" | "hpp" | "cc" | "cxx" => "cpp".into(),
        "htm" => "html".into(),
        "zsh" => "bash".into(),
        "patch" => "diff".into(),
        "exs" => "elixir".into(),
        "gql" => "graphql".into(),
        "mk" => "make".into(),
        other => other.into(),
    }
}

fn child_path(folder: &str, name: &str) -> String {
    format!("{}/{}", folder.trim_end_matches('/'), name)
}

/// One page of table rows for the Database Browser's `DataTable`. Cells are
/// stored ready to display (one line); `cell_text` serves copy.
#[derive(Default)]
struct DbGrid {
    columns: Vec<(SharedString, Pixels)>,
    rows: Vec<Vec<Option<SharedString>>>,
}

impl DbGrid {
    fn from_rows(page: &crate::docker_db::Rows) -> Self { Self::from_parts(&page.columns, &page.rows) }

    fn from_parts(names: &[String], source: &[Vec<Option<String>>]) -> Self {
        let display = |value: &Option<String>| value.as_ref().map(|text| SharedString::from(text.replace(['\n', '\r'], " ")));
        let rows: Vec<Vec<Option<SharedString>>> = source.iter().map(|row| row.iter().map(display).collect()).collect();
        // Width from the header and the first rows' text, within sane bounds.
        let columns = names.iter().enumerate().map(|(index, name)| {
            let widest = rows.iter().take(30)
                .filter_map(|row| row.get(index).cloned().flatten())
                .map(|cell| cell.chars().count())
                .chain(std::iter::once(name.chars().count() + 2))
                .max().unwrap_or(4);
            (SharedString::from(name.clone()), px((widest as f32 * 7.5 + 28.).clamp(80., 360.)))
        }).collect();
        Self { columns, rows }
    }
}

impl TableDelegate for DbGrid {
    fn columns_count(&self, _: &App) -> usize { self.columns.len() }

    fn rows_count(&self, _: &App) -> usize { self.rows.len() }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let (name, width) = self.columns[col_ix].clone();
        Column::new(SharedString::from(format!("col-{col_ix}")), name).width(width)
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        match self.rows.get(row_ix).and_then(|row| row.get(col_ix)).cloned().flatten() {
            Some(text) => div().w_full().truncate().font_family("monospace").text_xs().child(text).into_any_element(),
            None => div().italic().font_family("monospace").text_xs().text_color(cx.theme().muted_foreground).child("NULL").into_any_element(),
        }
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _: &App) -> String {
        self.rows.get(row_ix).and_then(|row| row.get(col_ix)).cloned().flatten().map(|text| text.to_string()).unwrap_or_else(|| "NULL".into())
    }
}

/// A data grid that also scrolls sideways with Shift + mouse wheel. The
/// table scrolls horizontally on its own only with a trackpad or its
/// scrollbar (shown while scrolling), and a plain wheel only goes up/down.
/// The listener runs in the capture phase, before the table's vertical
/// scroll can take the event.
fn grid_with_shift_scroll(grid: &Entity<TableState<DbGrid>>) -> AnyElement {
    let grid_for_wheel = grid.clone();
    let tracker = canvas(|_, _, _| {}, move |bounds, _, window, _| {
        let grid = grid_for_wheel.clone();
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
            if !phase.capture() || !event.modifiers.shift || !bounds.contains(&event.position) { return; }
            let delta = event.delta.pixel_delta(window.line_height());
            let step = if delta.x != px(0.) { delta.x } else { delta.y };
            grid.update(cx, |state, cx| {
                let handle = &state.horizontal_scroll_handle;
                let max = handle.max_offset().x.max(px(0.));
                let mut offset = handle.offset();
                offset.x = (offset.x + step).max(-max).min(px(0.));
                handle.set_offset(offset);
                cx.notify();
            });
            cx.stop_propagation();
        });
    });
    div().relative().size_full().min_w_0()
        .child(DataTable::new(grid).stripe(true).bordered(false))
        .child(tracker.absolute().top_0().left_0().size_full())
        .into_any_element()
}

/// A small labelled bar: green under 50%, yellow under 80%, red above.
fn usage_meter(label: &'static str, fraction: f32, text: String, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let fraction = fraction.clamp(0., 1.);
    let color = if fraction >= 0.8 { theme.red } else if fraction >= 0.5 { theme.yellow } else { theme.green };
    h_flex().gap_1p5().items_center()
        .child(div().w(px(28.)).text_xs().font_bold().text_color(theme.muted_foreground).child(label))
        .child(div().w(px(90.)).h(px(6.)).rounded(px(3.)).bg(theme.muted).overflow_hidden()
            .child(div().h_full().w(relative(fraction)).rounded(px(3.)).bg(color)))
        .child(div().text_xs().font_family("monospace").child(text))
}

/// Where the backup Save dialog starts: the user's Downloads folder, or
/// their home folder when there is none.
fn downloads_dir() -> std::path::PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(std::path::PathBuf::from);
    match home {
        Some(home) if home.join("Downloads").is_dir() => home.join("Downloads"),
        Some(home) => home,
        None => std::path::PathBuf::from("."),
    }
}

/// Row estimates in the table tree: 950, 12.4k, 3.1M.
fn compact_count(count: u64) -> String {
    match count {
        0..=999 => count.to_string(),
        1_000..=999_999 => format!("{:.1}k", count as f64 / 1_000.),
        _ => format!("{:.1}M", count as f64 / 1_000_000.),
    }
}

/// The 9 rwx bits from `ls -l`-style text such as `drwxr-sr-x`; `s`/`t`
/// count as executable, `S`/`T` as not. Unreadable text gives 0.
fn mode_from_filemode(text: &str) -> u32 {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() != 10 { return 0; }
    chars[1..].iter().enumerate().fold(0, |mode, (index, c)| {
        let set = match index % 3 { 0 => *c == 'r', 1 => *c == 'w', _ => matches!(c, 'x' | 's' | 't') };
        if set { mode | (1 << (8 - index)) } else { mode }
    })
}

fn mode_symbolic(mode: u32) -> String {
    (0..9).map(|index| if mode & (1 << (8 - index)) != 0 { ['r', 'w', 'x'][index % 3] } else { '-' }).collect()
}

fn is_markdown_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(_, ext)| matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown" | "mdown" | "mkd"))
}

/// Folder containing `path` (`/` for a top-level entry).
fn parent_dir(path: &str) -> String {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) | None => "/".into(),
        Some((parent, _)) => parent.into(),
    }
}

/// Appends the subfolders of `path` (and, for open ones, their own
/// subfolders) to the Move picker's rows. Recursion depth is how many
/// folders the user has opened, and each frame is small.
fn push_move_rows(
    nodes: &std::collections::HashMap<String, crate::remote_directory::Directory>,
    picker: &MovePicker, path: &str, depth: usize, rows: &mut Vec<MoveRow>,
) {
    let Some(directory) = nodes.get(path) else {
        rows.push(MoveRow::Note { depth, loading: true });
        return;
    };
    let mut any = false;
    for entry in directory.entries.iter().filter(|entry| entry.directory()) {
        let child = child_path(path, &entry.name);
        if child == picker.from { continue; }
        any = true;
        let expanded = picker.expanded.contains(&child);
        rows.push(MoveRow::Folder { path: child.clone(), name: entry.name.clone(), depth, expanded });
        if expanded { push_move_rows(nodes, picker, &child, depth + 1, rows); }
    }
    if !any { rows.push(MoveRow::Note { depth, loading: false }); }
}

/// Docker Compose file names: `compose.yaml`, `docker-compose.yml`, and the
/// usual variants like `docker-compose.prod.yml` / `compose.override.yaml`.
fn is_compose_file(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let Some(stem) = name.strip_suffix(".yml").or_else(|| name.strip_suffix(".yaml")) else { return false; };
    ["docker-compose", "compose"].iter().any(|base| stem == *base || stem.strip_prefix(base).is_some_and(|rest| rest.starts_with('.')))
}

/// POSIX single-quoting, so a path is passed to the shell as one literal word.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Validates a user-typed file/folder NAME (not a full path), for the New
/// File/New Folder/Rename dialogs: non-empty, no path separator, no control
/// characters, and not `.`/`..`.
fn validate_fs_name(name: &str) -> Result<String, ()> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.chars().any(char::is_control) {
        return Err(());
    }
    Ok(name.to_string())
}

fn size_label(entry: &crate::remote_directory::Entry, t: &'static i18n::Strings) -> String {
    if entry.directory() { return "Folder".into(); }
    match entry.size {
        Some(size) if size >= 1024 * 1024 => format!("{:.1} MiB", size as f64 / 1_048_576.0),
        Some(size) if size >= 1024 => format!("{:.1} KiB", size as f64 / 1024.0),
        Some(size) => format!("{size} B"),
        None => t.not_available.into(),
    }
}
