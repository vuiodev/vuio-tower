mod service;
mod tray;

use tauri::Manager;
use tray::AppState;

#[tauri::command]
fn get_settings() -> Result<serde_json::Value, String> {
    service::ensure_config_exists()?;
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

fn patch_settings_document(
    original: &str,
    port: u16,
    name: &str,
    directories: &[String],
) -> Result<(String, vuio_core::config::ConfigChangeImpact), String> {
    use std::str::FromStr;
    use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};
    use vuio_core::config::{classify_change, AppConfig};

    let old_config = AppConfig::from_toml_str(original).map_err(|error| error.to_string())?;
    let mut document = DocumentMut::from_str(original).map_err(|error| error.to_string())?;
    if document["server"].get("uuid").is_none() {
        document["server"]["uuid"] = value(old_config.server.uuid.clone());
    }
    document["server"]["port"] = value(i64::from(port));
    document["server"]["name"] = value(name);
    let existing = document["media"]["directories"]
        .as_array_of_tables()
        .map(|tables| tables.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut updated = ArrayOfTables::new();
    for directory in directories {
        let normalized = directory.replace('\\', "/");
        if let Some(table) = existing.iter().find(|table| {
            table
                .get("path")
                .and_then(Item::as_str)
                .is_some_and(|path| path.replace('\\', "/") == normalized)
        }) {
            updated.push(table.clone());
        } else {
            let mut table = Table::new();
            table["path"] = value(normalized);
            table["recursive"] = value(true);
            updated.push(table);
        }
    }
    document["media"]["directories"] = Item::ArrayOfTables(updated);
    let candidate = document.to_string();
    let new_config = AppConfig::from_toml_str(&candidate).map_err(|error| error.to_string())?;
    Ok((candidate, classify_change(&old_config, &new_config)))
}

#[tauri::command]
async fn save_settings(
    state: tauri::State<'_, AppState>,
    port: u16,
    name: String,
    directories: Vec<String>,
) -> Result<(), String> {
    use vuio_core::config::ConfigChangeImpact;

    service::ensure_config_exists()?;
    let (config_path, _, _) = service::get_vuio_paths();
    let original = std::fs::read_to_string(&config_path).map_err(|error| error.to_string())?;
    let (candidate, impact) = patch_settings_document(&original, port, &name, &directories)?;
    if impact == ConfigChangeImpact::RestartRequired {
        state.service.stop().await?;
    }

    let temporary = config_path.with_extension("toml.tmp");
    std::fs::write(&temporary, &candidate).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, &config_path).map_err(|error| error.to_string())?;

    if impact == ConfigChangeImpact::RestartRequired {
        if let Err(error) = state.service.start().await {
            std::fs::write(&config_path, original).map_err(|rollback| rollback.to_string())?;
            let _ = state.service.start().await;
            return Err(format!(
                "new settings failed; previous settings restored: {error}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::patch_settings_document;
    use vuio_core::config::ConfigChangeImpact;

    const CONFIG: &str = r#"# keep this comment
[server]
port = 8080
interface = "0.0.0.0"
name = "VuIO"
custom_server_key = "keep"

[network]
interface_selection = "Auto"
multicast_ttl = 4
announce_interval_seconds = 30

[media]
scan_on_startup = true
watch_for_changes = true
supported_extensions = ["mp3"]

[[media.directories]]
path = "/media/music"
recursive = false
exclude_patterns = ["*.tmp"]

[database]
path = "/tmp/media"
vacuum_on_startup = false
backup_enabled = false

[future]
unknown = true
"#;

    #[test]
    fn settings_patch_preserves_unowned_toml() {
        let directories = vec!["/media/music".to_string(), "/media/video".to_string()];
        let (updated, impact) =
            patch_settings_document(CONFIG, 8080, "Living Room", &directories).unwrap();
        assert_eq!(impact, ConfigChangeImpact::LiveReload);
        assert!(updated.contains("# keep this comment"));
        assert!(updated.contains("custom_server_key = \"keep\""));
        assert!(updated.contains("exclude_patterns = [\"*.tmp\"]"));
        assert!(updated.contains("[future]"));
        assert!(updated.contains("unknown = true"));
        assert!(updated.contains("name = \"Living Room\""));
        assert!(updated.contains("path = \"/media/video\""));
    }

    #[test]
    fn port_change_requires_restart() {
        let directories = vec!["/media/music".to_string()];
        let (_, impact) = patch_settings_document(CONFIG, 9090, "VuIO", &directories).unwrap();
        assert_eq!(impact, ConfigChangeImpact::RestartRequired);
    }
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
            tray_menu: std::sync::RwLock::new(None),
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let startup_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let state = startup_handle.state::<AppState>();
                if let Err(error) = state.service.start().await {
                    eprintln!("Failed to start VuIO on boot: {error}");
                }
                tray::update_tray_menu(&startup_handle);
            });

            // Create tray menu
            let _ = tray::create_tray(&handle);

            // Start library stats updater loop
            tray::start_stats_updater(handle.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            select_directory
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // Ask the embedded core runtime to shut down cleanly on exit.
                let state = app.state::<AppState>();
                state.service.request_stop();
            }
        });
}
