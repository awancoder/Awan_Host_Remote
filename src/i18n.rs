//! UI text catalog. Every label, placeholder, button, and static message the
//! app renders comes from here instead of being written inline, so the whole
//! UI can switch between English and Indonesian at runtime. Backend-originated
//! status/error strings (ssh.rs, host_store.rs, packages.rs, live.rs,
//! remote_file.rs, remote_directory.rs, terminal.rs) are not yet routed
//! through this catalog — see the note in app.rs where `Terminal::status` is
//! read.
//!
//! Strings that don't actually differ between the two languages (proper
//! nouns, protocol names, technical tokens like "TCP"/"UFW", numeric
//! examples) are left as plain literals at their call site rather than
//! duplicated here with no real translation.

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Language {
    #[default]
    English,
    Indonesian,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::Indonesian];

    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Indonesian => "Bahasa Indonesia",
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Indonesian => "id",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code.trim() {
            "en" => Some(Self::English),
            "id" => Some(Self::Indonesian),
            _ => None,
        }
    }

    /// Reads the saved preference (see [`Self::persist`]); `None` if there is
    /// none yet, so the caller falls back to [`Self::detect_system`].
    pub fn load_saved() -> Option<Self> {
        let path = settings_path()?;
        let content = std::fs::read_to_string(path).ok()?;
        Self::from_code(content.trim())
    }

    /// Persists a manual choice from the Settings page so it survives a
    /// restart. Best-effort: a write failure (read-only profile, missing
    /// permissions) is silently ignored — the app keeps working for the
    /// session, it just re-detects next launch instead of remembering.
    pub fn persist(self) {
        let Some(path) = settings_path() else { return; };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, self.code());
    }

    /// Best-effort system-locale detection, used only when nothing was saved
    /// yet. Checks the POSIX locale environment variables first (set in Git
    /// Bash / WSL / Linux / macOS), then the Windows UI language. Anything
    /// other than a clear Indonesian match defaults to English.
    pub fn detect_system() -> Self {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
            if let Ok(value) = std::env::var(var) {
                let value = value.to_ascii_lowercase();
                if value.starts_with("id") { return Self::Indonesian; }
                if value.starts_with("en") { return Self::English; }
            }
        }
        #[cfg(windows)]
        {
            if windows_locale::is_indonesian() { return Self::Indonesian; }
        }
        Self::English
    }
}

fn settings_path() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|dir| std::path::PathBuf::from(dir).join("AwanHostRemote").join("language"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(|dir| std::path::PathBuf::from(dir).join(".config").join("awan-host-remote").join("language"))
    }
}

#[cfg(windows)]
mod windows_locale {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }

    /// `LANG_INDONESIAN` from winnt.h; the primary language id is the low 10
    /// bits of the LANGID Windows returns.
    pub fn is_indonesian() -> bool {
        const LANG_INDONESIAN: u16 = 0x21;
        let langid = unsafe { GetUserDefaultUILanguage() };
        (langid & 0x3FF) == LANG_INDONESIAN
    }
}

pub struct Strings {
    pub app_tagline: &'static str,

    // Sidebar / shell
    pub sidebar_manage_hosts: &'static str,
    pub sidebar_workspace_label: &'static str,
    pub sidebar_no_server: &'static str,
    pub sidebar_settings_tooltip: &'static str,
    pub add_new_host: &'static str,

    // Settings page
    pub settings_title: &'static str,
    pub settings_appearance_heading: &'static str,
    pub settings_appearance_desc: &'static str,
    pub settings_theme_light: &'static str,
    pub settings_theme_dark: &'static str,
    pub settings_language_heading: &'static str,
    pub settings_language_desc: &'static str,
    pub settings_about_heading: &'static str,
    pub settings_about_text: &'static str,

    // Hosts page
    pub hosts_page_subtitle: &'static str,
    pub hosts_empty: &'static str,
    pub hosts_login_saved: &'static str,
    pub hosts_login_not_saved: &'static str,
    pub hosts_connect: &'static str,
    pub hosts_fill_login: &'static str,
    pub hosts_delete: &'static str,
    pub hosts_delete_dialog_desc: &'static str,
    pub hosts_delete_dialog_ok: &'static str,
    pub hosts_deleted_unsaved: &'static str,

    // Add host dialog
    pub add_host_field_name: &'static str,
    pub add_host_field_address: &'static str,
    pub add_host_field_username: &'static str,
    pub add_host_field_port: &'static str,
    pub add_host_field_auth: &'static str,
    pub add_host_use_key_checkbox: &'static str,
    pub add_host_cancel: &'static str,
    pub add_host_submit: &'static str,
    pub host_name_placeholder: &'static str,
    pub host_key_path_placeholder: &'static str,
    pub host_secret_placeholder: &'static str,
    pub host_switch_in_progress: &'static str,

    // Terminal page
    pub terminal_choose_host_first: &'static str,
    pub terminal_open_hosts: &'static str,
    pub terminal_disconnect: &'static str,
    pub terminal_connect_saved: &'static str,
    pub terminal_key_path_label: &'static str,
    pub terminal_passphrase_label: &'static str,
    pub terminal_password_label: &'static str,
    pub terminal_remember_login: &'static str,
    pub terminal_connect: &'static str,
    pub terminal_no_shell_hint: &'static str,

    // Trust prompt
    pub trust_title: &'static str,
    pub trust_reject: &'static str,
    pub trust_accept: &'static str,

    // Live status row (shared by Summary/Control/Ports/Tools/Databases)
    pub live_badge: &'static str,
    pub offline_badge: &'static str,
    pub live_reload: &'static str,
    pub live_auto_refresh_on: &'static str,
    pub live_auto_refresh_off: &'static str,
    pub live_offline_hint: &'static str,
    pub live_reading: &'static str,
    pub not_available: &'static str,

    // Summary page
    pub summary_cpu_usage: &'static str,
    pub summary_memory: &'static str,
    pub summary_storage: &'static str,
    pub summary_memory_note: &'static str,
    pub summary_storage_note: &'static str,
    pub summary_hostname: &'static str,
    pub summary_os: &'static str,
    pub summary_kernel: &'static str,
    pub summary_cpu_model: &'static str,
    pub summary_swap: &'static str,
    pub summary_uptime: &'static str,
    pub summary_ssh: &'static str,
    pub summary_ssh_connected: &'static str,
    pub summary_refresh_note: &'static str,
    pub summary_disks_heading: &'static str,
    pub summary_disk_mount: &'static str,
    pub summary_disk_device: &'static str,
    pub summary_disk_fstype: &'static str,
    pub summary_disk_usage: &'static str,
    pub summary_updates_heading: &'static str,
    pub summary_updates_none: &'static str,
    pub summary_reboot_required: &'static str,
    pub summary_power_heading: &'static str,

    // Server Control page
    pub control_update_system: &'static str,
    pub control_reboot: &'static str,
    pub control_shutdown: &'static str,

    // Firewall page
    pub ports_manage_heading: &'static str,
    pub ports_udp_checkbox: &'static str,
    pub ports_allow_label: &'static str,
    pub ports_deny_label_ufw: &'static str,
    pub ports_deny_label_other: &'static str,
    pub ports_footnote: &'static str,
    pub firewall_none_detected: &'static str,

    // Applications (Tools) page
    pub tools_reload_status: &'static str,
    pub tools_offline_hint: &'static str,
    pub tools_unknown: &'static str,
    pub tools_install: &'static str,
    pub tools_scanning: &'static str,
    pub tools_catalog_tab: &'static str,
    pub tools_installed_tab: &'static str,
    pub tools_search_placeholder: &'static str,
    pub tools_status_installed: &'static str,
    pub tools_status_detected_path: &'static str,
    pub tools_status_not_installed: &'static str,
    pub tools_uninstall: &'static str,
    pub tools_manual_detect_note: &'static str,
    pub tools_installed_note: &'static str,
    pub tools_package_search_note: &'static str,
    pub tools_install_desc_fallback: &'static str,
    pub tools_install_ok: &'static str,
    pub tools_uninstall_desc: &'static str,
    pub tools_uninstall_ok: &'static str,

    // Databases page
    pub db_field_port: &'static str,

    // Editor / Explorer
    pub editor_open_terminal: &'static str,
    pub editor_explorer_heading: &'static str,
    pub editor_connect_hint: &'static str,
    pub editor_search: &'static str,
    pub editor_show_hidden: &'static str,
    pub editor_search_placeholder: &'static str,
    pub editor_empty_folder: &'static str,
    pub editor_truncated_note: &'static str,
    pub editor_discard_draft_title: &'static str,
    pub editor_discard_draft_ok: &'static str,
    pub editor_open_button: &'static str,
    pub editor_reload_button: &'static str,
    pub editor_saving_note: &'static str,
    pub editor_reading_note: &'static str,
    pub editor_opened_success: &'static str,
    pub editor_saved_success: &'static str,
    pub editor_no_file_heading: &'static str,
    pub editor_no_file_desc: &'static str,
    pub editor_saved_badge: &'static str,
    pub editor_unsaved_badge: &'static str,
    pub editor_copy: &'static str,
    pub editor_preview: &'static str,
    pub editor_edit: &'static str,
    pub editor_save: &'static str,
    pub editor_wrong_host_warning: &'static str,
    pub editor_no_revision_warning: &'static str,
    pub editor_oversized_warning: &'static str,
    pub editor_path_placeholder: &'static str,

