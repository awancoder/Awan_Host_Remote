use std::time::Duration;
use eframe::egui::{self, RichText};
use zeroize::Zeroizing;
use crate::{app::AwanApp, live::{Action, Kind, Request}, model::Page, packages::Maintenance, theme};

pub struct Database {
    engine: usize,
    host: String,
    port: String,
    user: String,
    password: Zeroizing<String>,
    db: String,
    schema: String,
    table: String,
    offset: usize,
}
impl Default for Database {
    fn default() -> Self {
        Self { engine: 0, host: "127.0.0.1".into(), port: "5432".into(), user: String::new(),
            password: Zeroizing::new(String::new()), db: "postgres".into(), schema: "public".into(), table: String::new(), offset: 0 }
    }
}
impl Database {
    pub fn clear_password(&mut self) { self.password = Zeroizing::new(String::new()); }
}

fn table(ui: &mut egui::Ui, id: &str, rows: &[Vec<String>]) {
    if rows.is_empty() { theme::muted(ui, "Tidak ada hasil."); return; }
    egui::ScrollArea::both().id_salt(id).max_height(440.0).show_rows(ui, 30.0, rows.len(), |ui, range| {
        for index in range {
            ui.horizontal(|ui| {
                let row = &rows[index];
                for cell in row {
                    let preview: String = cell.chars().take(512).collect();
                    let text = if index == 0 { RichText::new(preview).strong() } else { RichText::new(preview).monospace() };
                    ui.add_sized([160.0, 24.0], egui::Label::new(text).truncate()).on_hover_text(cell.chars().take(4096).collect::<String>());
                }
            });
        }
    });
}

impl AwanApp {
    pub fn poll_live_page(&mut self, ctx: &egui::Context) {
        let kind = match self.page { Page::Summary => Kind::Summary, Page::Control => Kind::Services, Page::Ports => Kind::Ports, _ => return };
        if !self.terminal.connected { return; }
        let interval = Duration::from_secs(if kind == Kind::Summary { 5 } else { 15 });
        let last = self.terminal.live.checked[kind.index()];
        if last.is_none_or(|last| last.elapsed() >= interval) {
            self.terminal.request_live(Request::new(kind));
        }
        // Poll only the active monitoring page. No repaint loop on idle pages.
        ctx.request_repaint_after(last.map(|last| interval.saturating_sub(last.elapsed())).unwrap_or(interval).max(Duration::from_secs(1)));
    }

    fn live_header(&mut self, ui: &mut egui::Ui, kind: Kind) -> bool {
        let connected = self.terminal.connected;
        ui.horizontal_wrapped(|ui| {
            theme::badge(ui, if connected { "LIVE · SSH" } else { "OFFLINE" }, if connected { theme::ACCENT } else { theme::MUTED });
            if kind != Kind::Database && ui.add_enabled(connected && self.terminal.live.busy.is_none() && !self.terminal.update_running,
                egui::Button::new("Muat ulang")).clicked() { self.terminal.request_live(Request::new(kind)); }
            if ui.button("Buka Terminal →").clicked() { self.page = Page::Terminal; }
            if let Some(time) = self.terminal.live.checked[kind.index()] {
                theme::muted(ui, format!("Pemeriksaan terakhir {} detik lalu", time.elapsed().as_secs()));
            }
        });
        if !connected { theme::muted(ui, "Hubungkan SSH untuk membaca data server. Tidak ada data contoh."); return false; }
        if self.terminal.live.busy == Some(kind) { ui.spinner(); theme::muted(ui, "Membaca server..."); }
        if let Some(error) = &self.terminal.live.errors[kind.index()] { ui.colored_label(theme::RED, error); }
        true
    }

