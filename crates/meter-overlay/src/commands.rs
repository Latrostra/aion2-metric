use meter_proto::Snapshot;
use tauri::{AppHandle, State};

use crate::lock::{self, LockState};
use crate::meter::MeterEngine;
use crate::panels::{self, PanelWindows, panel_label};

#[tauri::command]
pub fn get_lock_shortcut(lock_state: State<LockState>) -> String {
    lock_state.accel()
}

#[tauri::command]
pub fn set_lock_shortcut(app: AppHandle, accel: String) -> Result<String, String> {
    lock::change_shortcut(&app, accel)
}

#[tauri::command]
pub fn is_locked(lock_state: State<LockState>) -> bool {
    lock_state.is_locked()
}

#[tauri::command]
pub fn lock_overlay(app: AppHandle) {
    lock::set_locked(&app, true);
}

#[tauri::command]
pub fn detached_panels(windows: State<PanelWindows>) -> Vec<String> {
    windows.detached_panels()
}

#[tauri::command]
pub async fn detach_panel(app: AppHandle, panel: String) -> Result<(), String> {
    panels::detach(&app, &panel)
}

#[tauri::command]
pub fn attach_panel(app: AppHandle, panel: String) {
    panels::close_panel_window(&app, &panel_label(&panel));
}

#[tauri::command]
pub fn set_tab_zone(windows: State<PanelWindows>, bottom: f64) {
    windows.set_tab_bar_bottom(bottom);
}

#[tauri::command]
pub fn set_hide_unfocused(app: AppHandle, enabled: bool) {
    #[cfg(windows)]
    crate::game_window::set_follow_focus(&app, enabled);
    #[cfg(not(windows))]
    let _ = (app, enabled);
}

#[tauri::command]
pub fn get_snapshot(meter: State<MeterEngine>) -> Snapshot {
    meter.0.state().snapshot()
}

#[tauri::command]
pub fn reset_meter(meter: State<MeterEngine>) {
    meter.0.state().reset();
}

#[tauri::command]
pub fn reset_kinah(meter: State<MeterEngine>) {
    meter.0.state().reset_kinah();
}

#[tauri::command]
pub fn set_idle_secs(meter: State<MeterEngine>, secs: u32) {
    meter.0.state().set_idle_secs(secs);
}

#[tauri::command]
pub fn set_recent_secs(meter: State<MeterEngine>, secs: u32) {
    meter.0.state().set_recent_secs(secs);
}

#[tauri::command]
pub fn set_pinned_pets(meter: State<MeterEngine>, ids: Vec<u32>) {
    meter.0.state().set_pinned_pets(ids);
}
