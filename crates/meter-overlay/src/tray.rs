use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::lock::{self, LockState};
use crate::panels::MAIN_LABEL;

const LOCK_ITEM_ID: &str = "lock";
const SHOW_ITEM_ID: &str = "show";
const QUIT_ITEM_ID: &str = "quit";

fn toggle_visible(app: &AppHandle) {
    let visible = app
        .get_webview_window(MAIN_LABEL)
        .is_none_or(|main| main.is_visible().unwrap_or(true));
    for window in app.webview_windows().values() {
        let _ = if visible {
            window.hide()
        } else {
            window.show()
        };
    }
}

pub fn build(app: &AppHandle, lock_accel: &str) -> tauri::Result<()> {
    let lock_item = MenuItem::with_id(
        app,
        LOCK_ITEM_ID,
        lock::tray_label(lock_accel),
        true,
        None::<&str>,
    )?;
    app.state::<LockState>().set_tray_item(lock_item.clone());
    let show_item = MenuItem::with_id(app, SHOW_ITEM_ID, "Show / hide", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, QUIT_ITEM_ID, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&lock_item, &show_item, &quit_item])?;
    let icon = app
        .default_window_icon()
        .cloned()
        .expect("bundle icon configured");
    TrayIconBuilder::new()
        .icon(icon)
        .tooltip(crate::APP_NAME)
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            LOCK_ITEM_ID => lock::toggle_locked(app),
            SHOW_ITEM_ID => toggle_visible(app),
            QUIT_ITEM_ID => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