    pub fn summary_view(&mut self, ui: &mut egui::Ui) {
        if !self.live_header(ui, Kind::Summary) { return; }
        let Some(report) = &self.terminal.live.data[Kind::Summary.index()] else {
            theme::muted(ui, "Metrik belum tersedia. Pembacaan memerlukan Python 3 dan /proc pada server Linux."); return;
        };
        let memory = capacity(report.value("memory_used"), report.value("memory_total"));
        let disk = capacity(report.value("disk_used"), report.value("disk_total"));
        let cpu = report.value("cpu").parse::<f32>().ok().map(|v| format!("{v:.1}%")).unwrap_or_else(|| "Tidak tersedia".into());
        ui.columns(3, |columns| {
            theme::metric(&mut columns[0], "CPU USAGE", &cpu, &format!("{} CPU logis · sampel 250 ms", report.value("cores")), &self.terminal.live.cpu, theme::ACCENT);
            theme::metric(&mut columns[1], "MEMORY", &memory, "MemTotal − MemAvailable", &[], theme::BLUE);
            theme::metric(&mut columns[2], "STORAGE /", &disk, "Filesystem root server", &[], theme::AMBER);
        });
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            egui::Grid::new("live_system").spacing([24.0, 12.0]).show(ui, |ui| {
                for (label, value) in [("Hostname", report.value("hostname")), ("OS", report.value("os")), ("Kernel", report.value("kernel"))] {
                    theme::muted(ui, label); ui.label(value); ui.end_row();
                }
                theme::muted(ui, "Uptime");
                ui.label(report.value("uptime").parse::<u64>().ok().map(|s| format!("{} hari {} jam {} menit", s / 86400, s / 3600 % 24, s / 60 % 60)).unwrap_or_else(|| "Tidak tersedia".into()));
                ui.end_row();
                theme::muted(ui, "SSH"); ui.label("Terhubung ke host aktif"); ui.end_row();
            });
        });
        for row in &report.rows {
            if row.first().is_some_and(|key| key.ends_with("_error")) {
                ui.colored_label(theme::AMBER, row.join(": "));
            }
        }
        theme::muted(ui, "Grafik CPU hanya berisi sampel yang berhasil dibaca selama sesi ini. Pembaruan setiap 5 detik saat Summary terbuka.");
        ui.horizontal_wrapped(|ui| {
            for page in [Page::Control, Page::Ports, Page::Databases, Page::Terminal] {
                if ui.button(page.title()).clicked() { self.page = page; }
            }
        });
    }

    pub fn control_view(&mut self, ui: &mut egui::Ui) {
        if !self.live_header(ui, Kind::Services) { return; }
        let enabled = self.terminal.can_maintain();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(enabled, egui::Button::new("Update System")).clicked() { self.confirm_update = true; }
            for (label, reboot) in [("Reboot server", true), ("Shutdown server", false)] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() { self.confirm_package = Some(Maintenance::Server(Action::Power { reboot })); }
            }
        });
        if let Some(result) = &self.terminal.update_result { ui.label(result); }
        ui.add(egui::TextEdit::singleline(&mut self.service_search).hint_text("Cari unit atau deskripsi layanan..."));
        theme::muted(ui, "Unit service yang dimuat systemd, termasuk yang tidak aktif. Start/stop/restart dijalankan pada server.");
        let Some(report) = &self.terminal.live.data[Kind::Services.index()] else { return; };
        let needle = self.service_search.to_lowercase();
        let rows: Vec<_> = report.rows.iter().skip(1).filter(|row| row.len() == 5 && row.join(" ").to_lowercase().contains(&needle)).collect();
        egui::ScrollArea::vertical().id_salt("live_services").max_height(480.0).show_rows(ui, 72.0, rows.len(), |ui, range| {
            for index in range {
                let row = rows[index];
                ui.push_id(&row[0], |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.monospace(&row[0]);
                        theme::badge(ui, &format!("{} / {}", row[2], row[3]), if row[2] == "active" { theme::ACCENT } else { theme::AMBER });
                        for verb in ["start", "stop", "restart"] {
                            if ui.add_enabled(enabled, egui::Button::new(verb)).clicked() {
                                self.confirm_package = Some(Maintenance::Server(Action::Service { unit: row[0].clone(), verb: verb.into() }));
                            }
                        }
                    });
                    theme::muted(ui, &row[4]);
                });
            }
        });
        theme::muted(ui, format!("{} unit cocok", rows.len()));
    }

    pub fn ports_view(&mut self, ui: &mut egui::Ui) {
        if !self.live_header(ui, Kind::Ports) { return; }
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.port_search).hint_text("Cari port, alamat, atau proses..."));
            for (index, label) in ["Semua", "TCP", "UDP"].iter().enumerate() { ui.selectable_value(&mut self.protocol, index, *label); }
        });
        if let Some(report) = &self.terminal.live.data[Kind::Ports.index()] {
            let needle = self.port_search.to_lowercase();
            let mut rows = vec![vec!["Protokol".into(), "State".into(), "Alamat lokal".into(), "Proses".into()]];
            for row in &report.rows {
                if row.first().is_some_and(|key| key == "socket") && row.len() == 5
                    && row.join(" ").to_lowercase().contains(&needle)
                    && (self.protocol == 0 || row[1] == if self.protocol == 1 { "TCP" } else { "UDP" }) {
                    rows.push(row[1..].to_vec());
                }
            }
            table(ui, "live_ports", &rows);
            for row in &report.rows {
                if row.first().is_some_and(|key| key == "socket_error" || key == "firewall_error") {
                    ui.colored_label(theme::AMBER, row[1..].join(": "));
                } else if row.first().is_some_and(|key| key == "firewall") && row.len() == 3 {
                    egui::CollapsingHeader::new(format!("Aturan {} · hasil server", row[1])).default_open(true).show(ui, |ui| {
                        egui::ScrollArea::both().id_salt(&row[1]).max_height(220.0).show(ui, |ui| { ui.monospace(&row[2]); });
                    });
                }
            }
        }
        ui.separator();
        ui.label(RichText::new("Kelola aturan port").strong());
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("firewall_engine").selected_text(&self.firewall_engine).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.firewall_engine, "ufw".into(), "UFW");
                ui.selectable_value(&mut self.firewall_engine, "firewalld".into(), "firewalld (runtime)");
            });
            ui.add(egui::TextEdit::singleline(&mut self.firewall_port).hint_text("Port").desired_width(80.0));
            ui.checkbox(&mut self.firewall_udp, "UDP (default TCP)");
            let port = self.firewall_port.parse::<u16>().ok().filter(|port| *port != 0);
            let detected = self.terminal.live.data[Kind::Ports.index()].as_ref().is_some_and(|report|
                report.rows.iter().any(|row| row.len() == 3 && row[0] == "firewall" && row[1] == self.firewall_engine));
            for (label, open) in [("Tambah izin port", true), (if self.firewall_engine == "ufw" { "Tambah deny" } else { "Hapus izin port" }, false)] {
                if ui.add_enabled(detected && port.is_some() && self.terminal.can_maintain(), egui::Button::new(label)).clicked() {
                    self.confirm_package = Some(Maintenance::Server(Action::Firewall { engine: self.firewall_engine.clone(), port: port.unwrap_or(0), protocol: if self.firewall_udp { "udp" } else { "tcp" }.into(), open }));
                }
            }
        });
        theme::muted(ui, "Listening tidak berarti diizinkan firewall. Aturan cloud/security group tidak termasuk. UFW menambah allow/deny; firewalld menambah/menghapus port pada default zone untuk runtime. Tinjau aturan lengkap untuk konflik atau akses lewat service.");
    }

    fn query_database(&mut self, mode: &str) {
        if self.terminal.live.busy.is_some() { return; }
        let db = &self.db;
        let fields = vec![if db.engine == 0 { "postgres" } else { "mysql" }.into(), db.host.clone(), db.port.clone(),
            db.user.clone(), db.password.to_string(), db.db.clone(), mode.into(), db.schema.clone(), db.table.clone(), db.offset.to_string()];
        self.terminal.live.data[Kind::Database.index()] = None;
        self.terminal.request_live(Request { kind: Kind::Database, fields: Zeroizing::new(fields) });
    }

    pub fn database_view(&mut self, ui: &mut egui::Ui) {
        if !self.live_header(ui, Kind::Database) { return; }
        theme::badge(ui, "READ ONLY · VIA SSH", theme::ACCENT);
        theme::muted(ui, "Client psql (12+) atau mysql/mariadb dijalankan pada server SSH. Host database dilihat dari server tersebut. Password hanya berada di memori sesi; kosongkan untuk autentikasi lokal yang sudah tersedia.");
        let enabled = self.terminal.live.busy.is_none() && !self.terminal.update_running;
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal_wrapped(|ui| {
                let previous = self.db.engine;
                ui.selectable_value(&mut self.db.engine, 0, "PostgreSQL");
                ui.selectable_value(&mut self.db.engine, 1, "MySQL / MariaDB");
                if previous != self.db.engine {
                    self.db = Database { engine: self.db.engine, port: if self.db.engine == 0 { "5432" } else { "3306" }.into(), db: if self.db.engine == 0 { "postgres" } else { "" }.into(), ..Database::default() };
                    self.terminal.live.data[Kind::Database.index()] = None;
                }
            });
            let mut changed = false;
            egui::Grid::new("db_connection").num_columns(2).show(ui, |ui| {
                ui.label("Host (atau socket directory PostgreSQL)"); changed |= ui.text_edit_singleline(&mut self.db.host).changed(); ui.end_row();
                ui.label("Port"); changed |= ui.text_edit_singleline(&mut self.db.port).changed(); ui.end_row();
                ui.label("Username"); changed |= ui.text_edit_singleline(&mut self.db.user).changed(); ui.end_row();
                ui.label("Password"); changed |= ui.add(egui::TextEdit::singleline(&mut *self.db.password).password(true)).changed(); ui.end_row();
                ui.label("Database"); changed |= ui.text_edit_singleline(&mut self.db.db).changed(); ui.end_row();
                ui.label("Schema (PostgreSQL)"); changed |= ui.text_edit_singleline(&mut self.db.schema).changed(); ui.end_row();
                ui.label("Table"); changed |= ui.text_edit_singleline(&mut self.db.table).changed(); ui.end_row();
            });
            if changed {
                self.terminal.live.data[Kind::Database.index()] = None;
                self.terminal.live.errors[Kind::Database.index()] = None;
                self.terminal.live.checked[Kind::Database.index()] = None;
                self.db.offset = 0;
            }
            ui.horizontal_wrapped(|ui| {
                for (label, mode) in [("Daftar database", "databases"), ("Daftar tabel", "tables"), ("Struktur tabel", "structure"), ("Index", "indexes"), ("Lihat data", "data")] {
                    if ui.button(label).clicked() { self.db.offset = 0; self.query_database(mode); }
                }
            });
        });
        let mut selected_db = None;
        let mut selected_table = None;
        let mut next_offset = None;
        if let Some(report) = &self.terminal.live.data[Kind::Database.index()] {
            if let Some(context) = report.rows.first().filter(|row| row.len() == 6 && row[0] == "context") {
                theme::muted(ui, format!("Hasil: {} · database {} · {}.{} · offset {}", context[1], context[2], context[3], context[4], context[5]));
                table(ui, "database_result", &report.rows[1..]);
                if context[1] == "databases" || context[1] == "tables" {
                    theme::muted(ui, "Pilih objek untuk membuka:");
                    egui::ScrollArea::vertical().id_salt("database_objects").max_height(160.0).show(ui, |ui| {
                        for row in report.rows.iter().skip(2) {
                            if context[1] == "databases" && !row.is_empty() {
                                if ui.add_enabled(enabled, egui::Button::new(&row[0])).clicked() { selected_db = Some(row[0].clone()); }
                            } else if row.len() >= 2 && ui.add_enabled(enabled, egui::Button::new(format!("{}.{}", row[0], row[1]))).clicked() {
                                selected_table = Some((row[0].clone(), row[1].clone()));
                            }
                        }
                    });
                }
                if context[1] == "data" {
                    let offset = context[5].parse::<usize>().unwrap_or(0);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(enabled && offset >= 100, egui::Button::new("← Sebelumnya")).clicked() { next_offset = Some(offset - 100); }
                        if ui.add_enabled(enabled && report.rows.len() >= 102 && offset < 1_000_000, egui::Button::new("Berikutnya →")).clicked() { next_offset = Some(offset + 100); }
                    });
                    theme::muted(ui, "Maksimal 100 baris per halaman. Urutan mengikuti database; perubahan data selama browsing dapat menggeser halaman.");
                }
            }
        }
        if let Some(name) = selected_db { self.db.db = name; self.db.offset = 0; self.query_database("tables"); }
        if let Some((schema, table)) = selected_table { self.db.schema = schema; self.db.table = table; self.db.offset = 0; self.query_database("data"); }
        if let Some(offset) = next_offset { self.db.offset = offset; self.query_database("data"); }
        theme::muted(ui, "Daftar database/tabel dibatasi 1.000 objek, struktur 256 kolom. Editor SQL bebas belum tersedia; semua tombol memakai query baca yang ditentukan aplikasi.");
        theme::muted(ui, "Sel panjang ditampilkan sebagai pratinjau 512 karakter; arahkan pointer untuk melihat hingga 4.096 karakter.");
    }
}

fn capacity(used: &str, total: &str) -> String {
    match (used.parse::<f64>(), total.parse::<f64>()) {
        (Ok(used), Ok(total)) if total > 0.0 => format!("{:.1} / {:.1} GiB", used / 1_073_741_824.0, total / 1_073_741_824.0),
        _ => "Tidak tersedia".into(),
    }
}
