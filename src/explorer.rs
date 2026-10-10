use eframe::egui::{self, RichText};
use crate::{app::AwanApp, remote_directory::{Directory, Entry}, remote_file::Request, theme};

#[derive(Clone, Copy, Default)]
pub enum Navigation { #[default] Visit, History(usize), Refresh }

#[derive(Default)]
pub struct Explorer {
    pub owner: String,
    pub initialized: bool,
    pub error: Option<String>,
    directory: Option<Directory>,
    address: String,
    query: String,
    show_hidden: bool,
    selected: Option<String>,
    history: Vec<String>,
    position: usize,
    pending: Navigation,
}
impl Explorer {
    pub fn complete(&mut self, directory: Directory) {
        match self.pending {
            Navigation::History(index) if index < self.history.len() => {
                self.position = index;
                self.history[index] = directory.path.clone();
            }
            Navigation::Refresh if !self.history.is_empty() => {
                self.history[self.position] = directory.path.clone();
            }
            _ => {
                if self.history.get(self.position) != Some(&directory.path) {
                    self.history.truncate(if self.history.is_empty() { 0 } else { self.position + 1 });
                    self.history.push(directory.path.clone());
                    if self.history.len() > 64 { self.history.remove(0); }
                    self.position = self.history.len() - 1;
                }
            }
        }
        self.address = directory.path.clone();
        self.query = directory.query.clone();
        self.show_hidden = directory.show_hidden;
        self.selected = None;
        self.directory = Some(directory);
        self.error = None;
    }
}

pub fn child_path(folder: &str, name: &str) -> String { format!("{}/{}", folder.trim_end_matches('/'), name) }
pub(crate) fn parent_path(path: &str) -> String {
    path.trim_end_matches('/').rsplit_once('/').map(|(parent, _)| if parent.is_empty() { "/" } else { parent }).unwrap_or("/").into()
}
fn size_label(entry: &Entry) -> String {
    if entry.directory() { return "Folder".into(); }
    match entry.size {
        Some(size) if size >= 1024 * 1024 => format!("{:.1} MiB", size as f64 / 1_048_576.0),
        Some(size) if size >= 1024 => format!("{:.1} KiB", size as f64 / 1024.0),
        Some(size) => format!("{size} B"),
        None => "Tidak tersedia".into(),
    }
}

impl AwanApp {
    pub(crate) fn browse_directory(&mut self, path: String, navigation: Navigation, clear_query: bool) {
        if !self.terminal.connected || self.terminal.file_busy { return; }
        let browser = &mut self.editor.browser;
        let query = if clear_query { String::new() } else { browser.query.clone() };
        browser.initialized = true;
        match self.terminal.request_file(Request::List { path, query, show_hidden: browser.show_hidden }) {
            Ok(()) => {
                browser.pending = navigation;
                browser.error = None;
                self.editor.browsing = true;
            }
            Err(error) => browser.error = Some(error),
        }
    }