    // File manager: toolbar + context menu + dialogs
    pub fs_toolbar_new_file: &'static str,
    pub fs_toolbar_new_folder: &'static str,
    pub fs_toolbar_import_file: &'static str,
    pub fs_toolbar_import_folder: &'static str,
    pub fs_toolbar_paste: &'static str,
    pub fs_menu_open_terminal: &'static str,
    pub git_menu: &'static str,
    pub git_status: &'static str,
    pub git_pull: &'static str,
    pub git_push: &'static str,
    pub git_push_title: &'static str,
    pub terminal_copy: &'static str,
    pub terminal_paste: &'static str,
    pub compose_up_menu: &'static str,
    pub compose_up_title: &'static str,
    pub compose_up_ok: &'static str,
    pub compose_checking: &'static str,
    pub docker_missing: &'static str,
    pub compose_missing: &'static str,
    pub compose_setup_title: &'static str,
    pub compose_setup_ok: &'static str,
    pub terminal_not_ready: &'static str,
    pub terminal_fullscreen_app: &'static str,
    pub fs_menu_new_file: &'static str,
    pub fs_menu_new_folder: &'static str,
    pub fs_menu_paste: &'static str,
    pub fs_menu_copy: &'static str,
    pub fs_menu_rename: &'static str,
    pub fs_menu_move: &'static str,
    pub fs_menu_delete: &'static str,
    pub fs_new_file_title: &'static str,
    pub fs_new_file_label: &'static str,
    pub fs_new_file_submit: &'static str,
    pub fs_new_folder_title: &'static str,
    pub fs_new_folder_label: &'static str,
    pub fs_new_folder_submit: &'static str,
    pub fs_rename_title: &'static str,
    pub fs_rename_label: &'static str,
    pub fs_rename_submit: &'static str,
    pub fs_move_title: &'static str,
    pub fs_move_label: &'static str,
    pub fs_move_submit: &'static str,
    pub fs_dialog_cancel: &'static str,
    pub fs_dialog_location: &'static str,
    pub docker_sort_label: &'static str,
    pub docker_sort_project: &'static str,
    pub docker_sorted_cpu: &'static str,
    pub docker_sorted_ram: &'static str,
    pub docker_stats_manual: &'static str,
    pub db_tab_data: &'static str,
    pub db_tab_sql: &'static str,
    pub db_browser_shift_hint: &'static str,
    pub db_sql_template_tooltip: &'static str,
    pub db_sql_no_database: &'static str,
    pub db_sql_run: &'static str,
    pub db_sql_running: &'static str,
    pub db_sql_no_result: &'static str,
    pub db_sql_hint: &'static str,
    pub db_sql_confirm_title: &'static str,
    pub db_sql_confirm_destructive_title: &'static str,
    pub docker_browse_data: &'static str,
    pub db_browser_heading: &'static str,
    pub db_browser_back: &'static str,
    pub db_browser_pick_container: &'static str,
    pub db_browser_read_only: &'static str,
    pub db_browser_loading: &'static str,
    pub db_browser_no_databases: &'static str,
    pub db_browser_no_tables: &'static str,
    pub db_browser_pick_table: &'static str,
    pub db_browser_pick_table_desc: &'static str,
    pub db_browser_no_rows: &'static str,
    pub fs_menu_chmod: &'static str,
    pub fs_chmod_title: &'static str,
    pub fs_chmod_read: &'static str,
    pub fs_chmod_write: &'static str,
    pub fs_chmod_execute: &'static str,
    pub fs_chmod_owner: &'static str,
    pub fs_chmod_group: &'static str,
    pub fs_chmod_others: &'static str,
    pub fs_chmod_presets: &'static str,
    pub fs_chmod_preset_644: &'static str,
    pub fs_chmod_preset_755: &'static str,
    pub fs_chmod_preset_600: &'static str,
    pub fs_chmod_preset_700: &'static str,
    pub fs_chmod_recursive: &'static str,
    pub fs_chmod_recursive_note: &'static str,
    pub fs_chmod_dir_no_exec: &'static str,
    pub fs_chmod_world_writable: &'static str,
    pub fs_chmod_apply: &'static str,
    pub tools_column_package: &'static str,
    pub tools_column_version: &'static str,
    pub fs_move_pick_hint: &'static str,
    pub fs_move_current: &'static str,
    pub fs_move_loading: &'static str,
    pub fs_move_no_folders: &'static str,
    pub fs_move_target: &'static str,
    pub fs_move_type_path: &'static str,
    pub fs_move_into_self: &'static str,
    pub fs_delete_title: &'static str,
    pub fs_delete_ok: &'static str,
    pub fs_invalid_name: &'static str,
    pub fs_invalid_path: &'static str,
    pub fs_import_nothing: &'static str,
    pub fs_import_cancelled: &'static str,
    pub git_clone_toolbar: &'static str,
    pub git_clone_menu: &'static str,
    pub git_clone_title: &'static str,
    pub git_clone_url_label: &'static str,
    pub git_clone_url_placeholder: &'static str,
    pub git_clone_dir_label: &'static str,
    pub git_clone_dir_placeholder: &'static str,
    pub git_clone_note: &'static str,
    pub git_clone_submit: &'static str,
    pub git_clone_invalid_url: &'static str,
    pub git_clone_busy: &'static str,
    pub git_clone_failed_hint: &'static str,
    pub git_clone_https_hint: &'static str,
    pub git_clone_use_ssh: &'static str,
    pub git_account_button: &'static str,
    pub git_account_manage: &'static str,
    pub git_account_missing: &'static str,
    pub git_account_title: &'static str,
    pub git_account_close: &'static str,
    pub git_account_recheck: &'static str,
    pub git_account_intro: &'static str,
    pub git_account_checking: &'static str,
    pub git_account_not_connected: &'static str,
    pub git_step_host_title: &'static str,
    pub git_step_host_done: &'static str,
    pub git_step_host_pending: &'static str,
    pub git_step_host_scan: &'static str,
    pub git_step_host_trust: &'static str,
    pub git_step_key_title: &'static str,
    pub git_step_key_copy: &'static str,
    pub git_step_key_none: &'static str,
    pub git_step_key_generate: &'static str,
    pub git_step_test_title: &'static str,
    pub git_step_test_button: &'static str,

    // Persistent terminal panel
    pub terminal_footer_fullscreen_tooltip: &'static str,
    pub terminal_footer_restore_tooltip: &'static str,
    pub terminal_footer_close_tooltip: &'static str,
    pub terminal_footer_reopen: &'static str,

    // Maintenance confirmation dialogs
    pub confirm_update_title: &'static str,
    pub confirm_update_ok: &'static str,
    pub confirm_reboot_title: &'static str,
    pub confirm_shutdown_title: &'static str,
    pub confirm_power_desc_tail: &'static str,
    pub confirm_reboot_ok: &'static str,
    pub confirm_shutdown_ok: &'static str,
    pub confirm_firewall_open_title: &'static str,
    pub confirm_firewall_deny_title_ufw: &'static str,
    pub confirm_firewall_deny_title_other: &'static str,
    pub confirm_firewall_allow_ok: &'static str,
    pub confirm_firewall_apply_ok: &'static str,
    pub confirm_firewall_action_allow: &'static str,
    pub confirm_firewall_action_close: &'static str,
    pub confirm_firewall_ssh_warning: &'static str,
    pub firewall_inactive_warning: &'static str,
    pub firewall_enable_button: &'static str,
    pub firewall_no_rules: &'static str,
    pub firewall_disable_button: &'static str,
    pub firewall_delete_rule_button: &'static str,
    pub confirm_firewall_delete_title: &'static str,
    pub confirm_firewall_delete_ok: &'static str,
    pub firewall_reload_button: &'static str,
    pub confirm_firewall_reload_title: &'static str,
    pub confirm_firewall_reload_ok: &'static str,
    pub docker_heading: &'static str,
    pub docker_offline_hint: &'static str,
    pub docker_search_placeholder: &'static str,
    pub docker_loading: &'static str,
    pub docker_empty: &'static str,
    pub docker_no_match: &'static str,
    pub docker_standalone: &'static str,
    pub docker_permission_hint: &'static str,
    pub docker_permission_setup: &'static str,
    pub docker_logs: &'static str,
    pub docker_restart: &'static str,
    pub docker_restart_ok: &'static str,
    pub docker_stop: &'static str,
    pub docker_stop_ok: &'static str,
    pub docker_start: &'static str,
    pub docker_exec: &'static str,
    pub docker_exec_label: &'static str,
    pub docker_exec_note: &'static str,
    pub docker_exec_run: &'static str,
    pub docker_backup: &'static str,
    pub docker_show_backup: &'static str,
    pub docker_rebuild: &'static str,
    pub docker_rebuild_ok: &'static str,
    pub docker_rebuild_tooltip: &'static str,
    pub docker_down: &'static str,
    pub docker_down_ok: &'static str,
    pub docker_running: &'static str,
    pub docker_stopping: &'static str,
    pub docker_stop_task: &'static str,
    pub docker_done: &'static str,
    pub docker_close_output: &'static str,
    pub docker_no_output: &'static str,
    pub docker_following: &'static str,
    pub docker_close: &'static str,
    pub docker_network_create: &'static str,
    pub docker_network_tooltip: &'static str,
    pub docker_network_title: &'static str,
    pub docker_network_label: &'static str,
    pub docker_network_note: &'static str,
}

