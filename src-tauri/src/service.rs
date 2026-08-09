use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
use vuio_core::lifecycle::RuntimeOptions;
use vuio_core::runtime::{RuntimeHandle, RuntimeStatus, VuioRuntime};

pub const DEFAULT_SERVER_NAME: &str = "VuIO";

/// Resolves standard directories and ensures they exist.
pub fn get_vuio_paths() -> (PathBuf, PathBuf, PathBuf) {
    let mut config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    config_dir.push("vuio");
    let _ = fs::create_dir_all(&config_dir);
    (
        config_dir.join("config.toml"),
        config_dir.join("media.redb"),
        config_dir.join("vuio.log"),
    )
}

pub fn ensure_config_exists() -> Result<(), String> {
    let (config_path, _, _) = get_vuio_paths();
    if !config_path.exists() {
        vuio_core::config::AppConfig::ensure_platform_directories_exist()
            .map_err(|error| error.to_string())?;
        let mut config = vuio_core::config::AppConfig::default();
        config.server.name = DEFAULT_SERVER_NAME.to_string();
        return config
            .save_to_file(config_path)
            .map_err(|error| error.to_string());
    }

    vuio_core::config::AppConfig::load_or_create(&config_path)
        .map_err(|error| error.to_string())?;
    let content = fs::read_to_string(&config_path).map_err(|error| error.to_string())?;
    if let Some(updated) = normalize_legacy_server_name(&content)? {
        fs::write(config_path, updated).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn normalize_legacy_server_name(content: &str) -> Result<Option<String>, String> {
    use std::str::FromStr;
    use toml_edit::{value, DocumentMut};

    let mut document = DocumentMut::from_str(content).map_err(|error| error.to_string())?;
    let name = document["server"]["name"].as_str();
    if name != Some("Vuio") {
        return Ok(None);
    }

    document["server"]["name"] = value(DEFAULT_SERVER_NAME);
    Ok(Some(document.to_string()))
}

/// Owns the single embedded VuIO runtime used by the desktop application.
pub struct ServiceState {
    runtime: Mutex<Option<RuntimeHandle>>,
    running: AtomicBool,
}

impl ServiceState {
    pub fn new() -> Self {
        Self {
            runtime: Mutex::new(None),
            running: AtomicBool::new(false),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        let mut runtime = self.runtime.lock().await;
        if runtime
            .as_ref()
            .is_some_and(|handle| handle.status() == RuntimeStatus::Running)
        {
            return Ok(());
        }

        ensure_config_exists()?;
        let (config_path, _, log_path) = get_vuio_paths();
        let handle = VuioRuntime::start(RuntimeOptions {
            config_path: Some(config_path.to_string_lossy().into_owned()),
            log_file: Some(log_path.to_string_lossy().into_owned()),
            ..RuntimeOptions::default()
        });
        *runtime = Some(handle);
        self.running.store(true, Ordering::Release);
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), String> {
        self.running.store(false, Ordering::Release);
        let handle = self.runtime.lock().await.take();
        if let Some(handle) = handle {
            handle.shutdown().await.map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn request_stop(&self) {
        self.running.store(false, Ordering::Release);
        if let Ok(runtime) = self.runtime.try_lock() {
            if let Some(handle) = runtime.as_ref() {
                handle.request_shutdown();
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_legacy_server_name;

    #[test]
    fn normalizes_legacy_default_server_name() {
        let content = "# preserved\n[server]\nname = \"Vuio\"\nport = 8080\n";
        let updated = normalize_legacy_server_name(content).unwrap().unwrap();
        assert!(updated.contains("# preserved"));
        assert!(updated.contains("name = \"VuIO\""));
        assert!(updated.contains("port = 8080"));
    }

    #[test]
    fn preserves_custom_server_name() {
        let content = "[server]\nname = \"Living Room\"\nport = 8080\n";
        assert!(normalize_legacy_server_name(content).unwrap().is_none());
    }
}
