use eframe::egui::{self, RichText};

use crate::app::AwanApp;
use crate::model::Page;
use crate::theme::{self, ACCENT, AMBER, MUTED};

fn section_title(ui: &mut egui::Ui, title: &str, note: &str) {
    ui.label(RichText::new(title).size(17.0).strong());
    theme::muted(ui, note);
    ui.add_space(6.0);
}

impl AwanApp {
    pub fn terminal_view(&mut self, ui: &mut egui::Ui) {
        let Some(host) = self.active.and_then(|index| self.hosts.get(index)).cloned() else {
            section_title(ui, "Pilih host terlebih dahulu", "Tambahkan profil SSH atau pilih host yang sudah tersedia.");
            ui.horizontal(|ui| {
                if theme::primary(ui, "+ Add New Host").clicked() { self.add_host(); }
                if ui.button("Buka Hosts").clicked() { self.page = Page::Hosts; }
            });
            return;
        };
        theme::card().inner_margin(12).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{}@{}:{}", host.user, host.address, host.port)).monospace());
                theme::badge(ui, &self.terminal.status, if self.terminal.connected { ACCENT } else { AMBER });
                if self.terminal.busy() && ui.button("Putuskan / batalkan").clicked() { self.terminal.disconnect(); }
            });
            if !self.terminal.busy() {
                if host.saved && ui.add_enabled(!self.host_store.busy, egui::Button::new("Sambungkan dengan login tersimpan").fill(theme::ACCENT_BG)).clicked() {
                    self.terminal.connect_saved(&host, ui.ctx());
                }
                theme::muted(ui, "Login manual untuk koneksi pertama atau memperbarui kredensial:");
                if host.key_auth {
                    theme::field(ui, "Path private key (absolut)", &mut self.terminal.key_path, "C:\\Users\\nama\\.ssh\\id_ed25519");
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(if host.key_auth { "Passphrase (opsional)" } else { "Password SSH" });
                    ui.add(egui::TextEdit::singleline(&mut self.terminal.secret).password(true).desired_width(230.0));
                    if ui.add_enabled(!self.host_store.busy, egui::Button::new("Sambungkan SSH").fill(theme::ACCENT_BG)).clicked() { self.terminal.connect(&host, ui.ctx()); }
                });
                ui.add_enabled_ui(cfg!(windows), |ui| {
                    ui.checkbox(&mut self.terminal.remember_login, "Simpan login setelah berhasil (Windows Credential Manager)");
                });
                theme::muted(ui, "SSH key: file private key tetap di lokasi asal. Login tersimpan tetap memverifikasi host key server.");
            }
        });
        if let Some(error) = self.terminal.error.clone() {
            egui::ScrollArea::vertical().id_salt("ssh_error").max_height(70.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(theme::RED, error);
                    if ui.small_button("Tutup pesan").clicked() { self.terminal.error = None; }
                });
            });
        }
        let allow_input = !self.show_host_form && !self.confirm_update && self.confirm_package.is_none() && self.delete_host.is_none() && self.terminal.trust.is_none() && !self.editor.modal_open();
        self.terminal.view(ui, allow_input);
    }

    pub fn tools_view(&mut self, ui: &mut egui::Ui) {
        use crate::packages::{Maintenance, Software};
        ui.horizontal_wrapped(|ui| {
            theme::badge(ui, "LIVE · SSH", ACCENT);
            if ui.add_enabled(self.terminal.can_maintain(), egui::Button::new("Muat ulang status")).clicked() {
                self.terminal.refresh_packages();
            }
            if ui.button(if self.terminal.update_running { "Lihat progres →" } else { "Buka Terminal →" }).clicked() {
                self.page = Page::Terminal;
            }
        });
        if !self.terminal.connected {
            theme::muted(ui, "Hubungkan SSH ke server asli untuk memeriksa dan mengelola aplikasi. Status belum diketahui.");
            ui.add_space(10.0);
            for app in Software::ALL {
                ui.horizontal(|ui| {
                    ui.label(app.name());
                    theme::badge(ui, "Unknown", MUTED);
                    ui.add_enabled(false, egui::Button::new("Install"));
                });
            }
            return;
        }
        if self.terminal.update_running {
            ui.colored_label(AMBER, format!("{} berjalan. Status akan dimuat ulang setelah operasi selesai.", self.terminal.task_title));
        }
        if self.terminal.inventory_scanning { theme::muted(ui, "Memeriksa paket yang terpasang di server..."); }
        if let Some(error) = &self.terminal.package_error { ui.colored_label(theme::RED, error); }
        if let Some(result) = &self.terminal.update_result { ui.label(result); }
        let can_manage = self.terminal.can_maintain();
        let Some(inventory) = &self.terminal.inventory else { return; };
        theme::muted(ui, format!("{} · {} paket terpasang · status dari pemeriksaan terakhir", inventory.manager, inventory.packages.len()));
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.package_tab, 0, "Katalog aplikasi");
            ui.selectable_value(&mut self.package_tab, 1, "Semua paket terpasang");
        });
        let mut chosen = None;
        if self.package_tab == 0 {
            for app in Software::ALL {
                let installed = app.installed(inventory);
                let manual = installed.is_empty() && inventory.binaries.iter().any(|id| id == app.id());
                ui.push_id(app.id(), |ui| {
                    theme::card().show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(app.name()).size(18.0).strong());
                            theme::badge(ui, if !installed.is_empty() { "Installed" } else if manual { "Terdeteksi di PATH" } else { "Not Installed" },
                                if !installed.is_empty() { ACCENT } else if manual { AMBER } else { MUTED });
                            if installed.is_empty() && ui.add_enabled(can_manage && !manual, egui::Button::new("Install")).clicked() {
                                chosen = Some(Maintenance::Install(app));
                            }
                        });
                        theme::muted(ui, app.description());
                        for package in installed {
                            ui.horizontal_wrapped(|ui| {
                                ui.monospace(format!("{} · {}", package.name, package.version));
                                if ui.add_enabled(can_manage, egui::Button::new("Uninstall")).clicked() {
                                    chosen = Some(Maintenance::Remove(package.name.clone()));
                                }
                            });
                        }
                        if manual { theme::muted(ui, "Executable tersedia, tetapi paket katalog tidak ditemukan. Periksa asal instalasi sebelum mengubahnya."); }
                    });
                });
            }
            theme::muted(ui, "Installed menunjukkan paket terpasang; tidak menjamin layanan sedang berjalan atau sudah dikonfigurasi.");
        } else {
            ui.add(egui::TextEdit::singleline(&mut self.package_search).hint_text("Cari nama paket atau versi...").desired_width(340.0));
            theme::muted(ui, "Daftar ini mencakup library dan paket sistem. Uninstall dapat memengaruhi layanan yang bergantung padanya.");
            let needle = self.package_search.to_lowercase();
            let indices = inventory.packages.iter().enumerate().filter(|(_, package)| {
                package.name.to_lowercase().contains(&needle) || package.version.to_lowercase().contains(&needle)
            }).map(|(index, _)| index).collect::<Vec<_>>();
            theme::muted(ui, format!("{} paket cocok", indices.len()));
            let width = ui.available_width();
            egui::ScrollArea::vertical().id_salt("installed_packages").max_height(420.0)
                .show_rows(ui, 34.0, indices.len(), |ui, range| {
                    for row in range {
                        let package = &inventory.packages[indices[row]];
                        ui.push_id(indices[row], |ui| {
                            ui.horizontal(|ui| {
                                ui.add_sized([width * 0.42, 30.0], egui::Label::new(&package.name).truncate()).on_hover_text(&package.name);
                                ui.add_sized([width * 0.32, 30.0], egui::Label::new(RichText::new(&package.version).monospace()).truncate()).on_hover_text(&package.version);
                                if ui.add_enabled(can_manage, egui::Button::new("Uninstall")).clicked() {
                                    chosen = Some(Maintenance::Remove(package.name.clone()));
                                }
                            });
                        });
                    }
                });
        }
        if let Some(action) = chosen { self.confirm_package = Some(action); }
    }

    pub fn hosts_view(&mut self, ui: &mut egui::Ui) {
        theme::heading(ui, "Hosts", Page::Hosts.subtitle());
        theme::muted(ui, "Satu profil per IP. Login yang berhasil disimpan tersedia lagi setelah aplikasi ditutup; pilih Sambungkan untuk masuk kembali.");
        if self.host_store.busy { ui.spinner(); theme::muted(ui, "Memuat atau memperbarui penyimpanan profil..."); }
        ui.add_space(12.0);
        if self.hosts.is_empty() {
            theme::card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                section_title(ui, "Belum ada host", "Tambahkan profil server SSH pertama Anda.");
                ui.horizontal(|ui| {
                    if theme::primary(ui, "+  Add New Host").clicked() { self.add_host(); }

                });
            });
            return;
        }
        let mut selected = None;
        let mut removed = None;
        let mut manual = None;
        for (index, host) in self.hosts.iter().enumerate() {
            ui.push_id(index, |ui| {
                theme::card().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&host.name).size(18.0).strong());
                            ui.label(RichText::new(format!("{}@{}:{}", host.user, host.address, host.port)).monospace().color(MUTED));
                            theme::muted(ui, if host.key_auth { "SSH key" } else { "Password" });
                            theme::badge(ui, if host.saved { "LOGIN TERSIMPAN" } else { "BELUM DISIMPAN" }, if host.saved { ACCENT } else { AMBER });
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let available = !self.host_store.busy && !self.terminal.file_busy && !self.terminal.update_running;
                            if ui.add_enabled(available && !(self.terminal.busy() && !self.terminal.connected), egui::Button::new("Hapus")).clicked() { removed = Some(index); }
                            if ui.add_enabled(available, egui::Button::new("Login manual")).clicked() { manual = Some(index); }
                            if ui.add_enabled(available, egui::Button::new(if host.saved { "Sambungkan →" } else { "Isi login →" })).clicked() { selected = Some(index); }
                            if self.active == Some(index) { theme::badge(ui, "DIPILIH", ACCENT); }
                        });
                    });
                });
            });
            ui.add_space(8.0);
        }
        if let Some(index) = removed {
            self.delete_host = Some(self.hosts[index].address.clone());
        } else if let Some(index) = selected {
            self.open_saved_host(index, ui.ctx());
        } else if let Some(index) = manual {
            self.activate(index);
            self.page = Page::Terminal;
        }
        ui.add_space(12.0);
        ui.separator();
        ui.label(RichText::new("Semua data berasal dari host SSH aktif. Akses yang tidak tersedia ditampilkan sebagai error.").color(MUTED));
    }
}