const EN: Strings = Strings {
    app_tagline: "Awan Host Remote",

    sidebar_manage_hosts: "Host",
    sidebar_workspace_label: "Workspace",
    sidebar_no_server: "No server connected",
    sidebar_settings_tooltip: "Settings",
    add_new_host: "Add New Host",

    settings_title: "Settings",
    settings_appearance_heading: "Appearance",
    settings_appearance_desc: "Choose a light or dark theme for the app.",
    settings_theme_light: "Light",
    settings_theme_dark: "Dark",
    settings_language_heading: "Language",
    settings_language_desc: "Choose the app's display language.",
    settings_about_heading: "About",
    settings_about_text: "Awan Host Remote — a desktop app for connecting to and controlling servers over SSH.",

    hosts_page_subtitle: "One profile per IP. A successfully saved login is available again after the app restarts.",
    hosts_empty: "No hosts yet. Add your first SSH server profile.",
    hosts_login_saved: "LOGIN SAVED",
    hosts_login_not_saved: "NOT SAVED",
    hosts_connect: "Connect",
    hosts_fill_login: "Enter login",
    hosts_delete: "Delete",
    hosts_delete_dialog_desc: "The profile and any login the app saved will be deleted. This host's session is closed if active. The private key file and the server itself are not touched.",
    hosts_delete_dialog_ok: "Delete profile & login",
    hosts_deleted_unsaved: "Unsaved profile removed from the list.",

    add_host_field_name: "Host name",
    add_host_field_address: "IP address / hostname",
    add_host_field_username: "Username",
    add_host_field_port: "SSH port",
    add_host_field_auth: "Authentication",
    add_host_use_key_checkbox: "Use SSH key (instead of password)",
    add_host_cancel: "Cancel",
    add_host_submit: "Add & open Terminal",
    host_name_placeholder: "e.g. Production API",
    host_key_path_placeholder: "C:\\Users\\name\\.ssh\\id_ed25519",
    host_secret_placeholder: "Password / passphrase",
    host_switch_in_progress: "Wait for the file operation to finish before switching hosts.",

    terminal_choose_host_first: "Choose a host first",
    terminal_open_hosts: "Open Hosts",
    terminal_disconnect: "Disconnect",
    terminal_connect_saved: "Connect with saved login",
    terminal_key_path_label: "Private key path (absolute)",
    terminal_passphrase_label: "Passphrase (optional)",
    terminal_password_label: "SSH password",
    terminal_remember_login: "Save login after success (OS credential store)",
    terminal_connect: "Connect SSH",
    terminal_no_shell_hint: "Connect via SSH to open the server shell.",

    trust_title: "Verify SSH server",
    trust_reject: "Reject",
    trust_accept: "Trust & save",

    live_badge: "LIVE · SSH",
    offline_badge: "OFFLINE",
    live_reload: "Reload",
    live_auto_refresh_on: "Auto-refresh: On",
    live_auto_refresh_off: "Auto-refresh: Off",
    live_offline_hint: "Connect via SSH to read server data. No sample data is shown.",
    live_reading: "Reading server...",
    not_available: "Not available",

    summary_cpu_usage: "CPU USAGE",
    summary_memory: "MEMORY",
    summary_storage: "STORAGE /",
    summary_memory_note: "MemTotal − MemAvailable",
    summary_storage_note: "Server root filesystem",
    summary_hostname: "Hostname",
    summary_os: "OS",
    summary_kernel: "Kernel",
    summary_cpu_model: "CPU Model",
    summary_swap: "Swap",
    summary_uptime: "Uptime",
    summary_ssh: "SSH",
    summary_ssh_connected: "Connected to active host",
    summary_refresh_note: "Refreshes every 5 seconds while Summary is open.",
    summary_disks_heading: "Storage Devices",
    summary_disk_mount: "Mount",
    summary_disk_device: "Device",
    summary_disk_fstype: "Filesystem",
    summary_disk_usage: "Used / Total",
    summary_updates_heading: "System Updates",
    summary_updates_none: "System is up to date.",
    summary_reboot_required: "A reboot is required to finish applying recent updates.",
    summary_power_heading: "Power",

    control_update_system: "Update System",
    control_reboot: "Reboot server",
    control_shutdown: "Shutdown server",

    ports_manage_heading: "Manage port rules",
    ports_udp_checkbox: "UDP (default TCP)",
    ports_allow_label: "Add port rule",
    ports_deny_label_ufw: "Add deny",
    ports_deny_label_other: "Remove port rule",
    ports_footnote: "Listening doesn't mean the firewall allows it. Cloud/security-group rules aren't included. UFW adds an allow/deny rule; firewalld adds/removes the port on the default zone for the runtime only.",
    firewall_none_detected: "No supported firewall (UFW or firewalld) was detected on this server.",

    tools_reload_status: "Reload status",
    tools_offline_hint: "Connect via SSH to the real server to check and manage applications. Status is unknown until then.",
    tools_unknown: "Unknown",
    tools_install: "Install",
    tools_scanning: "Checking installed packages on the server...",
    tools_catalog_tab: "App catalog",
    tools_installed_tab: "All installed packages",
    tools_search_placeholder: "Search package name or version...",
    tools_status_installed: "Installed",
    tools_status_detected_path: "Detected in PATH",
    tools_status_not_installed: "Not Installed",
    tools_uninstall: "Uninstall",
    tools_manual_detect_note: "An executable was found, but no catalog package was. Check how it was installed before changing anything.",
    tools_installed_note: "\"Installed\" shows a package is present; it doesn't mean the service is running or configured.",
    tools_package_search_note: "This list includes libraries and system packages. Uninstalling can affect services that depend on them.",
    tools_install_desc_fallback: "The package is installed from a repository already configured on the server.",
    tools_install_ok: "Install",
    tools_uninstall_desc: "Other apps or services may be affected. Review the package manager's plan and answer its prompts in the operation terminal. purge/autoremove are not used.",
    tools_uninstall_ok: "Continue",

    db_field_port: "Port",

    editor_open_terminal: "Open Terminal →",
    editor_explorer_heading: "Explorer",
    editor_connect_hint: "Connect via SSH to browse server folders.",
    editor_search: "Search",
    editor_show_hidden: "Show hidden files",
    editor_search_placeholder: "Search files...",
    editor_empty_folder: "Folder is empty, or no name matches the filter.",
    editor_truncated_note: "Capped at 2,000 entries/1 MiB for this folder. Search within the Home folder to narrow it down.",
    editor_discard_draft_title: "Replace the unsaved draft?",
    editor_discard_draft_ok: "Discard draft & open",
    editor_open_button: "Open",
    editor_reload_button: "Reload",
    editor_saving_note: "Saving file to the server...",
    editor_reading_note: "Reading file from the server...",
    editor_opened_success: "File opened from the server.",
    editor_saved_success: "File saved to the server.",
    editor_no_file_heading: "Choose a file to edit",
    editor_no_file_desc: "Click a folder in Explorer to go in. Click a text file to open it here. You can also type a file path directly above.",
    editor_saved_badge: "SAVED",
    editor_unsaved_badge: "UNSAVED",
    editor_copy: "Copy",
    editor_preview: "Preview",
    editor_edit: "Edit",
    editor_save: "Save",
    editor_wrong_host_warning: "This draft belongs to a different host. Reconnect to that host to save it.",
    editor_no_revision_warning: "Copy the draft before reloading to check the server's file. Saving is disabled until the file is reopened.",
    editor_oversized_warning: "Content exceeds 1 MiB. Trim the text before saving.",
    editor_path_placeholder: "File path to open directly...",

    fs_toolbar_new_file: "New file",
    fs_toolbar_new_folder: "New folder",
    fs_toolbar_import_file: "Import file(s)",
    fs_toolbar_import_folder: "Import folder",
    fs_toolbar_paste: "Paste",
    fs_menu_open_terminal: "Open in Terminal",
    git_menu: "Git",
    git_status: "Status",
    git_pull: "Pull",
    git_push: "Push",
    git_push_title: "Push to the remote?",
    terminal_copy: "Copy",
    terminal_paste: "Paste",
    compose_up_menu: "Run docker compose up -d --build",
    compose_up_title: "Run docker compose up?",
    compose_up_ok: "Run",
    compose_checking: "Checking Docker access on the server...",
    docker_missing: "Docker isn't installed on this server. Install Docker Engine from the Applications page first.",
    compose_missing: "The docker compose plugin isn't installed on this server. Install Docker Compose from the Applications page first.",
    compose_setup_title: "Set up Docker access (one time)",
    compose_setup_ok: "Set up access & run",
    terminal_not_ready: "The terminal isn't ready. Connect over SSH first, then try again.",
    terminal_fullscreen_app: "A full-screen program (vim, htop, less, ...) is open in the terminal. Exit it first, then try again.",
    fs_menu_new_file: "New file here",
    fs_menu_new_folder: "New folder here",
    fs_menu_paste: "Paste here",
    fs_menu_copy: "Copy",
    fs_menu_rename: "Rename",
    fs_menu_move: "Move to...",
    fs_menu_delete: "Delete",
    fs_new_file_title: "New file",
    fs_new_file_label: "File name",
    fs_new_file_submit: "Create",
    fs_new_folder_title: "New folder",
    fs_new_folder_label: "Folder name",
    fs_new_folder_submit: "Create",
    fs_rename_title: "Rename",
    fs_rename_label: "New name",
    fs_rename_submit: "Rename",
    fs_move_title: "Move to...",
    fs_move_label: "Destination path",
    fs_move_submit: "Move",
    fs_dialog_cancel: "Cancel",
    fs_dialog_location: "Created in:",
    docker_sort_label: "Sort:",
    docker_sort_project: "Project",
    docker_sorted_cpu: "Highest CPU first",
    docker_sorted_ram: "Highest RAM first",
    docker_stats_manual: "Usage as of the last load; turn on Auto-refresh for updates every 10 s.",
    db_tab_data: "Data",
    db_tab_sql: "SQL",
    db_browser_shift_hint: "Shift + mouse wheel scrolls sideways. Drag a column edge to resize it.",
    db_sql_template_tooltip: "Insert a starter statement into the editor",
    db_sql_no_database: "No database chosen; click one on the left",
    db_sql_run: "Run",
    db_sql_running: "Running query...",
    db_sql_no_result: "The query finished with no rows.",
    db_sql_hint: "SELECT, SHOW, and EXPLAIN run right away in a read-only session. CREATE, INSERT, UPDATE, DELETE, ALTER, and DROP ask for confirmation first. The query runs in the database selected on the left.",
    db_sql_confirm_title: "Run this query? It changes the database",
    db_sql_confirm_destructive_title: "Run this query? It deletes or alters data",
    docker_browse_data: "Browse data (databases & tables)",
    db_browser_heading: "Database Browser",
    db_browser_back: "Docker",
    db_browser_pick_container: "Open a database container from the Docker page with the table button.",
    db_browser_read_only: "READ ONLY",
    db_browser_loading: "Loading...",
    db_browser_no_databases: "No databases found.",
    db_browser_no_tables: "No tables",
    db_browser_pick_table: "Choose a table",
    db_browser_pick_table_desc: "Open a database on the left, then click a table to see its rows.",
    db_browser_no_rows: "No rows on this page.",
    fs_menu_chmod: "Permissions (chmod)",
    fs_chmod_title: "Change permissions",
    fs_chmod_read: "Read",
    fs_chmod_write: "Write",
    fs_chmod_execute: "Execute",
    fs_chmod_owner: "Owner",
    fs_chmod_group: "Group",
    fs_chmod_others: "Others",
    fs_chmod_presets: "Presets:",
    fs_chmod_preset_644: "Regular file: owner edits, everyone reads",
    fs_chmod_preset_755: "Folder or script: owner edits, everyone opens/runs",
    fs_chmod_preset_600: "Private file (keys, .env): owner only",
    fs_chmod_preset_700: "Private folder or script: owner only",
    fs_chmod_recursive: "Also apply to everything inside this folder",
    fs_chmod_recursive_note: "Subfolders get these permissions. Files get them without Execute, unless a file is already executable (like chmod -R a=rwX). Symlinks are skipped.",
    fs_chmod_dir_no_exec: "Without Execute for the owner, this folder cannot be opened, even by you.",
    fs_chmod_world_writable: "Write for Others lets every account on the server change this. Usually not wanted.",
    fs_chmod_apply: "Apply",
    tools_column_package: "Package",
    tools_column_version: "Version",
    fs_move_pick_hint: "Click the destination folder, then Move. Click the arrow to open a folder without choosing it.",
    fs_move_current: "current location",
    fs_move_loading: "Loading...",
    fs_move_no_folders: "No subfolders",
    fs_move_target: "Moves to:",
    fs_move_type_path: "Type a path instead",
    fs_move_into_self: "A folder cannot be moved into itself.",
    fs_delete_title: "Delete this permanently?",
    fs_delete_ok: "Delete",
    fs_invalid_name: "Name cannot be empty, contain '/', or control characters.",
    fs_invalid_path: "Enter an absolute Linux path, e.g. /home/ubuntu/app.",
    fs_import_nothing: "Nothing was selected to import.",
    fs_import_cancelled: "Import cancelled.",
    git_clone_toolbar: "Git clone",
    git_clone_menu: "Git clone here",
    git_clone_title: "Clone a Git repository",
    git_clone_url_label: "Repository URL",
    git_clone_url_placeholder: "https://github.com/user/repo.git",
    git_clone_dir_label: "Folder name (optional)",
    git_clone_dir_placeholder: "Leave empty to use the repository name",
    git_clone_note: "Runs git clone on the server. For a private repository, set up the SSH login above and use its SSH URL (git@github.com:user/repo.git); password prompts are disabled.",
    git_clone_submit: "Clone",
    git_clone_invalid_url: "Use an https://, ssh://, git:// or user@host:path repository URL.",
    git_clone_busy: "Another server operation is still running. Try again once it finishes.",
    git_clone_failed_hint: "Git's own message in the terminal panel shows why — usually a wrong URL, a private repository the server has no access to, or an unknown SSH host key.",
    git_clone_https_hint: "This HTTPS URL logs in with a username/token, not the server's SSH key, so it fails for private repositories. Use the SSH URL instead:",
    git_clone_use_ssh: "Use SSH URL",
    git_account_button: "Set up login",
    git_account_manage: "Manage",
    git_account_missing: "Git SSH login isn't set up on this server — needed for private repositories.",
    git_account_title: "Git SSH login",
    git_account_close: "Close",
    git_account_recheck: "Check again",
    git_account_intro: "Lets this server clone private repositories over SSH. The key is created and kept on the server; this app never sends it anywhere.",
    git_account_checking: "Checking the server...",
    git_account_not_connected: "Not connected yet. Complete the steps below.",
    git_step_host_title: "1. Verify the provider's host key",
    git_step_host_done: "The host key is trusted on this server.",
    git_step_host_pending: "The server has not connected to this provider before.",
    git_step_host_scan: "Fetch fingerprint",
    git_step_host_trust: "Fingerprint matches — trust it",
    git_step_key_title: "2. Add the server's SSH key to your account",
    git_step_key_copy: "Copy public key",
    git_step_key_none: "No SSH key found in ~/.ssh on this server.",
    git_step_key_generate: "Generate SSH key",
    git_step_test_title: "3. Test the connection",
    git_step_test_button: "Test connection",

    terminal_footer_fullscreen_tooltip: "Fullscreen",
    terminal_footer_restore_tooltip: "Restore",
    terminal_footer_close_tooltip: "Close terminal panel",
    terminal_footer_reopen: "Terminal",

    confirm_update_title: "Update the server's system?",
    confirm_update_ok: "Run update",
    confirm_reboot_title: "Reboot server?",
    confirm_shutdown_title: "Shutdown server?",
    confirm_power_desc_tail: "The server and its services will stop; the SSH connection will drop. Verify the result once the server is reachable again.",
    confirm_reboot_ok: "Reboot",
    confirm_shutdown_ok: "Shutdown",
    confirm_firewall_open_title: "Add port rule?",
    confirm_firewall_deny_title_ufw: "Add deny rule?",
    confirm_firewall_deny_title_other: "Remove port rule?",
    confirm_firewall_allow_ok: "Add rule",
    confirm_firewall_apply_ok: "Apply",
    confirm_firewall_action_allow: "Allow",
    confirm_firewall_action_close: "Close/remove rule for",
    confirm_firewall_ssh_warning: " WARNING: this is the active profile's SSH port — closing access can lock you out of your connection.",
    firewall_inactive_warning: "UFW is currently inactive. Rules can't be managed until it's enabled.",
    firewall_enable_button: "Enable UFW",
    firewall_no_rules: "No port rules are configured yet.",
    firewall_disable_button: "Disable firewall",
    firewall_delete_rule_button: "Delete",
    confirm_firewall_delete_title: "Delete this rule?",
    confirm_firewall_delete_ok: "Delete",
    firewall_reload_button: "Reload",
    confirm_firewall_reload_title: "Reload firewall rules?",
    confirm_firewall_reload_ok: "Reload",
    docker_heading: "Docker",
    docker_offline_hint: "Connect to a host to see its containers and Compose projects.",
    docker_search_placeholder: "Search container, image, or project",
    docker_loading: "Reading containers...",
    docker_empty: "No containers on this server yet.",
    docker_no_match: "No container matches the search.",
    docker_standalone: "Standalone containers",
    docker_permission_hint: "This SSH account cannot reach the Docker socket (permission denied). Add it to the docker group once (requires sudo), then reload this page.",
    docker_permission_setup: "Set up Docker access",
    docker_logs: "Logs (follow)",
    docker_restart: "Restart",
    docker_restart_ok: "Yes, restart",
    docker_stop: "Stop",
    docker_stop_ok: "Yes, stop",
    docker_start: "Start",
    docker_exec: "Run command in container",
    docker_exec_label: "Command",
    docker_exec_note: "Runs once with sh -c inside the container, without a TTY. For interactive programs, use the Terminal.",
    docker_exec_run: "Run",
    docker_backup: "Back up database to this computer",
    docker_show_backup: "Show file",
    docker_rebuild: "Rebuild & up",
    docker_rebuild_ok: "Yes, rebuild",
    docker_rebuild_tooltip: "docker compose up -d --build",
    docker_down: "Down",
    docker_down_ok: "Yes, take it down",
    docker_running: "Running...",
    docker_stopping: "Stopping...",
    docker_stop_task: "Stop",
    docker_done: "Done",
    docker_close_output: "Close output",
    docker_no_output: "Waiting for output...",
    docker_following: "Following logs...",
    docker_close: "Close",
    docker_network_create: "Create network",
    docker_network_tooltip: "docker network create",
    docker_network_title: "Create Docker network",
    docker_network_label: "Network name",
    docker_network_note: "Creates a bridge network (e.g. edge) that a reverse proxy and several Compose projects can share; declare it in compose.yaml as external: true. If the network already exists, nothing is changed.",
};

