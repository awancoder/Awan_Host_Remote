use eframe::egui;
use crate::{app::AwanApp, host_store::Event, model::Page, theme};

impl AwanApp {
    pub fn poll_host_store(&mut self) {
        while let Some(event) = self.host_store.poll() {
            match event {
                Event::Listed(Ok((hosts, skipped))) => {
                    for host in hosts {
                        if !self.hosts.iter().any(|existing| existing.address == host.address) { self.hosts.push(host); }
                    }
                    if !self.hosts.is_empty() && self.active.is_none() && self.page == Page::Summary && !self.show_host_form {
                        self.page = Page::Hosts;
                    }
                    if skipped > 0 { self.host_store_message = Some(format!("{skipped} profil tersimpan tidak dapat dibaca dan dilewati. Entri tersebut tidak dihapus otomatis.")); }
                }
                Event::Listed(Err(error)) => self.host_store_message = Some(error),
                Event::Deleted(address, Ok(())) => {
                    self.remove_host_from_list(&address);
                    self.host_store_message = Some(format!("Profil dan login tersimpan {address} dihapus."));
                }
                Event::Deleted(_, Err(error)) => self.host_store_message = Some(error),
            }
        }
    }

    fn remove_host_from_list(&mut self, address: &str) {
        let Some(index) = self.hosts.iter().position(|host| host.address == address) else { return; };
        if self.active == Some(index) {
            self.terminal = crate::terminal::Terminal::default();
            self.db = crate::live_views::Database::default();
        }
        self.hosts.remove(index);
        self.active = match self.active {
            Some(active) if active == index => None,
            Some(active) if active > index => Some(active - 1),
            other => other,
        };
    }

    pub fn open_saved_host(&mut self, index: usize, ctx: &egui::Context) {
        if self.host_store.busy || self.terminal.file_busy || self.terminal.update_running { return; }
        self.activate(index);
        self.page = Page::Terminal;
        if self.hosts[index].saved && !self.terminal.busy() {
            self.terminal.connect_saved(&self.hosts[index], ctx);
        }
    }

    pub fn delete_host_dialog(&mut self, ctx: &egui::Context) {
        let Some(address) = self.delete_host.clone() else { return; };
        let Some(host) = self.hosts.iter().find(|host| host.address == address).cloned() else {
            self.delete_host = None;
            return;
        };
        egui::Modal::new(egui::Id::new("delete_saved_host")).show(ctx, |ui| {
            ui.set_width(430.0);
            ui.heading("Hapus profil SSH?");
            ui.label(format!("{} · {}@{}:{}", host.name, host.user, host.address, host.port));
            ui.label("Profil beserta login yang disimpan aplikasi akan dihapus. Sesi host ini ditutup jika aktif. File private key dan server tidak dihapus.");
            ui.horizontal(|ui| {
                if ui.button("Batal").clicked() { self.delete_host = None; }
                let allowed = !self.host_store.busy && !self.terminal.file_busy && !self.terminal.update_running
                    && !(self.terminal.busy() && !self.terminal.connected);
                if ui.add_enabled(allowed, egui::Button::new("Hapus profil & login").fill(theme::ACCENT_BG)).clicked() {
                    self.delete_host = None;
                    if host.saved {
                        match self.host_store.delete(address.clone()) {
                            Ok(()) => {
                                if self.active.and_then(|index| self.hosts.get(index)).is_some_and(|active| active.address == address) {
                                    self.terminal = crate::terminal::Terminal::default();
                                    self.db = crate::live_views::Database::default();
                                }
                            }
                            Err(error) => self.host_store_message = Some(error.to_string()),
                        }
                    } else {
                        self.remove_host_from_list(&address);
                        self.host_store_message = Some("Profil yang belum disimpan telah dihapus dari daftar.".into());
                    }
                }
            });
        });
    }
}
