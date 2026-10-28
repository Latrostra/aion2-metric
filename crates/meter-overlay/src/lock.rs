use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use tauri::menu::MenuItem;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::hotkey;

struct LockShortcut {
    accel: String,
    shortcut: Shortcut,
}

pub struct LockState {
    locked: AtomicBool,
    shortcut: Mutex<LockShortcut>,
    tray_item: Mutex<Option<MenuItem<Wry>>>,
}

impl LockState {
    pub fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            shortcut: Mutex::new(LockShortcut {
                accel: hotkey::DEFAULT.to_string(),
                shortcut: default_shortcut(),
            }),
            tray_item: Mutex::new(None),
        }
    }

    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }

    pub fn accel(&self) -> String {
        self.active_shortcut().accel.clone()
    }

    pub fn set_tray_item(&self, item: MenuItem<Wry>) {
        *self.tray_item.lock().expect("tray item") = Some(item);
    }

    fn active_shortcut(&self) -> MutexGuard<'_, LockShortcut> {
        self.shortcut.lock().expect("lock key")
    }
}

fn default_shortcut() -> Shortcut {
    hotkey::parse(hotkey::DEFAULT).expect("default shortcut parses")
}

pub fn tray_label(accel: &str) -> String {
    format!("Lock / unlock ({})", hotkey::display(accel))
}

pub fn is_locked(app: &AppHandle) -> bool {
    app.state::<LockState>().is_locked()
}

pub fn set_locked(app: &AppHandle, locked: bool) {
    app.state::<LockState>()
        .locked
        .store(locked, Ordering::Relaxed);
    for window in app.webview_windows().values() {
        let _ = window.set_ignore_cursor_events(locked);
        let _ = window.set_resizable(!locked);
        let _ = window.eval(format!(
            "document.body.classList.toggle('locked', {locked})"
        ));
    }
}

pub fn toggle_locked(app: &AppHandle) {
    set_locked(app, !is_locked(app));
}

pub fn shortcut_plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            let lock_shortcut = app.state::<LockState>().active_shortcut().shortcut;
            if *shortcut == lock_shortcut && event.state() == ShortcutState::Pressed {
                toggle_locked(app);
            }
        })
        .build()
}

pub fn register_saved_shortcut(app: &AppHandle) -> Result<String, Box<dyn Error>> {
    let saved = hotkey::load(&app.path().app_config_dir()?);
    match hotkey::parse(&saved) {
        Ok(shortcut) if app.global_shortcut().register(shortcut).is_ok() => {
            *app.state::<LockState>().active_shortcut() = LockShortcut {
                accel: saved.clone(),
                shortcut,
            };
            Ok(saved)
        }
        _ => {
            eprintln!(
                "lock shortcut {saved} unavailable, using {}",
                hotkey::DEFAULT
            );
            app.global_shortcut().register(default_shortcut())?;
            Ok(hotkey::DEFAULT.to_string())
        }
    }
}

pub fn change_shortcut(app: &AppHandle, accel: String) -> Result<String, String> {
    let new_shortcut = hotkey::parse(&accel)?;
    let lock_state = app.state::<LockState>();
    let old_shortcut = lock_state.active_shortcut().shortcut;
    if new_shortcut != old_shortcut {
        app.global_shortcut()
            .register(new_shortcut)
            .map_err(|e| format!("shortcut taken by another app: {e}"))?;
        let _ = app.global_shortcut().unregister(old_shortcut);
    }
    *lock_state.active_shortcut() = LockShortcut {
        accel: accel.clone(),
        shortcut: new_shortcut,
    };
    if let Some(item) = lock_state.tray_item.lock().expect("tray item").as_ref() {
        let _ = item.set_text(tray_label(&accel));
    }
    let config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    hotkey::save(&config_dir, &accel).map_err(|e| format!("nie zapisano: {e}"))?;
    Ok(accel)
}