const ID: Strings = Strings {
    app_tagline: "Awan Host Remote",

    sidebar_manage_hosts: "Host",
    sidebar_workspace_label: "Ruang Kerja",
    sidebar_no_server: "Belum ada server terhubung",
    sidebar_settings_tooltip: "Pengaturan",
    add_new_host: "Tambah Host Baru",

    settings_title: "Pengaturan",
    settings_appearance_heading: "Tampilan",
    settings_appearance_desc: "Pilih tema terang atau gelap untuk aplikasi.",
    settings_theme_light: "Terang",
    settings_theme_dark: "Gelap",
    settings_language_heading: "Bahasa",
    settings_language_desc: "Pilih bahasa tampilan aplikasi.",
    settings_about_heading: "Tentang",
    settings_about_text: "Awan Host Remote — aplikasi desktop untuk menghubungkan dan mengontrol server melalui SSH.",

    hosts_page_subtitle: "Satu profil per IP. Login yang berhasil disimpan tersedia lagi setelah aplikasi dibuka ulang.",
    hosts_empty: "Belum ada host. Tambahkan profil server SSH pertama Anda.",
    hosts_login_saved: "LOGIN TERSIMPAN",
    hosts_login_not_saved: "BELUM DISIMPAN",
    hosts_connect: "Sambungkan",
    hosts_fill_login: "Isi login",
    hosts_delete: "Hapus",
    hosts_delete_dialog_desc: "Profil beserta login yang disimpan aplikasi akan dihapus. Sesi host ini ditutup jika aktif. File private key dan server tidak dihapus.",
    hosts_delete_dialog_ok: "Hapus profil & login",
    hosts_deleted_unsaved: "Profil yang belum disimpan telah dihapus dari daftar.",

    add_host_field_name: "Nama host",
    add_host_field_address: "IP address / hostname",
    add_host_field_username: "Username",
    add_host_field_port: "SSH port",
    add_host_field_auth: "Autentikasi",
    add_host_use_key_checkbox: "Pakai SSH key (bukan password)",
    add_host_cancel: "Batal",
    add_host_submit: "Tambah & buka Terminal",
    host_name_placeholder: "Contoh: Production API",
    host_key_path_placeholder: "C:\\Users\\nama\\.ssh\\id_ed25519",
    host_secret_placeholder: "Password / passphrase",
    host_switch_in_progress: "Tunggu operasi file selesai sebelum mengganti host.",

    terminal_choose_host_first: "Pilih host terlebih dahulu",
    terminal_open_hosts: "Buka Hosts",
    terminal_disconnect: "Putuskan",
    terminal_connect_saved: "Sambungkan dengan login tersimpan",
    terminal_key_path_label: "Path private key (absolut)",
    terminal_passphrase_label: "Passphrase (opsional)",
    terminal_password_label: "Password SSH",
    terminal_remember_login: "Simpan login setelah berhasil (penyimpanan kredensial OS)",
    terminal_connect: "Sambungkan SSH",
    terminal_no_shell_hint: "Sambungkan SSH untuk membuka shell server.",

    trust_title: "Verifikasi server SSH",
    trust_reject: "Tolak",
    trust_accept: "Percayai & simpan",

    live_badge: "LIVE · SSH",
    offline_badge: "OFFLINE",
    live_reload: "Muat ulang",
    live_auto_refresh_on: "Refresh otomatis: Aktif",
    live_auto_refresh_off: "Refresh otomatis: Nonaktif",
    live_offline_hint: "Hubungkan SSH untuk membaca data server. Tidak ada data contoh.",
    live_reading: "Membaca server...",
    not_available: "Tidak tersedia",

    summary_cpu_usage: "CPU USAGE",
    summary_memory: "MEMORY",
    summary_storage: "STORAGE /",
    summary_memory_note: "MemTotal − MemAvailable",
    summary_storage_note: "Filesystem root server",
    summary_hostname: "Hostname",
    summary_os: "OS",
    summary_kernel: "Kernel",
    summary_cpu_model: "Model CPU",
    summary_swap: "Swap",
    summary_uptime: "Uptime",
    summary_ssh: "SSH",
    summary_ssh_connected: "Terhubung ke host aktif",
    summary_refresh_note: "Pembaruan setiap 5 detik saat Summary terbuka.",
    summary_disks_heading: "Perangkat Penyimpanan",
    summary_disk_mount: "Mount",
    summary_disk_device: "Device",
    summary_disk_fstype: "Filesystem",
    summary_disk_usage: "Terpakai / Total",
    summary_updates_heading: "Update Sistem",
    summary_updates_none: "Sistem sudah yang terbaru.",
    summary_reboot_required: "Reboot diperlukan untuk menyelesaikan penerapan pembaruan terbaru.",
    summary_power_heading: "Daya",

    control_update_system: "Update System",
    control_reboot: "Reboot server",
    control_shutdown: "Shutdown server",

    ports_manage_heading: "Kelola aturan port",
    ports_udp_checkbox: "UDP (default TCP)",
    ports_allow_label: "Tambah izin port",
    ports_deny_label_ufw: "Tambah deny",
    ports_deny_label_other: "Hapus izin port",
    ports_footnote: "Listening tidak berarti diizinkan firewall. Aturan cloud/security group tidak termasuk. UFW menambah allow/deny; firewalld menambah/menghapus port pada default zone untuk runtime.",
    firewall_none_detected: "Tidak ada firewall yang didukung (UFW atau firewalld) terdeteksi pada server ini.",

    tools_reload_status: "Muat ulang status",
    tools_offline_hint: "Hubungkan SSH ke server asli untuk memeriksa dan mengelola aplikasi. Status belum diketahui.",
    tools_unknown: "Unknown",
    tools_install: "Install",
    tools_scanning: "Memeriksa paket yang terpasang di server...",
    tools_catalog_tab: "Katalog aplikasi",
    tools_installed_tab: "Semua paket terpasang",
    tools_search_placeholder: "Cari nama paket atau versi...",
    tools_status_installed: "Installed",
    tools_status_detected_path: "Terdeteksi di PATH",
    tools_status_not_installed: "Not Installed",
    tools_uninstall: "Uninstall",
    tools_manual_detect_note: "Executable tersedia, tetapi paket katalog tidak ditemukan. Periksa asal instalasi sebelum mengubahnya.",
    tools_installed_note: "Installed menunjukkan paket terpasang; tidak menjamin layanan sedang berjalan atau sudah dikonfigurasi.",
    tools_package_search_note: "Daftar ini mencakup library dan paket sistem. Uninstall dapat memengaruhi layanan yang bergantung padanya.",
    tools_install_desc_fallback: "Paket dipasang dari repository yang sudah dikonfigurasi pada server.",
    tools_install_ok: "Install",
    tools_uninstall_desc: "Aplikasi atau layanan lain dapat ikut terpengaruh. Tinjau rencana package manager dan jawab konfirmasinya di terminal operasi. Tidak memakai purge/autoremove.",
    tools_uninstall_ok: "Lanjutkan",

    db_field_port: "Port",

    editor_open_terminal: "Buka Terminal →",
    editor_explorer_heading: "Explorer",
    editor_connect_hint: "Hubungkan SSH untuk menjelajahi folder server.",
    editor_search: "Cari",
    editor_show_hidden: "Tampilkan file tersembunyi",
    editor_search_placeholder: "Cari file...",
    editor_empty_folder: "Folder kosong atau tidak ada nama yang cocok dengan filter.",
    editor_truncated_note: "Dibatasi 2.000 entri/1 MiB untuk folder ini. Gunakan pencarian folder Home untuk mempersempit.",
    editor_discard_draft_title: "Ganti draft yang belum disimpan?",
    editor_discard_draft_ok: "Buang draft & buka",
    editor_open_button: "Buka",
    editor_reload_button: "Muat ulang",
    editor_saving_note: "Menyimpan file ke server...",
    editor_reading_note: "Membaca file dari server...",
    editor_opened_success: "File berhasil dibuka dari server.",
    editor_saved_success: "File berhasil disimpan ke server.",
    editor_no_file_heading: "Pilih file untuk diedit",
    editor_no_file_desc: "Klik folder di Explorer untuk masuk. Klik file teks untuk membukanya di sini. Anda juga dapat memasukkan path file langsung di atas.",
    editor_saved_badge: "TERSIMPAN",
    editor_unsaved_badge: "BELUM DISIMPAN",
    editor_copy: "Salin",
    editor_preview: "Preview",
    editor_edit: "Edit",
    editor_save: "Simpan",
    editor_wrong_host_warning: "Draft milik host lain. Sambungkan kembali host tersebut untuk menyimpan.",
    editor_no_revision_warning: "Salin draft sebelum memuat ulang untuk memeriksa file server. Penyimpanan ulang dinonaktifkan sampai file dibuka kembali.",
    editor_oversized_warning: "Konten melebihi 1 MiB. Kurangi teks sebelum menyimpan.",
    editor_path_placeholder: "Path file untuk dibuka langsung...",

    fs_toolbar_new_file: "File baru",
    fs_toolbar_new_folder: "Folder baru",
    fs_toolbar_import_file: "Impor file",
    fs_toolbar_import_folder: "Impor folder",
    fs_toolbar_paste: "Tempel",
    fs_menu_open_terminal: "Buka di Terminal",
    git_menu: "Git",
    git_status: "Status",
    git_pull: "Pull",
    git_push: "Push",
    git_push_title: "Push ke remote?",
    terminal_copy: "Salin",
    terminal_paste: "Tempel",
    compose_up_menu: "Jalankan docker compose up -d --build",
    compose_up_title: "Jalankan docker compose up?",
    compose_up_ok: "Jalankan",
    compose_checking: "Memeriksa akses Docker di server...",
    docker_missing: "Docker belum terpasang di server ini. Pasang Docker Engine dari halaman Applications terlebih dahulu.",
    compose_missing: "Plugin docker compose belum terpasang di server ini. Pasang Docker Compose dari halaman Applications terlebih dahulu.",
    compose_setup_title: "Atur akses Docker (sekali saja)",
    compose_setup_ok: "Atur akses & jalankan",
    terminal_not_ready: "Terminal belum siap. Hubungkan SSH terlebih dahulu, lalu coba lagi.",
    terminal_fullscreen_app: "Program layar penuh (vim, htop, less, ...) sedang terbuka di terminal. Keluar dulu dari program itu, lalu coba lagi.",
    fs_menu_new_file: "File baru di sini",
    fs_menu_new_folder: "Folder baru di sini",
    fs_menu_paste: "Tempel di sini",
    fs_menu_copy: "Salin",
    fs_menu_rename: "Ganti nama",
    fs_menu_move: "Pindahkan ke...",
    fs_menu_delete: "Hapus",
    fs_new_file_title: "File baru",
    fs_new_file_label: "Nama file",
    fs_new_file_submit: "Buat",
    fs_new_folder_title: "Folder baru",
    fs_new_folder_label: "Nama folder",
    fs_new_folder_submit: "Buat",
    fs_rename_title: "Ganti nama",
    fs_rename_label: "Nama baru",
    fs_rename_submit: "Ganti nama",
    fs_move_title: "Pindahkan ke...",
    fs_move_label: "Path tujuan",
    fs_move_submit: "Pindahkan",
    fs_dialog_cancel: "Batal",
    fs_dialog_location: "Dibuat di:",
    docker_sort_label: "Urutkan:",
    docker_sort_project: "Project",
    docker_sorted_cpu: "CPU tertinggi lebih dulu",
    docker_sorted_ram: "RAM tertinggi lebih dulu",
    docker_stats_manual: "Pemakaian saat terakhir dimuat; nyalakan Auto-refresh untuk pembaruan tiap 10 detik.",
    db_tab_data: "Data",
    db_tab_sql: "SQL",
    db_browser_shift_hint: "Shift + scroll mouse untuk geser ke kiri/kanan. Tarik tepi kolom untuk mengubah lebarnya.",
    db_sql_template_tooltip: "Isi editor dengan contoh statement",
    db_sql_no_database: "Belum memilih database; klik di sebelah kiri",
    db_sql_run: "Jalankan",
    db_sql_running: "Menjalankan query...",
    db_sql_no_result: "Query selesai tanpa baris hasil.",
    db_sql_hint: "SELECT, SHOW, dan EXPLAIN langsung dijalankan dalam sesi hanya-baca. CREATE, INSERT, UPDATE, DELETE, ALTER, dan DROP meminta konfirmasi dulu. Query dijalankan di database yang dipilih di sebelah kiri.",
    db_sql_confirm_title: "Jalankan query ini? Database akan berubah",
    db_sql_confirm_destructive_title: "Jalankan query ini? Data/struktur akan dihapus atau diubah",
    docker_browse_data: "Lihat data (database & tabel)",
    db_browser_heading: "Database Browser",
    db_browser_back: "Docker",
    db_browser_pick_container: "Buka container database dari halaman Docker lewat tombol tabel.",
    db_browser_read_only: "HANYA BACA",
    db_browser_loading: "Memuat...",
    db_browser_no_databases: "Tidak ada database.",
    db_browser_no_tables: "Tidak ada tabel",
    db_browser_pick_table: "Pilih tabel",
    db_browser_pick_table_desc: "Buka database di sebelah kiri, lalu klik tabel untuk melihat isinya.",
    db_browser_no_rows: "Tidak ada baris di halaman ini.",
    fs_menu_chmod: "Ubah izin (chmod)",
    fs_chmod_title: "Ubah izin",
    fs_chmod_read: "Baca",
    fs_chmod_write: "Tulis",
    fs_chmod_execute: "Eksekusi",
    fs_chmod_owner: "Pemilik",
    fs_chmod_group: "Grup",
    fs_chmod_others: "Lainnya",
    fs_chmod_presets: "Preset:",
    fs_chmod_preset_644: "File biasa: pemilik mengedit, semua bisa membaca",
    fs_chmod_preset_755: "Folder atau script: pemilik mengedit, semua bisa membuka/menjalankan",
    fs_chmod_preset_600: "File privat (key, .env): hanya pemilik",
    fs_chmod_preset_700: "Folder atau script privat: hanya pemilik",
    fs_chmod_recursive: "Terapkan juga ke seluruh isi folder ini",
    fs_chmod_recursive_note: "Subfolder mendapat izin ini. File mendapat izin yang sama tanpa Eksekusi, kecuali file yang memang sudah executable (seperti chmod -R a=rwX). Symlink dilewati.",
    fs_chmod_dir_no_exec: "Tanpa izin Eksekusi untuk pemilik, folder ini tidak bisa dibuka, bahkan oleh Anda.",
    fs_chmod_world_writable: "Izin Tulis untuk Lainnya membuat semua akun di server bisa mengubahnya. Biasanya tidak diinginkan.",
    fs_chmod_apply: "Terapkan",
    tools_column_package: "Paket",
    tools_column_version: "Versi",
    fs_move_pick_hint: "Klik folder tujuan, lalu Move. Klik panah untuk membuka folder tanpa memilihnya.",
    fs_move_current: "lokasi sekarang",
    fs_move_loading: "Memuat...",
    fs_move_no_folders: "Tidak ada subfolder",
    fs_move_target: "Dipindah ke:",
    fs_move_type_path: "Ketik path saja",
    fs_move_into_self: "Folder tidak dapat dipindahkan ke dalam dirinya sendiri.",
    fs_delete_title: "Hapus permanen?",
    fs_delete_ok: "Hapus",
    fs_invalid_name: "Nama tidak boleh kosong, mengandung '/', atau karakter kontrol.",
    fs_invalid_path: "Masukkan path absolut Linux, misalnya /home/ubuntu/app.",
    fs_import_nothing: "Tidak ada yang dipilih untuk diimpor.",
    fs_import_cancelled: "Impor dibatalkan.",
    git_clone_toolbar: "Git clone",
    git_clone_menu: "Git clone di sini",
    git_clone_title: "Clone repository Git",
    git_clone_url_label: "URL repository",
    git_clone_url_placeholder: "https://github.com/user/repo.git",
    git_clone_dir_label: "Nama folder (opsional)",
    git_clone_dir_placeholder: "Kosongkan untuk memakai nama repository",
    git_clone_note: "Menjalankan git clone di server. Untuk repository privat, atur login SSH di atas lalu pakai URL SSH-nya (git@github.com:user/repo.git); prompt password dinonaktifkan.",
    git_clone_submit: "Clone",
    git_clone_invalid_url: "Gunakan URL repository https://, ssh://, git:// atau user@host:path.",
    git_clone_busy: "Operasi server lain masih berjalan. Coba lagi setelah selesai.",
    git_clone_failed_hint: "Pesan git di panel terminal menunjukkan penyebabnya — biasanya URL salah, repository privat yang tidak bisa diakses server, atau host key SSH yang belum dikenal.",
    git_clone_https_hint: "URL HTTPS ini login memakai username/token, bukan SSH key server, sehingga gagal untuk repository privat. Pakai URL SSH:",
    git_clone_use_ssh: "Pakai URL SSH",
    git_account_button: "Atur login",
    git_account_manage: "Kelola",
    git_account_missing: "Login SSH Git belum diatur di server ini — diperlukan untuk repository privat.",
    git_account_title: "Login SSH Git",
    git_account_close: "Tutup",
    git_account_recheck: "Periksa ulang",
    git_account_intro: "Agar server ini bisa clone repository privat lewat SSH. Key dibuat dan disimpan di server; aplikasi ini tidak pernah mengirimnya ke mana pun.",
    git_account_checking: "Memeriksa server...",
    git_account_not_connected: "Belum terhubung. Selesaikan langkah di bawah.",
    git_step_host_title: "1. Verifikasi host key penyedia",
    git_step_host_done: "Host key sudah dipercaya di server ini.",
    git_step_host_pending: "Server ini belum pernah terhubung ke penyedia ini.",
    git_step_host_scan: "Ambil fingerprint",
    git_step_host_trust: "Fingerprint cocok — percayai",
    git_step_key_title: "2. Tambahkan SSH key server ke akun Anda",
    git_step_key_copy: "Salin public key",
    git_step_key_none: "Belum ada SSH key di ~/.ssh pada server ini.",
    git_step_key_generate: "Buat SSH key",
    git_step_test_title: "3. Uji koneksi",
    git_step_test_button: "Uji koneksi",

    terminal_footer_fullscreen_tooltip: "Layar penuh",
    terminal_footer_restore_tooltip: "Pulihkan",
    terminal_footer_close_tooltip: "Tutup panel terminal",
    terminal_footer_reopen: "Terminal",

    confirm_update_title: "Update system server?",
    confirm_update_ok: "Jalankan update",
    confirm_reboot_title: "Reboot server?",
    confirm_shutdown_title: "Shutdown server?",
    confirm_power_desc_tail: "Server dan layanan akan berhenti; koneksi SSH akan terputus. Hasil perlu diverifikasi setelah server kembali tersedia.",
    confirm_reboot_ok: "Reboot",
    confirm_shutdown_ok: "Shutdown",
    confirm_firewall_open_title: "Tambah izin port?",
    confirm_firewall_deny_title_ufw: "Tambah deny port?",
    confirm_firewall_deny_title_other: "Hapus izin port?",
    confirm_firewall_allow_ok: "Tambah izin",
    confirm_firewall_apply_ok: "Terapkan",
    confirm_firewall_action_allow: "Mengizinkan",
    confirm_firewall_action_close: "Menutup/menghapus izin",
    confirm_firewall_ssh_warning: " PERINGATAN: port ini adalah port SSH profil aktif — menutup akses dapat mengunci koneksi Anda.",
    firewall_inactive_warning: "UFW sedang tidak aktif. Aturan tidak dapat dikelola sampai diaktifkan.",
    firewall_enable_button: "Aktifkan UFW",
    firewall_no_rules: "Belum ada aturan port yang dikonfigurasi.",
    firewall_disable_button: "Nonaktifkan firewall",
    firewall_delete_rule_button: "Hapus",
    confirm_firewall_delete_title: "Hapus aturan ini?",
    confirm_firewall_delete_ok: "Hapus",
    firewall_reload_button: "Reload",
    confirm_firewall_reload_title: "Reload aturan firewall?",
    confirm_firewall_reload_ok: "Reload",
    docker_heading: "Docker",
    docker_offline_hint: "Hubungkan host untuk melihat container dan project Compose-nya.",
    docker_search_placeholder: "Cari container, image, atau project",
    docker_loading: "Membaca container...",
    docker_empty: "Belum ada container di server ini.",
    docker_no_match: "Tidak ada container yang cocok dengan pencarian.",
    docker_standalone: "Container mandiri",
    docker_permission_hint: "Akun SSH ini belum bisa mengakses socket Docker (permission denied). Tambahkan ke grup docker sekali saja (butuh sudo), lalu muat ulang halaman ini.",
    docker_permission_setup: "Atur akses Docker",
    docker_logs: "Log (ikuti)",
    docker_restart: "Restart",
    docker_restart_ok: "Ya, restart",
    docker_stop: "Stop",
    docker_stop_ok: "Ya, hentikan",
    docker_start: "Start",
    docker_exec: "Jalankan perintah di container",
    docker_exec_label: "Perintah",
    docker_exec_note: "Dijalankan sekali dengan sh -c di dalam container, tanpa TTY. Untuk program interaktif, gunakan Terminal.",
    docker_exec_run: "Jalankan",
    docker_backup: "Backup database ke komputer ini",
    docker_show_backup: "Lihat file",
    docker_rebuild: "Rebuild & up",
    docker_rebuild_ok: "Ya, rebuild",
    docker_rebuild_tooltip: "docker compose up -d --build",
    docker_down: "Down",
    docker_down_ok: "Ya, down",
    docker_running: "Berjalan...",
    docker_stopping: "Menghentikan...",
    docker_stop_task: "Hentikan",
    docker_done: "Selesai",
    docker_close_output: "Tutup output",
    docker_no_output: "Menunggu output...",
    docker_following: "Mengikuti log...",
    docker_close: "Tutup",
    docker_network_create: "Buat network",
    docker_network_tooltip: "docker network create",
    docker_network_title: "Buat network Docker",
    docker_network_label: "Nama network",
    docker_network_note: "Membuat network bridge (mis. edge) yang dapat dipakai bersama oleh reverse proxy dan beberapa project Compose; deklarasikan di compose.yaml sebagai external: true. Jika network sudah ada, tidak ada yang diubah.",
};

