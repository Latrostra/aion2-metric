use leptos::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::PointerEvent;

use crate::i18n::tr;
use crate::overlay::PRIMARY_BUTTON;
use crate::tauri;

const PANEL_QUERY_PREFIX: &str = "panel=";
const ACTIVATE_GLOBAL: &str = "a2mActivate";
const DETACHED_GLOBAL: &str = "a2mDetached";
const TAB_BAR_SELECTOR: &str = ".panel-tabs";
const HEADER_SELECTOR: &str = "header";
const TEAR_OFF_PX: i32 = 12;
const PRIMARY_BUTTON_MASK: u16 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    Dps,
    Pets,
    Limits,
    Kinah,
}

impl Panel {
    pub const ALL: [Panel; 4] = [Panel::Dps, Panel::Pets, Panel::Limits, Panel::Kinah];

    pub fn key(self) -> &'static str {
        match self {
            Panel::Dps => "dps",
            Panel::Pets => "pets",
            Panel::Limits => "limits",
            Panel::Kinah => "kinah",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Panel::Dps => "DPS",
            Panel::Pets => tr("Pets", "Pety"),
            Panel::Limits => tr("Limits", "Limity"),
            Panel::Kinah => tr("Kina", "Kina"),
        }
    }

    pub fn parse(key: &str) -> Option<Panel> {
        Panel::ALL.into_iter().find(|panel| panel.key() == key)
    }
}

pub fn own_panel() -> Option<Panel> {
    let search = window().location().search().ok()?;
    search
        .trim_start_matches('?')
        .split('&')
        .find_map(|param| param.strip_prefix(PANEL_QUERY_PREFIX))
        .and_then(Panel::parse)
}

fn expose_global(name: &str, handler: impl Fn(JsValue) + 'static) {
    let callback = Closure::<dyn Fn(JsValue)>::new(handler);
    let _ = js_sys::Reflect::set(&window(), &JsValue::from_str(name), callback.as_ref());
    callback.forget();
}

pub fn expose_activate(active: RwSignal<Panel>) {
    expose_global(ACTIVATE_GLOBAL, move |value| {
        if let Some(panel) = value.as_string().as_deref().and_then(Panel::parse) {
            active.set(panel);
        }
    });
}

pub fn expose_detached_setter(detached: RwSignal<Vec<String>>) {
    expose_global(DETACHED_GLOBAL, move |value| {
        detached.set(tauri::string_list(&value));
    });
}

pub fn report_tab_zone() {
    let doc = document();
    let Some(tab_zone) = doc
        .query_selector(TAB_BAR_SELECTOR)
        .ok()
        .flatten()
        .or_else(|| doc.query_selector(HEADER_SELECTOR).ok().flatten())
    else {
        return;
    };
    let bottom = tab_zone.get_bounding_client_rect().bottom() * window().device_pixel_ratio();
    tauri::set_tab_zone(bottom);
}

#[derive(Clone, Copy)]
struct TabPress {
    panel: Panel,
    start_x: i32,
    start_y: i32,
}

fn event_element(event: &PointerEvent) -> Option<web_sys::Element> {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
}

#[component]
pub fn TabBar(
    tabs: Memo<Vec<Panel>>,
    current: Memo<Option<Panel>>,
    active: RwSignal<Panel>,
) -> impl IntoView {
    let press = StoredValue::new(None::<TabPress>);
    let on_move = move |event: PointerEvent| {
        let Some(TabPress {
            panel,
            start_x,
            start_y,
        }) = press.get_value()
        else {
            return;
        };
        if event.buttons() & PRIMARY_BUTTON_MASK == 0 {
            press.set_value(None);
            return;
        }
        let dx = event.screen_x() - start_x;
        let dy = event.screen_y() - start_y;
        if dx.abs().max(dy.abs()) >= TEAR_OFF_PX {
            press.set_value(None);
            if let Some(element) = event_element(&event) {
                let _ = element.release_pointer_capture(event.pointer_id());
            }
            tauri::detach_panel(panel.key());
        }
    };
    let on_down = move |panel: Panel, event: PointerEvent| {
        if event.button() == PRIMARY_BUTTON {
            press.set_value(Some(TabPress {
                panel,
                start_x: event.screen_x(),
                start_y: event.screen_y(),
            }));
            if let Some(element) = event_element(&event) {
                let _ = element.set_pointer_capture(event.pointer_id());
            }
        }
    };
    let title = tr("Drag out to detach", "Przeciągnij, żeby odpiąć");
    view! {
        <nav class="panel-tabs">
            <For each=move || tabs.get() key=|panel| panel.key() let:panel>
                <button
                    class="panel-tab"
                    class:on=move || current.get() == Some(panel)
                    title=title
                    on:click=move |_| active.set(panel)
                    on:pointerdown=move |event: PointerEvent| on_down(panel, event)
                    on:pointermove=on_move
                    on:pointerup=move |_| press.set_value(None)
                >
                    {panel.label()}
                </button>
            </For>
        </nav>
    }
}
