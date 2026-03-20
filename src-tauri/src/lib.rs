mod search;
mod streaming;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            search::commands::search,
            search::commands::list_search_roots,
            search::commands::list_subfolders,
            search::commands::search_in_root,
            search::commands::search_folder_batch,
            streaming::cancel_search,
            streaming::search_streaming,
            streaming::search_with_progress,
            search::io_commands::open_in_explorer,
            search::io_commands::save_filter_file,
            search::io_commands::load_filter_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