pub fn strings(lang: Language) -> &'static Strings {
    match lang {
        Language::English => &EN,
        Language::Indonesian => &ID,
    }
}

// --- Dynamic (interpolated) messages -------------------------------------
// Kept as functions rather than catalog fields because word order around the
// interpolated values can differ between languages.

pub fn delete_host_title(lang: Language, host_name: &str) -> String {
    match lang {
        Language::English => format!("Delete profile {host_name}?"),
        Language::Indonesian => format!("Hapus profil {host_name}?"),
    }
}

pub fn profiles_skipped(lang: Language, skipped: usize) -> String {
    match lang {
        Language::English => format!("{skipped} saved profile(s) could not be read and were skipped. They were not deleted automatically."),
        Language::Indonesian => format!("{skipped} profil tersimpan tidak dapat dibaca dan dilewati. Entri tersebut tidak dihapus otomatis."),
    }
}

pub fn profile_deleted(lang: Language, address: &str) -> String {
    match lang {
        Language::English => format!("Saved profile and login for {address} deleted."),
        Language::Indonesian => format!("Profil dan login tersimpan {address} dihapus."),
    }
}

pub fn host_already_exists(lang: Language) -> String {
    match lang {
        Language::English => "This IP/address already exists. Pick the profile on Hosts to connect, or delete it first to change it.".into(),
        Language::Indonesian => "IP/alamat ini sudah ada. Pilih profil di Hosts untuk menyambungkan, atau hapus profil dahulu untuk mengubahnya.".into(),
    }
}

