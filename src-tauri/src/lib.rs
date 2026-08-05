mod commands;
pub mod core;
mod persistence;
mod utils;

use commands::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            import::parse_excel,
            grouping::compute_grouping,
            export::export_result,
            export::export_multiple_results,
            clipboard::parse_clipboard_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
