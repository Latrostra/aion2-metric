use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::dock;
use crate::panels::{self, MAIN_LABEL, PanelWindows, panel_name};

const DROP_POLL_INTERVAL: Duration = Duration::from_millis(30);
const DRAG_RELEASE_QUIET_TIME: Duration = Duration::from_millis(60);

#[cfg(windows)]
fn left_mouse_button_held() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    const KEY_DOWN_BIT: u16 = 0x8000;
    unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON)) as u16 & KEY_DOWN_BIT != 0 }
}

#[cfg(not(windows))]
fn left_mouse_button_held() -> bool {
    false
}

fn cursor_over_tab_bar(app: &AppHandle) -> bool {
    let windows = app.state::<PanelWindows>();
    let tab_bar_bottom = windows.tab_bar_bottom() as i32;
    let Some(main) = windows.rect_of(MAIN_LABEL) else {
        return false;
    };
    let Ok(cursor) = app.cursor_position() else {
        return false;
    };
    let (x, y) = (cursor.x as i32, cursor.y as i32);
    tab_bar_bottom > 0
        && x >= main.x
        && x < main.x + main.w as i32
        && y >= main.y
        && y < main.y + tab_bar_bottom
}

fn snap_to_neighbour(app: &AppHandle, label: &str) {
    let windows = app.state::<PanelWindows>();
    let rects = windows.rects().clone();
    let Some(rect) = rects.get(label).copied() else {
        return;
    };
    let snapped_position = {
        let mut layout = windows.layout();
        let targets = layout.snap_targets(label, &rects);
        let dock = dock::nearest_dock(rect, &targets);
        let position = dock.as_ref().and_then(|dock| {
            let target = rects.get(&dock.target)?;
            Some(dock::docked_position(*target, dock, rect.w, rect.h))
        });
        if let Some(entry) = layout.panels.get_mut(label) {
            entry.dock = dock;
        }
        position
    };
    if let Some((x, y)) = snapped_position
        && (x, y) != (rect.x, rect.y)
    {
        panels::move_window(app, label, x, y);
        panels::move_docked_children(app, label);
    }
    windows.save_layout();
}

fn return_to_tab(app: &AppHandle, label: &str) {
    let Some(panel) = panel_name(label) else {
        return;
    };
    panels::activate_tab(app, panel);
    panels::close_panel_window(app, label);
}

fn finish_drop(app: &AppHandle, label: &str, over_tab_bar: bool) {
    panels::show_drop_target(app, false);
    if over_tab_bar {
        return_to_tab(app, label);
    } else {
        snap_to_neighbour(app, label);
    }
}

pub fn watch_panel_drops(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(DROP_POLL_INTERVAL);
            let windows = app.state::<PanelWindows>();
            if !windows.is_dragging() {
                continue;
            }
            let over_tab_bar = cursor_over_tab_bar(&app);
            if left_mouse_button_held() {
                let handle = app.clone();
                let _ =
                    app.run_on_main_thread(move || panels::show_drop_target(&handle, over_tab_bar));
                continue;
            }
            for label in windows.take_finished_drags(DRAG_RELEASE_QUIET_TIME) {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || finish_drop(&handle, &label, over_tab_bar));
            }
        }
    });
}
