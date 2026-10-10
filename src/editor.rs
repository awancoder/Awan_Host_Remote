use eframe::egui::{self, RichText};
use crate::{app::AwanApp, model::Page, remote_file::{Reply, Request, MAX_BYTES}, theme};

#[derive(Default)]
pub struct Editor {
    path_input: String,
    path: String,
    host: String,
    pending_host: String,
    text: String,
    original: String,
    revision: Option<String>,
    message: Option<String>,
    failed: bool,
    saving: bool,
    discard_open: Option<String>,
    pub(crate) browser: crate::explorer::Explorer,
    pub(crate) browsing: bool,
    pub confirm_exit: bool,
    pub allow_exit: bool,
}

impl Editor {
    pub fn dirty(&self) -> bool { self.text != self.original }
    pub fn modal_open(&self) -> bool { self.discard_open.is_some() || self.confirm_exit }
    pub fn clear_explorer(&mut self) {
        self.browser = crate::explorer::Explorer::default();
        self.browsing = false;
    }

    pub fn finish(&mut self, result: Result<Reply, String>) {
        match result {
            Ok(Reply::Directory(directory)) => self.browser.complete(directory),
            Ok(Reply::Read { path, revision, text }) => {
                self.path = path.clone();
                self.path_input = path;
                self.host = self.pending_host.clone();
                self.original = text.clone();
                self.text = text;
                self.revision = Some(revision);
                self.message = Some("File berhasil dibuka dari server.".into());
                self.failed = false;
            }
            Ok(Reply::Saved { revision }) => {
                self.original = self.text.clone();
                self.revision = Some(revision);
                self.message = Some("File berhasil disimpan ke server.".into());
                self.failed = false;
            }
            Err(error) => {
                if self.browsing {
                    self.browser.error = Some(error);
                } else {
                    if self.saving { self.revision = None; }
                    self.message = Some(error);
                    self.failed = true;
                }
            }
        }
        self.saving = false;
        self.browsing = false;
    }
}

impl AwanApp {
    fn editor_host(&self) -> Option<String> {
        self.active.and_then(|index| self.hosts.get(index))
            .map(|host| format!("{}@[{}]:{}", host.user, host.address, host.port))
    }

    fn open_remote_file(&mut self, path: String) {
        let Some(host) = self.editor_host() else { return; };
        match self.terminal.request_file(Request::Read { path }) {
            Ok(()) => {
                self.editor.pending_host = host;
                self.editor.saving = false;
                self.editor.browsing = false;
                self.editor.message = None;
            }
            Err(error) => { self.editor.message = Some(error); self.editor.failed = true; }
        }
    }

    pub(crate) fn select_remote_file(&mut self, path: String) {
        if self.terminal.file_busy { return; }
        if self.editor.dirty() { self.editor.discard_open = Some(path); }
        else { self.open_remote_file(path); }
    }

