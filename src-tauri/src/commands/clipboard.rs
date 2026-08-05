use arboard::Clipboard;

/// Parse file paths from clipboard (files copied from Finder/Explorer)
#[tauri::command]
pub async fn parse_clipboard_files() -> Result<Vec<String>, String> {
    // arboard's Clipboard is not Send, must be created and used in a separate thread
    let handle = std::thread::spawn(|| -> Result<Vec<String>, String> {
        let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
        match clipboard.get().file_list() {
            Ok(file_list) => {
                let paths: Vec<String> = file_list
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                println!(
                    "[Rust] parse_clipboard_files: Retrieved {} file path(s)",
                    paths.len()
                );
                Ok(paths)
            }
            Err(e) => {
                println!(
                    "[Rust] parse_clipboard_files: No files in clipboard - {}",
                    e
                );
                Ok(vec![]) // No files in clipboard, return empty list
            }
        }
    });

    match handle.join() {
        Ok(res) => res,
        Err(_) => Err("Task execution failed: thread panicked".to_string()),
    }
}
