use std::sync::RwLock;
use tauri::menu::{Menu, MenuItem, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

pub struct LibraryStats {
    pub total_files: usize,
    pub total_size_bytes: u64,
}

#[derive(Clone)]
pub struct TrayMenuItems {
    status: MenuItem<tauri::Wry>,
    library: MenuItem<tauri::Wry>,
    open_dashboard: MenuItem<tauri::Wry>,
    start_server: MenuItem<tauri::Wry>,
    stop_server: MenuItem<tauri::Wry>,
    trigger_scan: MenuItem<tauri::Wry>,
    reset_db: MenuItem<tauri::Wry>,
}

pub struct AppState {
    pub service: crate::service::ServiceState,
    pub stats: RwLock<LibraryStats>,
    pub tray_menu: RwLock<Option<TrayMenuItems>>,
}

pub fn get_configured_port() -> u16 {
    let (config_path, _, _) = crate::service::get_vuio_paths();
    if let Ok(content) = std::fs::read_to_string(config_path) {
        if let Ok(value) = crate::parse_settings_document(&content) {
            if let Some(server) = value.get("server") {
                if let Some(port) = server.get("port") {
                    if let Some(p) = port.as_integer() {
                        return p as u16;
                    }
                }
            }
        }
    }
    8080 // Default port
}

fn format_size(bytes: u64) -> String {
    let kb = bytes as f64 / 1024.0;
    let mb = kb / 1024.0;
    let gb = mb / 1024.0;
    if gb >= 1.0 {
        format!("{:.2} GB", gb)
    } else if mb >= 1.0 {
        format!("{:.2} MB", mb)
    } else {
        format!("{:.2} KB", kb)
    }
}

struct TrayMenuState {
    status_text: String,
    library_text: String,
    is_running: bool,
}

fn tray_menu_state(
    is_running: bool,
    port: u16,
    total_files: usize,
    total_size_bytes: u64,
) -> TrayMenuState {
    let status_text = if is_running {
        format!("Status: Running (Port {port})")
    } else {
        "Status: Stopped".to_string()
    };
    let library_text = if is_running {
        format!(
            "Library: {total_files} files ({})",
            format_size(total_size_bytes)
        )
    } else {
        "Library: Offline".to_string()
    };

    TrayMenuState {
        status_text,
        library_text,
        is_running,
    }
}

fn current_tray_menu_state(app: &tauri::AppHandle) -> TrayMenuState {
    let state = app.state::<AppState>();
    let is_running = state.service.is_running();
    let port = get_configured_port();
    let stats = state.stats.read().unwrap();
    tray_menu_state(is_running, port, stats.total_files, stats.total_size_bytes)
}

fn build_menu(app: &tauri::AppHandle) -> Result<(Menu<tauri::Wry>, TrayMenuItems), tauri::Error> {
    let menu_state = current_tray_menu_state(app);

    let status_item = MenuItemBuilder::new(&menu_state.status_text)
        .id("status")
        .enabled(false)
        .build(app)?;

    let library_item = MenuItemBuilder::new(&menu_state.library_text)
        .id("library")
        .enabled(false)
        .build(app)?;

    let open_item = MenuItemBuilder::new("Open Dashboard")
        .id("open_dashboard")
        .enabled(menu_state.is_running)
        .build(app)?;

    let start_item = MenuItemBuilder::new("Start Server")
        .id("start_server")
        .enabled(!menu_state.is_running)
        .build(app)?;

    let stop_item = MenuItemBuilder::new("Stop Server")
        .id("stop_server")
        .enabled(menu_state.is_running)
        .build(app)?;

    let scan_item = MenuItemBuilder::new("Trigger Media Scan")
        .id("trigger_scan")
        .enabled(menu_state.is_running)
        .build(app)?;

    let reset_item = MenuItemBuilder::new("Reset Database...")
        .id("reset_db")
        .enabled(menu_state.is_running)
        .build(app)?;

    let settings_item = MenuItemBuilder::new("Settings...")
        .id("settings")
        .build(app)?;

    let quit_item = MenuItemBuilder::new("Quit").id("quit").build(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &library_item,
            &PredefinedMenuItem::separator(app)?,
            &open_item,
            &start_item,
            &stop_item,
            &scan_item,
            &reset_item,
            &settings_item,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;
    let items = TrayMenuItems {
        status: status_item,
        library: library_item,
        open_dashboard: open_item,
        start_server: start_item,
        stop_server: stop_item,
        trigger_scan: scan_item,
        reset_db: reset_item,
    };

    Ok((menu, items))
}

pub fn update_tray_menu(app: &tauri::AppHandle) {
    let menu_state = current_tray_menu_state(app);
    let items = app.state::<AppState>().tray_menu.read().unwrap().clone();
    let Some(items) = items else {
        return;
    };

    let updates = [
        items.status.set_text(menu_state.status_text),
        items.library.set_text(menu_state.library_text),
        items.open_dashboard.set_enabled(menu_state.is_running),
        items.start_server.set_enabled(!menu_state.is_running),
        items.stop_server.set_enabled(menu_state.is_running),
        items.trigger_scan.set_enabled(menu_state.is_running),
        items.reset_db.set_enabled(menu_state.is_running),
    ];
    for result in updates {
        if let Err(error) = result {
            eprintln!("Failed to update tray menu item: {error}");
        }
    }
}

pub fn open_settings_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let was_visible = window.is_visible().unwrap_or(false);
        let _ = window.show();
        let _ = window.set_focus();
        if !was_visible {
            let _ = window.emit("settings-opened", ());
        }
    }
}

