use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::WheelEvent;

use crate::panels::own_panel;
use crate::tauri;

const OVERLAY_CLASS: &str = "overlay";
const PANEL_WINDOW_CLASS: &str = "panel";
pub const LOCKED_CLASS: &str = "locked";
const ROWS_HEIGHT_VAR: &str = "--rows-h";
const WHEEL_LINE_PX: f64 = 18.0;
pub const PRIMARY_BUTTON: i16 = 0;

const RESIZE_EDGES: [(&str, &str); 8] = [
    ("n", "North"),
    ("s", "South"),
    ("e", "East"),
    ("w", "West"),
    ("ne", "NorthEast"),
    ("nw", "NorthWest"),
    ("se", "SouthEast"),
    ("sw", "SouthWest"),
];

pub fn add_body_class(class: &str) {
    if let Some(body) = document().body() {
        let _ = body.class_list().add_1(class);
    }
}

fn body_has_class(class: &str) -> bool {
    document()
        .body()
        .is_some_and(|body| body.class_list().contains(class))
}

pub fn mark_body_classes() {
    add_body_class(OVERLAY_CLASS);
    if own_panel().is_some() {
        add_body_class(PANEL_WINDOW_CLASS);
    }
}

pub fn on_window_event(name: &str, handler: impl FnMut(web_sys::Event) + 'static) {
    let callback = Closure::<dyn FnMut(web_sys::Event)>::new(handler);
    let _ = window().add_event_listener_with_callback(name, callback.as_ref().unchecked_ref());
    callback.forget();
}

pub fn remember_unlocked_rows_height(rows: web_sys::Element) {
    let observed = rows.clone();
    let callback = Closure::<dyn FnMut(js_sys::Array)>::new(move |_: js_sys::Array| {
        if body_has_class(LOCKED_CLASS) {
            return;
        }
        if let Some(root) = document()
            .document_element()
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let height = format!("{}px", observed.client_height());
            let _ = root.style().set_property(ROWS_HEIGHT_VAR, &height);
        }
    });
    if let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
        observer.observe(&rows);
        std::mem::forget(observer);
    }
    callback.forget();
}

pub fn scroll_by_wheel(container: Option<web_sys::HtmlDivElement>, event: &WheelEvent) {
    let Some(container) = container else {
        return;
    };
    if container.scroll_height() <= container.client_height() {
        return;
    }
    let px_per_delta = if event.delta_mode() == WheelEvent::DOM_DELTA_LINE {
        WHEEL_LINE_PX
    } else {
        1.0
    };
    container.set_scroll_top(container.scroll_top() + (event.delta_y() * px_per_delta) as i32);
    event.prevent_default();
}

#[component]
pub fn ResizeHandles() -> impl IntoView {
    RESIZE_EDGES
        .into_iter()
        .map(|(edge_class, direction)| {
            view! {
                <div
                    class=format!("rz {edge_class}")
                    on:mousedown=move |event| {
                        if event.button() == PRIMARY_BUTTON {
                            event.prevent_default();
                            event.stop_propagation();
                            tauri::start_resize_dragging(direction);
                        }
                    }
                ></div>
            }
        })
        .collect_view()
}