pub fn trust_new_host(lang: Language, host: &str, algorithm: &str, fingerprint: &str) -> String {
    match lang {
        Language::English => format!("New host: {host} ({algorithm})\n{fingerprint}"),
        Language::Indonesian => format!("Host baru: {host} ({algorithm})\n{fingerprint}"),
    }
}

pub fn logical_cpus(lang: Language, cores: &str) -> String {
    match lang {
        Language::English => format!("{cores} logical CPUs"),
        Language::Indonesian => format!("{cores} CPU logis"),
    }
}

pub fn uptime_value(lang: Language, days: u64, hours: u64, minutes: u64) -> String {
    match lang {
        Language::English => format!("{days}d {hours}h {minutes}m"),
        Language::Indonesian => format!("{days} hari {hours} jam {minutes} menit"),
    }
}

pub fn updates_available(lang: Language, count: u32) -> String {
    match lang {
        Language::English => format!("{count} package update(s) available."),
        Language::Indonesian => format!("{count} paket dapat diperbarui."),
    }
}

pub fn firewall_rules_heading(lang: Language, engine: &str) -> String {
    match lang {
        Language::English => format!("{engine} rules · server output"),
        Language::Indonesian => format!("Aturan {engine} · hasil server"),
    }
}

pub fn maintenance_running(lang: Language, task: &str) -> String {
    match lang {
        Language::English => format!("{task} is running. Status reloads once the operation finishes."),
        Language::Indonesian => format!("{task} berjalan. Status akan dimuat ulang setelah operasi selesai."),
    }
}

