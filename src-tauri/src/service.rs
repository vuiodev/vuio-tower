use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandChild as Child;

/// Resolves standard directories and ensures they exist.
pub fn get_vuio_paths() -> (PathBuf, PathBuf, PathBuf) {
    let mut config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    config_dir.push("vuio");
    let _ = fs::create_dir_all(&config_dir);

    let config_path = config_dir.join("config.toml");
    let db_path = config_dir.join("media.redb");
    let log_path = config_dir.join("vuio.log");

    (config_path, db_path, log_path)
}

/// Ensures the config file exists and contains the correct database path.
pub fn ensure_config_exists() {
    let (config_path, db_path, _) = get_vuio_paths();
    if !config_path.exists() {
        // Create default config
        let default_config = format!(
            r#"[server]
port = 8080
interface = "0.0.0.0"
name = "Vuio"

[network]
interface_selection = "auto"
multicast_ttl = 4
announce_interval_seconds = 30

[media]
scan_on_startup = true
watch_for_changes = true
supported_extensions = ["mp4", "mkv", "avi", "mp3", "flac", "wav", "jpg", "png", "gif"]

[[media.directories]]
path = "{}"
recursive = true

[database]
path = "{}"
vacuum_on_startup = false
backup_enabled = true
"#,
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join("Downloads").to_string_lossy().replace('\\', "/"),
            db_path.to_string_lossy().replace('\\', "/")
        );
        let _ = fs::write(&config_path, default_config);
    } else {
        // If it exists, let's verify if database.path is set. If not, set it.
        if let Ok(content) = fs::read_to_string(&config_path) {
            if !content.contains("database") || !content.contains("path =") {
                // Append database config if not present
                let mut updated = content;
                updated.push_str(&format!(
                    "\n\n[database]\npath = \"{}\"\nvacuum_on_startup = false\nbackup_enabled = true\n",
                    db_path.to_string_lossy().replace('\\', "/")
                ));
                let _ = fs::write(&config_path, updated);
            }
        }
    }
}

/// ServiceState manages the lifecycle of the vuio sidecar process.
pub struct ServiceState {
    pub child: Mutex<Option<Child>>,
}

impl ServiceState {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
        }
    }

    pub fn start(&self, app: &AppHandle) -> Result<(), String> {
        let mut lock = self.child.lock().unwrap();
        if lock.is_some() {
            return Ok(()); // Already running
        }

        // Ensure config is present and database path is configured
        ensure_config_exists();
        let (config_path, _, log_path) = get_vuio_paths();

        // Build sidecar args
        let args = vec![
            "-c".to_string(),
            config_path.to_string_lossy().to_string(),
            "--log-file".to_string(),
            log_path.to_string_lossy().to_string(),
        ];

        let sidecar = app.shell().sidecar("vuio").map_err(|e| e.to_string())?;
        let sidecar = sidecar.args(args);

        let (mut rx, child) = sidecar.spawn().map_err(|e| e.to_string())?;

        // Start thread to read stdout/stderr events
        tauri::async_runtime::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let tauri_plugin_shell::process::CommandEvent::Stdout(line) = event {
                    let line_str = String::from_utf8_lossy(&line);
                    println!("vuio sidecar: {}", line_str.trim());
                }
            }
        });

        *lock = Some(child);
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut lock = self.child.lock().unwrap();
        if let Some(child) = lock.take() {
            child.kill().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.child.lock().unwrap().is_some()
    }
}
