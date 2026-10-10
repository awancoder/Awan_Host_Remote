#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Summary,
    Firewall,
    Terminal,
    Editor,
    Docker,
    /// Read-only browser for a database container; opened from the Docker
    /// page (it needs a container to browse), so it is not in [`Self::ALL`].
    DbBrowser,
    Tools,
    Hosts,
    /// Global app preferences, not a per-host workspace page — reachable
    /// from the sidebar footer regardless of connection state, so it is
    /// deliberately excluded from [`Self::ALL`].
    Settings,
}

impl Page {
    /// Sidebar order under a connected host: Summary, Terminal, File Editor,
    /// Docker, Firewall, Applications. Databases are browsed from the Docker
    /// page (Database Browser).
    pub const ALL: [Self; 7] = [
        Self::Summary, Self::Terminal, Self::Editor, Self::Docker, Self::Firewall,
        Self::Tools, Self::Hosts,
    ];

    /// Page names are short, mostly-technical labels (Summary, Firewall,
    /// Terminal...) that read the same in both languages, so they stay plain
    /// literals here rather than duplicated in the i18n catalog. Settings is
    /// the one exception, since it has a natural Indonesian equivalent.
    pub fn title(self, lang: crate::i18n::Language) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Firewall => "Firewall",
            Self::Terminal => "Terminal",
            Self::Editor => "File Editor",
            Self::Docker => "Docker",
            Self::DbBrowser => "Database Browser",
            Self::Tools => "Applications",
            Self::Hosts => "Hosts",
            Self::Settings => crate::i18n::strings(lang).settings_title,
        }
    }

    pub fn subtitle(self, lang: crate::i18n::Language) -> &'static str {
        match self {
            Self::Summary => if lang == crate::i18n::Language::English { "Every piece of server information in one place." } else { "Semua informasi server dalam satu tempat." },
            Self::Firewall => if lang == crate::i18n::Language::English { "Check UFW/firewalld status and manage access rules." } else { "Periksa status UFW/firewalld dan kelola aturan akses." },
            Self::Terminal => if lang == crate::i18n::Language::English { "Command-line workspace for the active host." } else { "Ruang kerja command line untuk host aktif." },
            Self::Editor => if lang == crate::i18n::Language::English { "Browse folders, open files, and edit text on the SSH server." } else { "Jelajahi folder, buka file, dan edit teks pada server SSH." },
            Self::Docker => if lang == crate::i18n::Language::English { "Containers and Compose projects on the server: logs, restart, rebuild, exec, and backups." } else { "Container dan project Compose di server: log, restart, rebuild, exec, dan backup." },
            Self::DbBrowser => if lang == crate::i18n::Language::English { "Browse a database container's databases, tables, and rows, and run SQL." } else { "Jelajahi database, tabel, dan baris di container database, serta jalankan SQL." },
            Self::Tools => if lang == crate::i18n::Language::English { "Install, check, and uninstall applications on the SSH server." } else { "Install, periksa, dan uninstall aplikasi pada server SSH." },
            Self::Hosts => if lang == crate::i18n::Language::English { "One workspace for all of your servers." } else { "Satu ruang kerja untuk seluruh server Anda." },
            Self::Settings => if lang == crate::i18n::Language::English { "App preferences." } else { "Preferensi aplikasi." },
        }
    }
}

#[derive(Clone)]
pub struct Host {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub user: String,
    pub key_auth: bool,
    pub saved: bool,
}

pub struct HostForm {
    pub name: String,
    pub address: String,
    pub port: String,
    pub user: String,
    pub key_auth: bool,
}

impl Default for HostForm {
    fn default() -> Self {
        Self {
            name: String::new(), address: String::new(), port: "22".into(),
            user: "ubuntu".into(), key_auth: true,
        }
    }
}

impl HostForm {
    pub fn host(&self) -> Result<Host, String> {
        if self.name.trim().is_empty() || self.address.trim().is_empty() || self.user.trim().is_empty() {
            return Err("Nama, alamat server, dan username wajib diisi.".into());
        }
        if self.address.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, ',' | '|' | '/' | '\\')) {
            return Err("Alamat server harus berupa IP atau hostname tanpa spasi dan karakter pemisah.".into());
        }
        let port = self.port.trim().parse::<u16>()
            .ok().filter(|port| *port > 0)
            .ok_or_else(|| "Port harus berupa angka 1 sampai 65535.".to_owned())?;
        let address = crate::host_store::address_key(&self.address);
        if address.is_empty() || address.len() > 253 {
            return Err("Alamat IP atau hostname tidak valid / terlalu panjang.".into());
        }
        Ok(Host {
            name: self.name.trim().to_owned(), address,
            user: self.user.trim().to_owned(), port, key_auth: self.key_auth, saved: false,
        })
    }
}