pub fn inventory_summary(lang: Language, manager: &str, count: usize) -> String {
    match lang {
        Language::English => format!("{manager} · {count} installed package(s) · status from the last check"),
        Language::Indonesian => format!("{manager} · {count} paket terpasang · status dari pemeriksaan terakhir"),
    }
}

pub fn matching_packages(lang: Language, count: usize) -> String {
    match lang {
        Language::English => format!("{count} matching package(s)"),
        Language::Indonesian => format!("{count} paket cocok"),
    }
}

pub fn install_desc_with_candidates(lang: Language, manager: &str, candidates: &str) -> String {
    match lang {
        Language::English => format!("The package is installed from a repository already configured on the server ({manager} · package candidate(s): {candidates}). No extra repository is added automatically."),
        Language::Indonesian => format!("Paket dipasang dari repository yang sudah dikonfigurasi pada server ({manager} · kandidat paket: {candidates}). Repository tambahan tidak dibuat otomatis."),
    }
}

pub fn install_title(lang: Language, app: &str) -> String {
    match lang {
        Language::English => format!("Install {app}?"),
        Language::Indonesian => format!("Install {app}?"),
    }
}

pub fn uninstall_title(lang: Language, package: &str) -> String {
    match lang {
        Language::English => format!("Uninstall {package}?"),
        Language::Indonesian => format!("Uninstall {package}?"),
    }
}

pub fn search_query_note(lang: Language, query: &str) -> String {
    match lang {
        Language::English => format!("Search: \"{query}\""),
        Language::Indonesian => format!("Pencarian: \"{query}\""),
    }
}

pub fn non_utf8_skipped(lang: Language, count: usize) -> String {
    match lang {
        Language::English => format!("{count} non-UTF-8 name(s) skipped."),
        Language::Indonesian => format!("{count} nama non-UTF-8 dilewati."),
    }
}

pub fn entry_detail(lang: Language, permissions: &str, size: &str, modified: &str, target: &str) -> String {
    match lang {
        Language::English => format!("{permissions} · {size}\nModified: {modified}\nTarget: {target}"),
        Language::Indonesian => format!("{permissions} · {size}\nUbah: {modified}\nTarget: {target}"),
    }
}

pub fn discard_draft_desc(lang: Language, path: &str) -> String {
    match lang {
        Language::English => format!("A successful read will replace the current draft. Copy or save the draft first if you still need it.\n{path}"),
        Language::Indonesian => format!("Pembacaan yang berhasil akan mengganti draft saat ini. Salin atau simpan draft terlebih dahulu jika masih diperlukan.\n{path}"),
    }
}

pub fn byte_count(lang: Language, bytes: usize) -> String {
    match lang {
        Language::English => format!("{bytes} bytes"),
        Language::Indonesian => format!("{bytes} byte"),
    }
}

pub fn git_push_desc(lang: Language, host: &str, path: &str) -> String {
    match lang {
        Language::English => format!("Sends the current branch's commits from this repository on {host} to its remote. Anyone using that repository will see them. Output appears in the terminal panel.\n\n{path}"),
        Language::Indonesian => format!("Mengirim commit dari branch aktif repository ini di {host} ke remote-nya. Siapa pun yang memakai repository tersebut akan melihatnya. Output tampil di panel terminal.\n\n{path}"),
    }
}

pub fn compose_setup_desc(lang: Language, host: &str, user: &str, command: &str) -> String {
    match lang {
        Language::English => format!("The SSH account \"{user}\" can't use Docker on {host} yet. This one-time step adds \"{user}\" to the docker group (sudo usermod -aG docker {user}), then runs the command right away — later runs skip this step. If sudo asks for a password, type it in the terminal panel.\n\nNote: members of the docker group effectively have root access to this server.\n\n{command}"),
        Language::Indonesian => format!("Akun SSH \"{user}\" belum bisa memakai Docker di {host}. Langkah sekali saja ini menambahkan \"{user}\" ke grup docker (sudo usermod -aG docker {user}), lalu langsung menjalankan perintahnya — berikutnya langkah ini dilewati. Jika sudo meminta password, ketik di panel terminal.\n\nCatatan: anggota grup docker praktis memiliki akses root ke server ini.\n\n{command}"),
    }
}

pub fn explorer_target_tooltip(lang: Language, folder: &str) -> String {
    match lang {
        Language::English => format!("New files, folders, imports and clones go into:\n{folder}\nClick a folder in the tree to change it; click here to go back to the root."),
        Language::Indonesian => format!("File, folder, impor, dan clone baru masuk ke:\n{folder}\nKlik folder di tree untuk mengganti; klik di sini untuk kembali ke root."),
    }
}

pub fn compose_up_desc(lang: Language, host: &str, command: &str) -> String {
    match lang {
        Language::English => format!("Builds the images and starts the services in the background on {host}. Containers that changed are recreated, which briefly interrupts them. The output appears in the terminal panel.\n\n{command}"),
        Language::Indonesian => format!("Membangun image lalu menjalankan service di background pada {host}. Container yang berubah akan dibuat ulang sehingga sempat terhenti sebentar. Output tampil di panel terminal.\n\n{command}"),
    }
}

pub fn db_sql_rows(lang: Language, count: usize, truncated: bool) -> String {
    match (lang, truncated) {
        (Language::English, false) => format!("{count} row(s)"),
        (Language::English, true) => format!("First {count} rows shown (result was larger)"),
        (Language::Indonesian, false) => format!("{count} baris"),
        (Language::Indonesian, true) => format!("Menampilkan {count} baris pertama (hasil lebih banyak)"),
    }
}

pub fn db_sql_confirm_desc(lang: Language, host: &str, container: &str, db: Option<&str>, destructive: bool, sql: &str) -> String {
    let db = db.unwrap_or("-");
    match lang {
        Language::English => format!(
            "Host: {host}\nContainer: {container} · database: {db}\n{}\n\n{sql}",
            if destructive { "This deletes or alters data or structure and cannot be undone. Make a backup first if unsure." } else { "This changes data or structure in the database." },
        ),
        Language::Indonesian => format!(
            "Host: {host}\nContainer: {container} · database: {db}\n{}\n\n{sql}",
            if destructive { "Query ini menghapus atau mengubah data/struktur dan tidak bisa dibatalkan. Buat backup dulu jika ragu." } else { "Query ini mengubah data atau struktur database." },
        ),
    }
}

pub fn db_browser_range(lang: Language, offset: usize, count: usize) -> String {
    let (first, last) = if count == 0 { (offset, offset) } else { (offset + 1, offset + count) };
    match lang {
        Language::English => format!("Rows {first}–{last}"),
        Language::Indonesian => format!("Baris {first}–{last}"),
    }
}

pub fn fs_chmod_dialog_title(lang: Language, name: &str) -> String {
    match lang {
        Language::English => format!("Permissions: {name}"),
        Language::Indonesian => format!("Izin akses: {name}"),
    }
}

pub fn fs_chmod_was(lang: Language, mode: u32) -> String {
    match lang {
        Language::English => format!("(was {mode:03o})"),
        Language::Indonesian => format!("(sebelumnya {mode:03o})"),
    }
}

pub fn fs_move_picker_title(lang: Language, name: &str) -> String {
    match lang {
        Language::English => format!("Move \"{name}\" to..."),
        Language::Indonesian => format!("Pindahkan \"{name}\" ke..."),
    }
}

pub fn docker_confirm_title(lang: Language, action: &crate::docker::Action) -> String {
    use crate::docker::Action;
    let english = lang == Language::English;
    match action {
        Action::Restart(name) => if english { format!("Are you sure you want to restart {name}?") } else { format!("Yakin ingin me-restart {name}?") },
        Action::Stop(name) => if english { format!("Are you sure you want to stop {name}?") } else { format!("Yakin ingin menghentikan {name}?") },
        Action::ComposeDown(project) => if english { format!("Are you sure you want to take {} down?", project.name) } else { format!("Yakin ingin menjalankan down pada {}?", project.name) },
        Action::ComposeUp { service: Some(service), .. } => if english { format!("Are you sure you want to rebuild {service}?") } else { format!("Yakin ingin me-rebuild {service}?") },
        Action::ComposeUp { project, service: None } => if english { format!("Are you sure you want to rebuild {}?", project.name) } else { format!("Yakin ingin me-rebuild {}?", project.name) },
        _ => if english { "Are you sure?".into() } else { "Apakah Anda yakin?".into() },
    }
}

pub fn docker_confirm_desc(lang: Language, host: &str, action: &crate::docker::Action) -> String {
    use crate::docker::Action;
    let command = action.title();
    let effect = match (lang, action) {
        (Language::English, Action::Stop(_)) => "The container stops and its service stays offline until you start it again.",
        (Language::English, Action::ComposeDown(_)) => "The project's containers and networks are stopped and removed, so the site/service goes offline; named volumes (database data) are kept. Start it again with Rebuild & up.",
        (Language::English, _) => "The service is briefly interrupted while its container restarts or is recreated.",
        (Language::Indonesian, Action::Stop(_)) => "Container dihentikan dan service-nya offline sampai Anda menjalankannya lagi.",
        (Language::Indonesian, Action::ComposeDown(_)) => "Container dan network project dihentikan lalu dihapus sehingga situs/service offline; named volume (data database) tetap disimpan. Jalankan lagi dengan Rebuild & up.",
        (Language::Indonesian, _) => "Service sempat terhenti sebentar selama container di-restart atau dibuat ulang.",
    };
    match lang {
        Language::English => format!("Target host: {host}\n{effect}\n\n{command}"),
        Language::Indonesian => format!("Host tujuan: {host}\n{effect}\n\n{command}"),
    }
}

pub fn docker_usage_total(lang: Language, cpu: f32, capacity: u32, memory: &str) -> String {
    match lang {
        Language::English => format!("Total running: CPU {cpu:.1}% of {capacity}% · RAM {memory}"),
        Language::Indonesian => format!("Total berjalan: CPU {cpu:.1}% dari {capacity}% · RAM {memory}"),
    }
}

pub fn docker_io(lang: Language, net: &str, block: &str) -> String {
    match lang {
        Language::English => format!("Net {net} · Disk {block}"),
        Language::Indonesian => format!("Jaringan {net} · Disk {block}"),
    }
}

pub fn docker_downloading(lang: Language, size: &str) -> String {
    match lang {
        Language::English => format!("Downloading backup... {size}"),
        Language::Indonesian => format!("Mengunduh backup... {size}"),
    }
}

