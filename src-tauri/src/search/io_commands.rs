use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[tauri::command]
pub fn open_in_explorer(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }

    let cleaned = trimmed.trim_matches('"').replace('/', "\\");
    let input_path = PathBuf::from(cleaned);
    let target = fs::canonicalize(&input_path).unwrap_or(input_path);

    if target.is_file() {
        Command::new("explorer")
            .arg("/select,")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to reveal file in Explorer: {}", e))
    } else if target.is_dir() {
        Command::new("explorer")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to open folder in Explorer: {}", e))
    } else {
        Err(format!("Path does not exist: {}", target.display()))
    }
}

#[tauri::command]
pub fn save_filter_file(path: String, content: String) -> Result<(), String> {
    let trimmed = path.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }

    let target = PathBuf::from(trimmed);
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create filter directory: {}", e))?;
        }
    }

    fs::write(&target, content).map_err(|e| format!("Failed to save filter file: {}", e))
}

#[tauri::command]
pub fn load_filter_file(path: String) -> Result<String, String> {
    let trimmed = path.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }

    let target = PathBuf::from(trimmed);
    fs::read_to_string(&target).map_err(|e| format!("Failed to load filter file: {}", e))
}
