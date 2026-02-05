use std::fs::{File, OpenOptions, create_dir_all, remove_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use chrono::Local;

/// Get the data directory for the application.
/// 
/// Priority order:
/// 1. Current working directory + "data/" (for development/source runs)
/// 2. Executable directory + "data/" (for distributed builds)
/// 3. Fallback to just "data" relative path
pub fn get_data_dir() -> PathBuf {
    // First, check if data/ exists in current working directory (development mode)
    let cwd_data = PathBuf::from("data");
    if cwd_data.exists() && cwd_data.is_dir() {
        return cwd_data;
    }

    // Next, try executable's directory (distributed mode)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_data = exe_dir.join("data");
            if exe_data.exists() && exe_data.is_dir() {
                return exe_data;
            }
            // If it doesn't exist yet, use this path (will be created)
            return exe_data;
        }
    }

    // Fallback to current working directory
    PathBuf::from("data")
}

/// Get the imports directory (data/imports)
pub fn get_imports_dir() -> PathBuf {
    get_data_dir().join("imports")
}

/// Get the exports directory (data/exports)
pub fn get_exports_dir() -> PathBuf {
    get_data_dir().join("exports")
}

/// Get the user cache directory (data/user_cache)
pub fn get_cache_dir() -> PathBuf {
    get_data_dir().join("user_cache")
}

/// Get the logs directory (data/user_cache/logs)
pub fn get_logs_dir() -> PathBuf {
    get_cache_dir().join("logs")
}

/// Get the resources directory for the application.
/// 
/// Priority order:
/// 1. Current working directory + "res/" (for development/source runs)
/// 2. Executable directory + "res/" (for distributed builds)
/// 3. Fallback to just "res" relative path
pub fn get_res_dir() -> PathBuf {
    // First, check if res/ exists in current working directory (development mode)
    let cwd_res = PathBuf::from("res");
    if cwd_res.exists() && cwd_res.is_dir() {
        return cwd_res;
    }

    // Next, try executable's directory (distributed mode)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_res = exe_dir.join("res");
            if exe_res.exists() && exe_res.is_dir() {
                return exe_res;
            }
        }
    }

    // Fallback to current working directory
    PathBuf::from("res")
}

/// Get the path to the preview icon used in BRZ files.
/// Returns the path to res/obj_icon.png
pub fn get_preview_icon_path() -> PathBuf {
    get_res_dir().join("obj_icon.png")
}

/// Load the preview icon bytes from the res directory.
/// Falls back to embedded default if file not found.
pub fn load_preview_icon() -> Vec<u8> {
    let icon_path = get_preview_icon_path();
    if icon_path.exists() {
        if let Ok(bytes) = std::fs::read(&icon_path) {
            return bytes;
        }
    }
    // Fallback to embedded default
    include_bytes!("../../res/obj_icon.png").to_vec()
}

/// Ensure all data directories exist
pub fn ensure_data_dirs() -> std::io::Result<()> {
    create_dir_all(get_imports_dir())?;
    create_dir_all(get_exports_dir())?;
    create_dir_all(get_cache_dir())?;
    create_dir_all(get_logs_dir())?;
    Ok(())
}

/// Flush (delete) all user cache data including logs
pub fn flush_cache() -> std::io::Result<()> {
    let cache_dir = get_cache_dir();
    if cache_dir.exists() {
        remove_dir_all(&cache_dir)?;
    }
    // Recreate empty cache structure
    create_dir_all(get_logs_dir())?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Logger {
    messages: Arc<Mutex<Vec<String>>>,
    log_file: Arc<Mutex<Option<File>>>,
}

impl Logger {
    pub fn new() -> Self {
        // Ensure data directories exist
        let _ = ensure_data_dirs();

        // Create log file with timestamp
        let log_file = Self::create_log_file();

        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            log_file: Arc::new(Mutex::new(log_file)),
        }
    }

    /// Create a logger without a log file (for temporary use during deserialization)
    fn new_without_file() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            log_file: Arc::new(Mutex::new(None)),
        }
    }

    fn create_log_file() -> Option<File> {
        let logs_dir = get_logs_dir();
        if create_dir_all(&logs_dir).is_err() {
            return None;
        }

        let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
        let log_path = logs_dir.join(format!("obj2brz_{}.log", timestamp));

        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok()
    }

    pub fn log(&self, message: String) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let formatted = format!("[{}] {}", timestamp, message);

        // Log to memory (for GUI display)
        if let Ok(mut messages) = self.messages.lock() {
            messages.push(message.clone());
        }

        // Log to file
        if let Ok(mut file_guard) = self.log_file.lock() {
            if let Some(ref mut file) = *file_guard {
                let _ = writeln!(file, "{}", formatted);
                let _ = file.flush();
            }
        }
    }

    pub fn get_messages(&self) -> Vec<String> {
        self.messages.lock().ok()
            .map(|m| m.clone())
            .unwrap_or_default()
    }

    /// Get the path to the current log file
    #[allow(dead_code)]
    pub fn get_log_path(&self) -> Option<PathBuf> {
        let logs_dir = get_logs_dir();
        if logs_dir.exists() {
            // Return the most recent log file
            if let Ok(entries) = std::fs::read_dir(&logs_dir) {
                let mut files: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map(|ext| ext == "log").unwrap_or(false))
                    .collect();
                files.sort_by_key(|e| e.path());
                return files.last().map(|e| e.path());
            }
        }
        None
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new_without_file()
    }
}