pub fn docker_exec_title(lang: Language, container: &str) -> String {
    match lang {
        Language::English => format!("Run a command in {container}"),
        Language::Indonesian => format!("Jalankan perintah di {container}"),
    }
}

pub fn docker_count(lang: Language, total: usize, running: usize) -> String {
    match lang {
        Language::English => format!("{total} containers · {running} running"),
        Language::Indonesian => format!("{total} container · {running} berjalan"),
    }
}

pub fn docker_reload_tooltip(lang: Language, command: &str) -> String {
    match lang {
        Language::English => format!("Reload config: {command}"),
        Language::Indonesian => format!("Muat ulang konfigurasi: {command}"),
    }
}

pub fn docker_rebuild_service(lang: Language, service: &str) -> String {
    match lang {
        Language::English => format!("Rebuild & deploy: docker compose up -d --build {service}"),
        Language::Indonesian => format!("Rebuild & deploy: docker compose up -d --build {service}"),
    }
}

pub fn git_connected_as(lang: Language, provider: &str, user: &str) -> String {
    match lang {
        Language::English => format!("Connected to {provider} as @{user}"),
        Language::Indonesian => format!("Terhubung ke {provider} sebagai @{user}"),
    }
}

pub fn git_compare_fingerprint(lang: Language, provider: &str) -> String {
    match lang {
        Language::English => format!("Before trusting, check this SHA256 fingerprint matches the one {provider} publishes in its documentation:"),
        Language::Indonesian => format!("Sebelum mempercayai, pastikan fingerprint SHA256 ini sama dengan yang dipublikasikan {provider} di dokumentasinya:"),
    }
}

pub fn git_add_key_hint(lang: Language, provider: crate::git_auth::Provider) -> String {
    use crate::git_auth::Provider;
    match (lang, provider) {
        (Language::English, Provider::GitHub) => "Add it on GitHub under Settings → SSH and GPG keys → New SSH key, with Key type set to \"Authentication Key\" (that is what lets the server clone and pull; \"Signing Key\" only signs commits). For access to one repository only, add it there as a Deploy key instead.".into(),
        (Language::Indonesian, Provider::GitHub) => "Tambahkan di GitHub: Settings → SSH and GPG keys → New SSH key, dengan Key type \"Authentication Key\" (jenis ini yang membuat server bisa clone dan pull; \"Signing Key\" hanya untuk menandatangani commit). Untuk akses ke satu repository saja, tambahkan sebagai Deploy key di repository tersebut.".into(),
        (Language::English, Provider::GitLab) => "Add it on GitLab under Preferences → SSH Keys. For access to one project only, add it there as a Deploy key instead.".into(),
        (Language::Indonesian, Provider::GitLab) => "Tambahkan di GitLab: Preferences → SSH Keys. Untuk akses ke satu project saja, tambahkan sebagai Deploy key di project tersebut.".into(),
    }
}

pub fn git_denied(lang: Language, provider: &str) -> String {
    match lang {
        Language::English => format!("{provider} rejected this server's key. Add the public key above to your account, then test again."),
        Language::Indonesian => format!("{provider} menolak key server ini. Tambahkan public key di atas ke akun Anda, lalu uji lagi."),
    }
}

pub fn fs_delete_desc(lang: Language, path: &str, is_dir: bool) -> String {
    match (lang, is_dir) {
        (Language::English, true) => format!("This cannot be undone. The folder is deleted along with everything inside it.\n{path}"),
        (Language::English, false) => format!("This cannot be undone.\n{path}"),
        (Language::Indonesian, true) => format!("Tindakan ini tidak dapat dibatalkan. Folder akan dihapus beserta seluruh isinya.\n{path}"),
        (Language::Indonesian, false) => format!("Tindakan ini tidak dapat dibatalkan.\n{path}"),
    }
}

pub fn fs_success_message(lang: Language, reply: &crate::remote_fs::Reply) -> String {
    use crate::remote_fs::Reply;
    match (lang, reply) {
        (Language::English, Reply::Created { path }) => format!("Created: {path}"),
        (Language::Indonesian, Reply::Created { path }) => format!("Dibuat: {path}"),
        (Language::English, Reply::Deleted { path }) => format!("Deleted: {path}"),
        (Language::Indonesian, Reply::Deleted { path }) => format!("Dihapus: {path}"),
        (Language::English, Reply::Renamed { from, to }) => format!("Renamed/moved: {from} → {to}"),
        (Language::Indonesian, Reply::Renamed { from, to }) => format!("Diganti nama/dipindahkan: {from} → {to}"),
        (Language::English, Reply::Copied { to }) => format!("Copied to: {to}"),
        (Language::Indonesian, Reply::Copied { to }) => format!("Disalin ke: {to}"),
        (Language::English, Reply::Uploaded { path }) => format!("Uploaded: {path}"),
        (Language::Indonesian, Reply::Uploaded { path }) => format!("Diunggah: {path}"),
        (Language::English, Reply::Chmoded { path, mode, recursive }) => format!("Permissions set to {mode:03o}{}: {path}", if *recursive { " (including contents)" } else { "" }),
        (Language::Indonesian, Reply::Chmoded { path, mode, recursive }) => format!("Izin diubah ke {mode:03o}{}: {path}", if *recursive { " (termasuk isinya)" } else { "" }),
    }
}

pub fn import_started(lang: Language, total: usize, skipped: usize) -> String {
    match (lang, skipped) {
        (Language::English, 0) => format!("Importing {total} item(s)..."),
        (Language::English, skipped) => format!("Importing {total} item(s)... ({skipped} local entr{suffix} skipped)", suffix = if skipped == 1 { "y" } else { "ies" }),
        (Language::Indonesian, 0) => format!("Mengimpor {total} item..."),
        (Language::Indonesian, skipped) => format!("Mengimpor {total} item... ({skipped} entri lokal dilewati)"),
    }
}

pub fn import_summary(lang: Language, done: usize, failed: usize) -> String {
    match (lang, failed) {
        (Language::English, 0) => format!("Import finished: {done} item(s)."),
        (Language::English, failed) => format!("Import finished: {done} item(s), {failed} failed."),
        (Language::Indonesian, 0) => format!("Impor selesai: {done} item."),
        (Language::Indonesian, failed) => format!("Impor selesai: {done} item, {failed} gagal."),
    }
}

pub fn power_confirm_desc(lang: Language, host: &str, user: &str, address: &str, port: u16) -> String {
    format!("{host} · {user}@{address}:{port}. {}", strings(lang).confirm_power_desc_tail)
}

pub fn power_action_notice(lang: Language, reboot: bool) -> String {
    match (lang, reboot) {
        (Language::English, true) => "Server is rebooting. This can take a few minutes — reconnect from Hosts once it's back.".into(),
        (Language::English, false) => "Server is shutting down. Reconnect from Hosts once it's back online.".into(),
        (Language::Indonesian, true) => "Server sedang reboot. Ini bisa memakan beberapa menit — sambungkan ulang dari Hosts setelah server kembali aktif.".into(),
        (Language::Indonesian, false) => "Server sedang dimatikan. Sambungkan ulang dari Hosts setelah server aktif kembali.".into(),
    }
}

pub fn update_confirm_desc(lang: Language, host: &str, user: &str, address: &str, port: u16) -> String {
    match lang {
        Language::English => format!(
            "{host} · {user}@{address}:{port}. APT: apt-get update && apt-get upgrade -y. DNF: dnf upgrade --refresh -y. Services may restart during the update; there is no distro upgrade or automatic reboot.",
        ),
        Language::Indonesian => format!(
            "{host} · {user}@{address}:{port}. APT: apt-get update && apt-get upgrade -y. DNF: dnf upgrade --refresh -y. Layanan dapat restart selama pembaruan; tidak ada upgrade versi distribusi atau reboot otomatis.",
        ),
    }
}

pub fn firewall_confirm_desc(lang: Language, action: &str, port: u16, protocol: &str, engine: &str) -> String {
    match lang {
        Language::English => format!("{action} {port}/{protocol} via {engine}. UFW adds an allow/deny rule; firewalld changes the runtime port on the default zone (not persistent across reload/reboot)."),
        Language::Indonesian => format!("{action} {port}/{protocol} lewat {engine}. UFW menambah allow/deny; firewalld mengubah port runtime pada default zone (tidak persisten setelah reload/reboot)."),
    }
}

pub fn firewall_delete_rule_desc(lang: Language, action: &str, to: &str) -> String {
    match lang {
        Language::English => format!("Remove the UFW rule \"{action} {to}\". This deletes that specific rule only; other rules are untouched."),
        Language::Indonesian => format!("Hapus aturan UFW \"{action} {to}\". Hanya aturan ini yang dihapus; aturan lain tidak terpengaruh."),
    }
}

pub fn firewall_toggle_title(lang: Language, engine: &str, enable: bool) -> String {
    match (lang, enable) {
        (Language::English, true) => format!("Enable {engine}?"),
        (Language::English, false) => format!("Disable {engine}?"),
        (Language::Indonesian, true) => format!("Aktifkan {engine}?"),
        (Language::Indonesian, false) => format!("Nonaktifkan {engine}?"),
    }
}

pub fn firewall_toggle_ok(lang: Language, enable: bool) -> &'static str {
    match (lang, enable) {
        (Language::English, true) => "Enable",
        (Language::English, false) => "Disable",
        (Language::Indonesian, true) => "Aktifkan",
        (Language::Indonesian, false) => "Nonaktifkan",
    }
}

pub fn firewall_toggle_warning(lang: Language, enable: bool) -> &'static str {
    match (lang, enable) {
        (Language::English, true) => "WARNING: if the SSH port isn't already allowed, enabling the firewall can lock you out of this connection.",
        (Language::English, false) => "WARNING: disabling the firewall removes all of its protection immediately.",
        (Language::Indonesian, true) => "PERINGATAN: jika port SSH belum diizinkan, mengaktifkan firewall dapat mengunci koneksi ini.",
        (Language::Indonesian, false) => "PERINGATAN: menonaktifkan firewall langsung menghilangkan seluruh perlindungannya.",
    }
}

pub fn firewall_reload_desc(lang: Language, engine: &str) -> String {
    match lang {
        Language::English => format!("Re-reads {engine}'s saved rules and re-applies them. Running rules stay the same; this just syncs the active firewall with what's on disk."),
        Language::Indonesian => format!("Membaca ulang aturan {engine} yang tersimpan dan menerapkannya kembali. Aturan yang berjalan tidak berubah; ini hanya menyelaraskan firewall aktif dengan yang tersimpan di disk."),
    }
}
