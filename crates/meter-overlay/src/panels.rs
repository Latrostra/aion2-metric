use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, Window,
};

use crate::dock::{self, Layout, PanelState, Rect};
use crate::lock;

pub const MAIN_LABEL: &str = "main";
pub const PANELS: [&str; 4] = ["dps", "pets", "limits", "stats"];
const PANEL_LABEL_PREFIX: &str = "panel-";
const PANEL_MIN_WIDTH: f64 = 160.0;
const PANEL_MIN_HEIGHT: f64 = 80.0;
const FALLBACK_PANEL_SIZE: (u32, u32) = (324, 222);
const DETACH_GRAB_OFFSET_X: i32 = 60;
const DETACH_GRAB_OFFSET_Y: i32 = 14;

pub fn panel_label(panel: &str) -> String {
    format!("{PANEL_LABEL_PREFIX}{panel}")
}

pub fn panel_name(label: &str) -> Option<&str> {
    label.strip_prefix(PANEL_LABEL_PREFIX)
}

#[derive(Default)]
pub struct PanelWindows {
    window_rects: Mutex<HashMap<String, Rect>>,
    layout: Mutex<Layout>,
    moves_in_flight: Mutex<HashMap<String, (i32, i32)>>,
    last_drag_moves: Mutex<HashMap<String, Instant>>,
    tab_bar_bottom: Mutex<u32>,
    drop_target_shown: AtomicBool,
    layout_saver: Mutex<Option<Sender<Layout>>>,
}

impl PanelWindows {
    pub fn rects(&self) -> MutexGuard<'_, HashMap<String, Rect>> {
        self.window_rects.lock().expect("window rects")
    }

    pub fn layout(&self) -> MutexGuard<'_, Layout> {
        self.layout.lock().expect("layout")
    }

    pub fn rect_of(&self, label: &str) -> Option<Rect> {
        self.rects().get(label).copied()
    }

    pub fn save_layout(&self) {
        let layout = self.layout().clone();
        if let Some(saver) = self.layout_saver.lock().expect("layout saver").as_ref() {
            let _ = saver.send(layout);
        }
    }

    pub fn detached_panels(&self) -> Vec<String> {
        self.layout()
            .panels
            .keys()
            .filter_map(|label| panel_name(label).map(str::to_string))
            .collect()
    }

    pub fn tab_bar_bottom(&self) -> u32 {
        *self.tab_bar_bottom.lock().expect("tab bar")
    }

    pub fn set_tab_bar_bottom(&self, bottom: f64) {
        *self.tab_bar_bottom.lock().expect("tab bar") = bottom.max(0.0).round() as u32;
    }

    pub fn set_drop_target_shown(&self, shown: bool) -> bool {
        self.drop_target_shown.swap(shown, Ordering::Relaxed) != shown
    }

    pub fn is_dragging(&self) -> bool {
        !self.drags().is_empty()
    }

    pub fn take_finished_drags(&self, quiet_time: Duration) -> Vec<String> {
        self.drags()
            .extract_if(|_, last_move| last_move.elapsed() >= quiet_time)
            .map(|(label, _)| label)
            .collect()
    }

    fn drags(&self) -> MutexGuard<'_, HashMap<String, Instant>> {
        self.last_drag_moves.lock().expect("drags")
    }

    fn expect_move(&self, label: &str, x: i32, y: i32) {
        self.moves_in_flight
            .lock()
            .expect("moves in flight")
            .insert(label.to_string(), (x, y));
    }

    fn take_expected_move(&self, label: &str, x: i32, y: i32) -> bool {
        let mut in_flight = self.moves_in_flight.lock().expect("moves in flight");
        let expected = in_flight.get(label) == Some(&(x, y));
        if expected {
            in_flight.remove(label);
        }
        expected
    }

    fn update_rect(&self, label: &str, change: impl FnOnce(&mut Rect)) -> Option<Rect> {
        let mut rects = self.rects();
        let rect = rects.get_mut(label)?;
        change(rect);
        Some(*rect)
    }
}

fn eval_in_main(app: &AppHandle, script: String) {
    if let Some(main) = app.get_webview_window(MAIN_LABEL) {
        let _ = main.eval(script);
    }
}

pub fn move_window(app: &AppHandle, label: &str, x: i32, y: i32) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    let windows = app.state::<PanelWindows>();
    windows.expect_move(label, x, y);
    windows.update_rect(label, |rect| {
        rect.x = x;
        rect.y = y;
    });
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

pub fn move_docked_children(app: &AppHandle, label: &str) {
    let moves = {
        let windows = app.state::<PanelWindows>();
        let rects = windows.rects().clone();
        windows.layout().follow_moves(label, &rects)
    };
    for (child, x, y) in moves {
        move_window(app, &child, x, y);
    }
}

