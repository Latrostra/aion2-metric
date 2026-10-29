#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod dock;
mod drag_drop;
#[cfg(windows)]
mod game_window;
mod hotkey;
mod lock;
mod main_window;
mod meter;
mod panels;
mod persist;
mod tray;
mod window_pos;

use tauri::{Manager, Window, WindowEvent};

use lock::LockState;
use panels::{MAIN_LABEL, PanelWindows, panel_name};
use window_pos::Tracker;

pub const APP_NAME: &str = "aion2-metric";

fn on_window_event(window: &Window, event: &WindowEvent) {
    let is_main = window.label() == MAIN_LABEL;
    match event {
        WindowEvent::Moved(position) => {
            if is_main {
                window.state::<Tracker>().moved(position.x, position.y);
            }
            panels::on_moved(window, position.x, position.y);
        }
        WindowEvent::Resized(size) => {
            if is_main {
                window.state::<Tracker>().resized(size.width, size.height);
            }
            panels::on_resized(window, size.width, size.height);
        }
        WindowEvent::Destroyed if panel_name(window.label()).is_some() => {
            panels::on_panel_closed(window.app_handle(), window.label());
        }
        _ => {}
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(lock::shortcut_plugin())
        .invoke_handler(tauri::generate_handler![
            commands::get_lock_shortcut,
            commands::set_lock_shortcut,
            commands::detached_panels,
            commands::detach_panel,
            commands::attach_panel,
            commands::set_tab_zone,
            commands::lock_overlay,
            commands::set_hide_unfocused,
            commands::is_locked,
            commands::get_snapshot,
            commands::reset_meter,
            commands::reset_income,
            commands::set_idle_secs,
            commands::set_recent_secs,
            commands::set_pinned_pets
        ])
        .manage(LockState::new())
        .manage(Tracker::new())
        .manage(PanelWindows::default())
        .on_window_event(on_window_event)
        .setup(|app| {
            let app = app.handle();
            app.manage(meter::start(app));
            let lock_accel = lock::register_saved_shortcut(app)?;
            main_window::restore(app);
            if let Err(err) = panels::restore(app) {
                eprintln!("could not restore panels: {err}");
            }
            drag_drop::watch_panel_drops(app.clone());
            #[cfg(windows)]
            game_window::follow_game(app);
            tray::build(app, &lock_accel)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the aion2-metric overlay");
}
