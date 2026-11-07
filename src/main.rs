#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod docker;
mod docker_access;
mod docker_db;
mod git_auth;
mod i18n;
mod model;
mod host_store;
mod packages;
mod remote_file;
mod remote_directory;
mod remote_fs;
mod live;
mod ssh;
mod stalwart;
mod system_update;
mod terminal;
mod terminal_widget;

// Not yet ported (plan: migrasi GUI ke gpui-kit, langkah 5-8). Kept on disk
// as reference for porting Summary/Control/Ports/Databases (live_views.rs),
// File Editor (editor.rs), the folder browser (explorer.rs), and the old
// Hosts/Terminal-connect/Applications view bodies (views.rs, saved_hosts.rs)
// now mostly superseded by app.rs.
// mod editor;
// mod explorer;
// mod live_views;
// mod saved_hosts;
// mod views;

use gpui_kit::AppContext as _;

struct AppAssets;

impl gpui_kit::AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        match path {
            "flags/id.svg" => Ok(Some(std::borrow::Cow::Borrowed(i18n::FLAG_ID_SVG.as_bytes()))),
            "flags/en.svg" | "flags/gb.svg" => Ok(Some(std::borrow::Cow::Borrowed(i18n::FLAG_EN_SVG.as_bytes()))),
            _ => gpui_kit::assets::AllAssets.load(path),
        }
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<gpui_kit::SharedString>> {
        let mut list = gpui_kit::assets::AllAssets.list(path)?;
        list.push("flags/id.svg".into());
        list.push("flags/en.svg".into());
        Ok(list)
    }
}

fn main() {
    gpui_kit::application()
        // Embedded Lucide catalog augmented with country flag SVGs.
        .with_assets(AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let window_options = gpui_kit::WindowOptions {
                window_bounds: Some(gpui_kit::WindowBounds::centered(
                    gpui_kit::size(gpui_kit::px(1280.0), gpui_kit::px(820.0)),
                    cx,
                )),
                // Wayland app_id / X11 WM_CLASS; must match StartupWMClass in
                // packaging/linux/awan-host-manager.desktop for the dock icon.
                app_id: Some("awan-host-manager".into()),
                ..Default::default()
            };
            gpui_kit::open_window(window_options, cx, |window, cx| {
                window.set_window_title("Awan Host Manager");
                cx.new(|cx| app::AwanApp::new(window, cx))
            })
            .expect("Failed to open window");
        });
}
