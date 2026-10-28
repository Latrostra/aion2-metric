use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow};

use crate::dock::Rect;
use crate::panels::{MAIN_LABEL, PanelWindows};
use crate::window_pos::{self, Area, Geometry, Tracker};

fn monitor_area(monitor: &Monitor) -> Area {
    Area {
        x: monitor.position().x,
        y: monitor.position().y,
        width: monitor.size().width,
        height: monitor.size().height,
    }
}

fn restore_and_track(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<()> {
    let config_dir = app.path().app_config_dir()?;
    let monitors: Vec<Area> = window
        .available_monitors()?
        .iter()
        .map(monitor_area)
        .collect();
    if let Some(saved) =
        window_pos::load(&config_dir).filter(|geometry| geometry.is_reachable(&monitors))
    {
        window.set_size(PhysicalSize::new(saved.width, saved.height))?;
        window.set_position(PhysicalPosition::new(saved.x, saved.y))?;
    }
    let (position, size) = (window.outer_position()?, window.outer_size()?);
    app.state::<PanelWindows>().rects().insert(
        MAIN_LABEL.to_string(),
        Rect {
            x: position.x,
            y: position.y,
            w: size.width,
            h: size.height,
        },
    );
    app.state::<Tracker>().start(
        config_dir,
        Geometry {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        },
    );
    window.show()
}

pub fn restore(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };
    if let Err(err) = restore_and_track(app, &window) {
        eprintln!("could not restore window position: {err}");
        let _ = window.show();
    }
}
