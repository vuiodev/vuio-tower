mod service;
mod tray;

use tray::AppState;
use tauri::Manager;

#[tauri::command]
fn get_settings() -> Result<serde_json::Value, String> {
    service::ensure_config_exists();
    let (config_path, _, _) = service::get_vuio_paths();
    let content = std::fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let value = content.parse::<toml::Value>().map_err(|e| e.to_string())?;

    let port = value
        .get("server")
        .and_then(|s| s.get("port"))
        .and_then(|p| p.as_integer())
        .unwrap_or(8080) as u16;

    let name = value
        .get("server")
        .and_then(|s| s.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("Vuio")
        .to_string();

    let mut directories = Vec::new();
    if let Some(media) = value.get("media") {
        if let Some(dirs) = media.get("directories") {
            if let Some(arr) = dirs.as_array() {
                for item in arr {
                    if let Some(path) = item.get("path").and_then(|p| p.as_str()) {
                        directories.push(path.to_string());
                    }
                }
            }
        }
    }

    Ok(serde_json::json!({
        "port": port,
        "name": name,
        "directories": directories
    }))
}

#[tauri::command]
fn save_settings(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    port: u16,
    name: String,
    directories: Vec<String>,
) -> Result<(), String> {
    let (config_path, db_path, _) = service::get_vuio_paths();

    let mut toml_str = format!(
        r#"[server]
port = {}
interface = "0.0.0.0"
name = "{}"

[network]
interface_selection = "auto"
multicast_ttl = 4
announce_interval_seconds = 30

[media]
scan_on_startup = true
watch_for_changes = true
supported_extensions = ["mp4", "mkv", "avi", "mp3", "flac", "wav", "jpg", "png", "gif"]
"#,
        port, name
    );

    for dir in directories {
        toml_str.push_str(&format!(
            "\n[[media.directories]]\npath = \"{}\"\nrecursive = true\n",
            dir.replace('\\', "/")
        ));
    }

    toml_str.push_str(&format!(
        "\n[database]\npath = \"{}\"\nvacuum_on_startup = false\nbackup_enabled = true\n",
        db_path.to_string_lossy().replace('\\', "/")
    ));

    std::fs::write(config_path, toml_str).map_err(|e| e.to_string())?;

    // Restart service
    let _ = state.service.stop();
    let _ = state.service.start(&app);
    tray::update_tray_menu(&app);

    Ok(())
}

#[tauri::command]
async fn select_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();

    app.dialog().file().pick_folder(move |folder| {
        let path_str = folder.and_then(|f| match f {
            tauri_plugin_dialog::FilePath::Path(p) => Some(p.to_string_lossy().to_string()),
            _ => None,
        });
        let _ = tx.send(path_str);
    });

    rx.await.map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .manage(AppState {
            service: service::ServiceState::new(),
            stats: std::sync::RwLock::new(tray::LibraryStats {
                total_files: 0,
                total_size_bytes: 0,
            }),
        })
        .setup(|app| {
            let handle = app.handle();
            let state = handle.state::<AppState>();

            // Auto-start the sidecar server on startup
            if let Err(e) = state.service.start(handle) {
                eprintln!("Failed to start sidecar on boot: {}", e);
            }

            // Create tray menu
            let _ = tray::create_tray(handle);

            // Start library stats updater loop
            tray::start_stats_updater(handle.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_settings, save_settings, select_directory])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // Terminate background sidecar cleanly on exit
                let state = app.state::<AppState>();
                let _ = state.service.stop();
            }
        });
}