    pub fn editor_view(&mut self, ui: &mut egui::Ui) {
        theme::heading(ui, Page::Editor.title(), Page::Editor.subtitle());
        let host = self.editor_host();
        let online = self.terminal.connected && host.is_some();
        ui.horizontal_wrapped(|ui| {
            theme::badge(ui, if online { "LIVE · SSH" } else { "OFFLINE" }, if online { theme::ACCENT } else { theme::MUTED });
            if let Some(host) = &host { ui.monospace(host); }
            if ui.button("Buka Terminal →").clicked() { self.page = Page::Terminal; }
        });
        theme::muted(ui, "File teks UTF-8 · maksimal 1 MiB · Python 3 diperlukan di server · izin akun SSH tanpa sudo.");
        if online {
            let owner = host.clone().unwrap_or_default();
            if self.editor.browser.owner != owner {
                self.editor.clear_explorer();
                self.editor.browser.owner = owner;
            }
            if !self.editor.browser.initialized && !self.terminal.file_busy {
                self.browse_directory("~".into(), crate::explorer::Navigation::Visit, true);
            }
        } else {
            self.editor.clear_explorer();
        }
        let explorer_max = (ui.available_width() * 0.48).clamp(240.0, 460.0);
        egui::SidePanel::left("file_explorer_panel").resizable(true).default_width(310.0).width_range(230.0..=explorer_max)
            .show_inside(ui, |ui| { self.explorer_view(ui); });
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 0)))
            .show_inside(ui, |ui| { self.editor_document(ui, online, host.as_deref()); });
    }

    fn editor_document(&mut self, ui: &mut egui::Ui, online: bool, host: Option<&str>) {
        let busy = self.terminal.file_busy;
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled(!busy, egui::TextEdit::singleline(&mut self.editor.path_input)
                .hint_text("Path file untuk dibuka langsung...").desired_width((ui.available_width() - 160.0).max(120.0)));
            if ui.add_enabled(online && !busy, egui::Button::new("Buka / muat ulang")).clicked() {
                let path = self.editor.path_input.clone();
                self.select_remote_file(path);
            }
        });
        if busy && !self.editor.browsing { ui.spinner(); theme::muted(ui, if self.editor.saving { "Menyimpan file ke server..." } else { "Membaca file dari server..." }); }
        if let Some(message) = &self.editor.message {
            ui.colored_label(if self.editor.failed { theme::RED } else { theme::ACCENT }, message);
        }
        if self.editor.path.is_empty() {
            ui.add_space(32.0);
            ui.heading("Pilih file untuk diedit");
            theme::muted(ui, "Klik dua kali folder untuk masuk. Klik dua kali file teks di penjelajah sebelah kiri untuk membukanya di sini. Anda juga dapat memasukkan path file langsung di atas.");
            return;
        }
        let same_host = host == Some(self.editor.host.as_str());
        ui.label(RichText::new(format!("{} · {}", self.editor.host, self.editor.path)).monospace());
        if !same_host { ui.colored_label(theme::AMBER, "Draft milik host lain. Sambungkan kembali host tersebut untuk menyimpan."); }
        if ui.add_enabled(online && same_host && !busy, egui::Button::new("Tampilkan di folder")).clicked() {
            self.browse_directory(crate::explorer::parent_path(&self.editor.path), crate::explorer::Navigation::Visit, true);
        }
        ui.horizontal_wrapped(|ui| {
            let dirty = self.editor.dirty();
            theme::badge(ui, if dirty { "BELUM DISIMPAN" } else { "TIDAK ADA PERUBAHAN" }, if dirty { theme::AMBER } else { theme::MUTED });
            let can_save = online && same_host && !busy && dirty && self.editor.revision.is_some() && self.editor.text.len() <= MAX_BYTES;
            if ui.add_enabled(can_save, egui::Button::new("Simpan ke server").fill(theme::ACCENT_BG)).clicked() {
                let request = Request::Save { path: self.editor.path.clone(),
                    revision: self.editor.revision.clone().unwrap_or_default(), text: self.editor.text.clone() };
                match self.terminal.request_file(request) {
                    Ok(()) => { self.editor.saving = true; self.editor.message = None; }
                    Err(error) => { self.editor.message = Some(error); self.editor.failed = true; }
                }
            }
            if ui.button("Salin draft").clicked() { ui.ctx().copy_text(self.editor.text.clone()); }
            theme::muted(ui, format!("{} byte · {} baris", self.editor.text.len(), self.editor.text.bytes().filter(|c| *c == b'\n').count() + 1));
        });
        if self.editor.revision.is_none() {
            ui.colored_label(theme::AMBER, "Salin draft sebelum memuat ulang untuk memeriksa file server. Penyimpanan ulang dinonaktifkan sampai file dibuka kembali.");
        }
        if self.editor.text.len() > MAX_BYTES { ui.colored_label(theme::AMBER, "Konten melebihi 1 MiB. Kurangi teks sebelum menyimpan."); }
        let height = (ui.available_height() - 28.0).max(120.0);
        egui::ScrollArea::both().id_salt("remote_editor_scroll").max_height(height).show(ui, |ui| {
            ui.add_enabled(!self.terminal.file_busy, egui::TextEdit::multiline(&mut self.editor.text)
                .id(egui::Id::new("remote_file_text")).font(egui::TextStyle::Monospace).code_editor()
                .char_limit(MAX_BYTES).desired_width(f32::INFINITY).desired_rows(24));
        });
        theme::muted(ui, "Draft hanya disimpan di memori aplikasi. Simpan sebelum menutup aplikasi.");
    }

    pub fn editor_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.editor.discard_open.clone() {
            egui::Modal::new(egui::Id::new("discard_editor_draft")).show(ctx, |ui| {
                ui.set_width(440.0);
                ui.heading("Ganti draft yang belum disimpan?");
                ui.label("Pembacaan yang berhasil akan mengganti draft saat ini. Salin atau simpan draft terlebih dahulu jika masih diperlukan.");
                ui.monospace(&path);
                ui.horizontal(|ui| {
                    if ui.button("Batal").clicked() { self.editor.discard_open = None; }
                    if ui.add_enabled(self.terminal.connected && !self.terminal.file_busy, egui::Button::new("Buang draft & buka")).clicked() {
                        self.editor.discard_open = None;
                        self.open_remote_file(path.clone());
                    }
                });
            });
        }
        if self.editor.confirm_exit {
            egui::Modal::new(egui::Id::new("close_editor_app")).show(ctx, |ui| {
                ui.set_width(440.0);
                ui.heading("Tutup aplikasi?");
                ui.label("Ada draft belum disimpan atau operasi file masih berlangsung. Menutup aplikasi dapat menghilangkan draft; hasil penyimpanan yang terputus perlu diperiksa di server.");
                ui.horizontal(|ui| {
                    if ui.button("Kembali ke editor").clicked() { self.editor.confirm_exit = false; self.page = Page::Editor; }
                    if ui.button("Tetap tutup").clicked() {
                        self.editor.allow_exit = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        }
    }
}
