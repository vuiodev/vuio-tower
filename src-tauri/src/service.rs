use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
use vuio_core::lifecycle::RuntimeOptions;
use vuio_core::runtime::{RuntimeHandle, RuntimeStatus, VuioRuntime};

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
    vuio_core::config::AppConfig::load_or_create(config_path)
        .map(|_| ())
        .map_err(|error| error.to_string())
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
