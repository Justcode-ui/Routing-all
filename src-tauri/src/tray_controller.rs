use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Runtime,
};

pub fn build_system_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "Open Settings", true, None::<&str>)?;
    let copy_key_item = MenuItem::with_id(app, "copy_key", "Copy Master Key", true, None::<&str>)?;
    let health_item = MenuItem::with_id(app, "health", "/v1/health — OK", true, None::<&str>)?;
    let sep = tauri::menu::PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit ROUTINGALL", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open_item, &copy_key_item, &health_item, &sep, &quit_item])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("ROUTINGALL — proxy running on 127.0.0.1:8081")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                // Show and focus the settings window
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "copy_key" => {
                // Emit copy event to webview window
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("copy_master_key", ());
                }
            }
            "quit" => {
                // Explicit Quit — the only action that terminates the listener (PRD FR-1a)
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
