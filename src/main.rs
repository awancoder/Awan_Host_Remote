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

fn main() {
    gpui_kit::application()
        // The File Editor's explorer/status icons (Wifi, Terminal, FileCode,
        // Save, ...) live outside gpui-component's small curated icon subset
        // (`Assets`), so the full bundled Lucide catalog is registered here.
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let window_options = gpui_kit::WindowOptions {
                window_bounds: Some(gpui_kit::WindowBounds::centered(
                    gpui_kit::size(gpui_kit::px(1280.0), gpui_kit::px(820.0)),
                    cx,
                )),
                // Wayland app_id / X11 WM_CLASS; must match StartupWMClass in
                // packaging/linux/awan-host-remote.desktop for the dock icon.
                app_id: Some("awan-host-remote".into()),
                ..Default::default()
            };
            gpui_kit::open_window(window_options, cx, |window, cx| {
                window.set_window_title("Awan Host Remote");
                cx.new(|cx| app::AwanApp::new(window, cx))
            })
            .expect("Failed to open window");
        });
}
