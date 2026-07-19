use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;
use std::sync::RwLock;

pub struct LibraryStats {
    pub total_files: usize,
    pub total_size_bytes: u64,
}

pub struct AppState {
    pub service: crate::service::ServiceState,
    pub stats: RwLock<LibraryStats>,
}

pub fn get_configured_port() -> u16 {
    let (config_path, _, _) = crate::service::get_vuio_paths();
    if let Ok(content) = std::fs::read_to_string(config_path) {
        if let Ok(value) = content.parse::<toml::Value>() {
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

pub fn build_menu(app: &tauri::AppHandle) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let state = app.state::<AppState>();
    let is_running = state.service.is_running();
    let port = get_configured_port();

    let status_text = if is_running {
        format!("Status: Running (Port {})", port)
    } else {
        "Status: Stopped".to_string()
    };

    let status_item = MenuItemBuilder::new(status_text)
        .id("status")
        .enabled(false)
        .build(app)?;

    let (files_count, size_formatted) = {
        let stats = state.stats.read().unwrap();
        (stats.total_files, format_size(stats.total_size_bytes))
    };

    let library_text = if is_running {
        format!("Library: {} files ({})", files_count, size_formatted)
    } else {
        "Library: Offline".to_string()
    };

    let library_item = MenuItemBuilder::new(library_text)
        .id("library")
        .enabled(false)
        .build(app)?;

    let open_item = MenuItemBuilder::new("Open Dashboard")
        .id("open_dashboard")
        .enabled(is_running)
        .build(app)?;

    let start_item = MenuItemBuilder::new("Start Server")
        .id("start_server")
        .enabled(!is_running)
        .build(app)?;

    let stop_item = MenuItemBuilder::new("Stop Server")
        .id("stop_server")
        .enabled(is_running)
        .build(app)?;

    let scan_item = MenuItemBuilder::new("Trigger Media Scan")
        .id("trigger_scan")
        .enabled(is_running)
        .build(app)?;

    let reset_item = MenuItemBuilder::new("Reset Database...")
        .id("reset_db")
        .enabled(is_running)
        .build(app)?;

    let settings_item = MenuItemBuilder::new("Settings...")
        .id("settings")
        .build(app)?;

    let quit_item = MenuItemBuilder::new("Quit")
        .id("quit")
        .build(app)?;

    Menu::with_items(
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
    )
}

pub fn update_tray_menu(app: &tauri::AppHandle) {
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(menu) = build_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

pub fn open_settings_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
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
                        let total_files = db_stats.get("total_files")
                            .and_then(|f| f.as_u64())
                            .unwrap_or(0) as usize;
                        let total_size_bytes = db_stats.get("total_size_bytes")
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
    let menu = build_menu(app)?;

    let _tray = TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().unwrap())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open_dashboard" => {
                let port = get_configured_port();
                let url = format!("http://localhost:{}", port);
                let _ = app.opener().open_url(url, None::<String>);
            }
            "start_server" => {
                let state = app.state::<AppState>();
                if let Err(e) = state.service.start(app) {
                    eprintln!("Failed to start server: {}", e);
                }
                update_tray_menu(app);
            }
            "stop_server" => {
                let state = app.state::<AppState>();
                if let Err(e) = state.service.stop() {
                    eprintln!("Failed to stop server: {}", e);
                }
                update_tray_menu(app);
            }
            "trigger_scan" => {
                let port = get_configured_port();
                tauri::async_runtime::spawn(async move {
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
                            let state = app_clone.state::<AppState>();
                            let _ = state.service.stop();
                            let (_, db_path, _) = crate::service::get_vuio_paths();
                            if db_path.exists() {
                                let _ = std::fs::remove_file(db_path);
                            }
                            let _ = state.service.start(&app_clone);
                            
                            // Reset local stats count immediately
                            {
                                let mut stats = state.stats.write().unwrap();
                                stats.total_files = 0;
                                stats.total_size_bytes = 0;
                            }
                            
                            update_tray_menu(&app_clone);
                        }
                    });
            }
            "settings" => {
                open_settings_window(app);
            }
            "quit" => {
                let state = app.state::<AppState>();
                let _ = state.service.stop();
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