pub fn start_stats_updater(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

            let state = app.state::<AppState>();
            if !state.service.is_running() {
                continue;
            }

            let port = get_configured_port();
            let url = format!("http://127.0.0.1:{}/metrics/json", port);

            let client = reqwest::Client::new();
            if let Ok(resp) = client.get(&url).send().await {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    if let Some(db_stats) = val.get("database_stats") {
                        let total_files = db_stats
                            .get("total_files")
                            .and_then(|f| f.as_u64())
                            .unwrap_or(0) as usize;
                        let total_size_bytes = db_stats
                            .get("total_size_bytes")
                            .and_then(|s| s.as_u64())
                            .unwrap_or(0);

                        {
                            let mut stats = state.stats.write().unwrap();
                            stats.total_files = total_files;
                            stats.total_size_bytes = total_size_bytes;
                        }

                        update_tray_menu(&app);
                    }
                }
            }
        }
    });
}

pub fn create_tray(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    let (menu, items) = build_menu(app)?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().unwrap())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open_dashboard" => {
                let port = get_configured_port();
                let url = format!("http://localhost:{}", port);
                let _ = app.opener().open_url(url, None::<String>);
            }
            "start_server" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    if let Err(error) = state.service.start().await {
                        eprintln!("Failed to start server: {error}");
                    }
                    update_tray_menu(&app);
                });
            }
            "stop_server" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    if let Err(error) = state.service.stop().await {
                        eprintln!("Failed to stop server: {error}");
                    }
                    update_tray_menu(&app);
                });
            }
            "trigger_scan" => {
                let port = get_configured_port();
                tauri::async_runtime::spawn(async move {
                    let (config_path, _, _) = crate::service::get_vuio_paths();
                    if config_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&config_path) {
                            let _ = std::fs::write(&config_path, content);
                        }
                    }
                    let client = reqwest::Client::new();
                    let _ = client.post(format!("http://127.0.0.1:{}/api/scan", port)).send().await;
                });
            }
            "reset_db" => {
                use tauri_plugin_dialog::DialogExt;
                let app_clone = app.clone();
                app.dialog()
                    .message("Are you sure you want to reset the database? This will clear all indexed media files and require a complete scan.")
                    .title("Reset Database")
                    .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
                    .buttons(tauri_plugin_dialog::MessageDialogButtons::YesNo)
                    .show(move |confirmed| {
                        if confirmed {
                            tauri::async_runtime::spawn(async move {
                                let state = app_clone.state::<AppState>();
                                let _ = state.service.stop().await;
                                let (_, db_path, _) = crate::service::get_vuio_paths();
                                if db_path.exists() {
                                    let _ = std::fs::remove_file(db_path);
                                }
                                let _ = state.service.start().await;
                                {
                                    let mut stats = state.stats.write().unwrap();
                                    stats.total_files = 0;
                                    stats.total_size_bytes = 0;
                                }
                                update_tray_menu(&app_clone);
                            });
                        }
                    });
            }
            "settings" => {
                open_settings_window(app);
            }
            "quit" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    let _ = state.service.stop().await;
                    app.exit(0);
                });
            }
            _ => {}
        })
        .build(app)?;

    *app.state::<AppState>().tray_menu.write().unwrap() = Some(items);
    update_tray_menu(app);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{format_size, tray_menu_state};

    #[test]
    fn formats_library_sizes() {
        assert_eq!(format_size(512), "0.50 KB");
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.00 GB");
    }

    #[test]
    fn running_menu_state_includes_port_and_library_stats() {
        let state = tray_menu_state(true, 9090, 12, 3 * 1024 * 1024);
        assert_eq!(state.status_text, "Status: Running (Port 9090)");
        assert_eq!(state.library_text, "Library: 12 files (3.00 MB)");
        assert!(state.is_running);
    }

    #[test]
    fn stopped_menu_state_is_offline() {
        let state = tray_menu_state(false, 9090, 12, 3 * 1024 * 1024);
        assert_eq!(state.status_text, "Status: Stopped");
        assert_eq!(state.library_text, "Library: Offline");
        assert!(!state.is_running);
    }
}