    pub(crate) fn explorer_view(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Explorer").size(18.0).strong());
        if !self.terminal.connected {
            theme::muted(ui, "Hubungkan SSH untuk menjelajahi folder server.");
            return;
        }
        let enabled = !self.terminal.file_busy;
        let mut navigate: Option<(String, Navigation, bool)> = None;
        let mut open_file = None;
        let browser = &mut self.editor.browser;
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(browser.position > 0, egui::Button::new("←")).on_hover_text("Folder sebelumnya").clicked() {
                    navigate = Some((browser.history[browser.position - 1].clone(), Navigation::History(browser.position - 1), true));
                }
                if ui.add_enabled(browser.position + 1 < browser.history.len(), egui::Button::new("→")).on_hover_text("Folder berikutnya").clicked() {
                    navigate = Some((browser.history[browser.position + 1].clone(), Navigation::History(browser.position + 1), true));
                }
                if ui.add_enabled(browser.directory.as_ref().is_some_and(|dir| dir.path != "/"), egui::Button::new("↑")).on_hover_text("Naik satu folder").clicked() {
                    if let Some(directory) = &browser.directory { navigate = Some((parent_path(&directory.path), Navigation::Visit, true)); }
                }
                if ui.button("Refresh").clicked() {
                    navigate = Some((browser.directory.as_ref().map(|dir| dir.path.clone()).unwrap_or_else(|| "~".into()), Navigation::Refresh, false));
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Home").clicked() {
                    navigate = Some((browser.directory.as_ref().map(|dir| dir.home.clone()).unwrap_or_else(|| "~".into()), Navigation::Visit, true));
                }
                if ui.button("Root /").clicked() { navigate = Some(("/".into(), Navigation::Visit, true)); }
            });
            ui.horizontal(|ui| {
                let input = ui.add(egui::TextEdit::singleline(&mut browser.address).hint_text("Path folder / atau ~")
                    .desired_width((ui.available_width() - 75.0).max(100.0)));
                if (input.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))) || ui.button("Buka").clicked() {
                    navigate = Some((browser.address.clone(), Navigation::Visit, true));
                }
            });
            if let Some(directory) = &browser.directory {
                egui::ScrollArea::horizontal().id_salt("explorer_breadcrumbs").max_height(34.0).show(ui, |ui| { ui.horizontal(|ui| {
                    if ui.small_button("/").clicked() { navigate = Some(("/".into(), Navigation::Visit, true)); }
                    let mut path = String::new();
                    for name in directory.path.split('/').filter(|part| !part.is_empty()) {
                        path.push('/'); path.push_str(name);
                        ui.label("›");
                        if ui.small_button(name).clicked() { navigate = Some((path.clone(), Navigation::Visit, true)); }
                    }
                }); });
            }
            ui.horizontal(|ui| {
                let input = ui.add(egui::TextEdit::singleline(&mut browser.query).hint_text("Cari nama di folder ini...")
                    .desired_width((ui.available_width() - 65.0).max(100.0)));
                if (input.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))) || ui.button("Cari").clicked() {
                    navigate = Some((browser.directory.as_ref().map(|dir| dir.path.clone()).unwrap_or_else(|| "~".into()), Navigation::Refresh, false));
                }
            });
            if ui.checkbox(&mut browser.show_hidden, "Tampilkan file tersembunyi").changed() {
                navigate = Some((browser.directory.as_ref().map(|dir| dir.path.clone()).unwrap_or_else(|| "~".into()), Navigation::Refresh, false));
            }
        });
        if self.editor.browsing { ui.spinner(); theme::muted(ui, "Memuat folder..."); }
        if let Some(error) = &browser.error { ui.colored_label(theme::RED, error); }
        if let Some(directory) = &browser.directory {
            theme::muted(ui, format!("{} entri{}", directory.entries.len(), if directory.query.is_empty() { String::new() } else { format!(" · pencarian: {}", directory.query) }));
            if directory.truncated { ui.colored_label(theme::AMBER, "Daftar dibatasi 2.000 entri / 1 MiB. Cari nama yang lebih spesifik untuk menampilkan entri lain."); }
            if directory.skipped > 0 { theme::muted(ui, format!("{} nama non-UTF-8 dilewati.", directory.skipped)); }
            if directory.entries.is_empty() { theme::muted(ui, "Folder kosong atau tidak ada nama yang cocok dengan filter."); }
            let height = (ui.available_height() - 120.0).max(100.0);
            egui::ScrollArea::vertical().id_salt("remote_folder_entries").max_height(height)
                .show_rows(ui, 49.0, directory.entries.len(), |ui, range| {
                    for index in range {
                        let entry = &directory.entries[index];
                        ui.push_id(&entry.name, |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            let icon = if entry.directory() { "▸" } else if entry.kind == "link" { "↗" } else { "≡" };
                            let text = RichText::new(format!("{icon} {}", entry.name.escape_debug()))
                                .color(if entry.directory() { theme::ACCENT } else { theme::TEXT });
                            let response = ui.add_enabled(enabled, egui::Button::new((text, egui::Atom::grow()))
                                .selected(browser.selected.as_deref() == Some(entry.name.as_str()))
                                .truncate()
                                .min_size(egui::vec2(ui.available_width(), 24.0)))
                                .on_hover_text(format!("{}\n{} · {}\n{}\n{}", entry.name, entry.permissions, size_label(entry), entry.modified, entry.target));
                            if response.clicked() { browser.selected = Some(entry.name.clone()); }
                            if entry.openable() && (response.double_clicked() || (response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)))) {
                                let path = child_path(&directory.path, &entry.name);
                                if entry.directory() { navigate = Some((path, Navigation::Visit, true)); }
                                else { open_file = Some(path); }
                            }
                            ui.label(RichText::new(format!("{} · {}", size_label(entry), entry.kind)).size(11.0).color(theme::MUTED));
                        });
                    }
                });
            if let Some(entry) = directory.entries.iter().find(|entry| Some(entry.name.as_str()) == browser.selected.as_deref()) {
                ui.separator();
                theme::muted(ui, format!("{} · {}", entry.permissions, entry.modified));
                if ui.add_enabled(enabled && entry.openable(), egui::Button::new(if entry.directory() { "Buka folder →" } else { "Edit file →" })).clicked() {
                    let path = child_path(&directory.path, &entry.name);
                    if entry.directory() { navigate = Some((path, Navigation::Visit, true)); }
                    else { open_file = Some(path); }
                }
                if !entry.openable() { theme::muted(ui, "Editor hanya membuka file biasa maksimal 1 MiB. Symlink file, file khusus, atau nama berkarakter kontrol tidak dapat diedit dari sini."); }
            }
        }
        if let Some((path, navigation, clear)) = navigate { self.browse_directory(path, navigation, clear); }
        else if let Some(path) = open_file { self.select_remote_file(path); }
    }
}