pub fn on_moved(window: &Window, x: i32, y: i32) {
    let app = window.app_handle();
    let label = window.label();
    let windows = app.state::<PanelWindows>();
    let Some(rect) = windows.update_rect(label, |rect| {
        rect.x = x;
        rect.y = y;
    }) else {
        return;
    };
    let moved_by_us = windows.take_expected_move(label, x, y);
    let is_panel = panel_name(label).is_some();
    if is_panel {
        if let Some(entry) = windows.layout().panels.get_mut(label) {
            entry.rect = rect;
            if !moved_by_us {
                entry.dock = None;
            }
        }
        if !moved_by_us {
            windows.drags().insert(label.to_string(), Instant::now());
        }
    }
    move_docked_children(app, label);
    if is_panel {
        windows.save_layout();
    }
}

pub fn on_resized(window: &Window, width: u32, height: u32) {
    if width == 0 || height == 0 {
        return;
    }
    let app = window.app_handle();
    let label = window.label();
    let windows = app.state::<PanelWindows>();
    let Some(rect) = windows.update_rect(label, |rect| {
        rect.w = width;
        rect.h = height;
    }) else {
        return;
    };
    if let Some(entry) = windows.layout().panels.get_mut(label) {
        entry.rect = rect;
    }
    move_docked_children(app, label);
    if panel_name(label).is_some() {
        windows.save_layout();
    }
}

pub fn on_panel_closed(app: &AppHandle, label: &str) {
    let windows = app.state::<PanelWindows>();
    windows.layout().remove_panel(label);
    windows.rects().remove(label);
    windows.save_layout();
    notify_detached(app);
}

fn notify_detached(app: &AppHandle) {
    let detached = app.state::<PanelWindows>().detached_panels();
    let json = serde_json::to_string(&detached).unwrap_or_else(|_| "[]".into());
    eval_in_main(
        app,
        format!("window.a2mDetached && window.a2mDetached({json})"),
    );
}

pub fn activate_tab(app: &AppHandle, panel: &str) {
    eval_in_main(
        app,
        format!(
            "window.a2mActivate && window.a2mActivate({})",
            serde_json::to_string(panel).unwrap_or_default()
        ),
    );
}

pub fn show_drop_target(app: &AppHandle, shown: bool) {
    if !app.state::<PanelWindows>().set_drop_target_shown(shown) {
        return;
    }
    eval_in_main(
        app,
        format!("document.body.classList.toggle('drop-target', {shown})"),
    );
}

fn open_panel(app: &AppHandle, panel: &str, rect: Rect) -> tauri::Result<WebviewWindow> {
    let label = panel_label(panel);
    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(format!("index.html?panel={panel}").into()),
    )
    .title(format!("{} {panel}", crate::APP_NAME))
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(true)
    .min_inner_size(PANEL_MIN_WIDTH, PANEL_MIN_HEIGHT)
    .visible(false)
    .build()?;
    window.set_size(PhysicalSize::new(rect.w, rect.h))?;
    window.set_position(PhysicalPosition::new(rect.x, rect.y))?;
    let windows = app.state::<PanelWindows>();
    windows.rects().insert(label.clone(), rect);
    windows.expect_move(&label, rect.x, rect.y);
    if lock::is_locked(app) {
        let _ = window.set_ignore_cursor_events(true);
    }
    window.show()?;
    Ok(window)
}

pub fn detach(app: &AppHandle, panel: &str) -> Result<(), String> {
    if !PANELS.contains(&panel) {
        return Err(format!("unknown panel {panel}"));
    }
    let label = panel_label(panel);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_focus();
        return Ok(());
    }
    let windows = app.state::<PanelWindows>();
    let (width, height) = windows
        .rect_of(MAIN_LABEL)
        .map_or(FALLBACK_PANEL_SIZE, |main| (main.w, main.h));
    let cursor = app.cursor_position().map_err(|e| e.to_string())?;
    let rect = Rect {
        x: cursor.x as i32 - DETACH_GRAB_OFFSET_X,
        y: cursor.y as i32 - DETACH_GRAB_OFFSET_Y,
        w: width,
        h: height,
    };
    windows
        .layout()
        .panels
        .insert(label, PanelState { rect, dock: None });
    let window = open_panel(app, panel, rect).map_err(|e| e.to_string())?;
    windows.save_layout();
    notify_detached(app);
    let _ = window.start_dragging();
    Ok(())
}

pub fn close_panel_window(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.close();
    }
}

pub fn restore(app: &AppHandle) -> tauri::Result<()> {
    let config_dir = app.path().app_config_dir()?;
    let layout = dock::load(&config_dir);
    let windows = app.state::<PanelWindows>();
    *windows.layout_saver.lock().expect("layout saver") = Some(dock::saver(config_dir));
    *windows.layout() = layout.clone();
    for (label, panel_state) in &layout.panels {
        let Some(panel) = panel_name(label).filter(|name| PANELS.contains(name)) else {
            continue;
        };
        if let Err(err) = open_panel(app, panel, panel_state.rect) {
            eprintln!("could not reopen panel {panel}: {err}");
        }
    }
    move_docked_children(app, MAIN_LABEL);
    Ok(())
}
