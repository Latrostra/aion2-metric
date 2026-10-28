use leptos::prelude::window;
use leptos::task::spawn_local;
use meter_proto::Snapshot;
use serde::de::DeserializeOwned;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

const TAURI_INTERNALS: &str = "__TAURI_INTERNALS__";
const TAURI_GLOBAL: &str = "__TAURI__";
const TAURI_MISSING_ERROR: &str = "Tauri is not available";

fn invoke(command: &str, args: &JsValue) -> Option<js_sys::Promise> {
    let internals = js_sys::Reflect::get(&window(), &JsValue::from_str(TAURI_INTERNALS)).ok()?;
    if internals.is_undefined() {
        return None;
    }
    let invoke: js_sys::Function = js_sys::Reflect::get(&internals, &JsValue::from_str("invoke"))
        .ok()?
        .dyn_into()
        .ok()?;
    invoke
        .call2(&internals, &JsValue::from_str(command), args)
        .ok()?
        .dyn_into()
        .ok()
}

async fn call(command: &str, args: &JsValue) -> Result<JsValue, String> {
    let promise = invoke(command, args).ok_or(TAURI_MISSING_ERROR)?;
    JsFuture::from(promise)
        .await
        .map_err(|error| error.as_string().unwrap_or_else(|| format!("{error:?}")))
}

fn call_and_forget(command: &'static str, args: JsValue) {
    spawn_local(async move {
        let _ = call(command, &args).await;
    });
}

fn args(pairs: &[(&str, JsValue)]) -> JsValue {
    let object = js_sys::Object::new();
    for (name, value) in pairs {
        let _ = js_sys::Reflect::set(&object, &JsValue::from_str(name), value);
    }
    object.into()
}

pub fn string_list(value: &JsValue) -> Vec<String> {
    js_sys::Array::from(value)
        .iter()
        .filter_map(|item| item.as_string())
        .collect()
}

pub fn start_resize_dragging(direction: &str) {
    let _ = invoke(
        "plugin:window|start_resize_dragging",
        &args(&[("value", direction.into())]),
    );
}

pub fn set_tab_zone(bottom: f64) {
    let _ = invoke("set_tab_zone", &args(&[("bottom", bottom.into())]));
}

pub fn set_hide_unfocused(enabled: bool) {
    let _ = invoke("set_hide_unfocused", &args(&[("enabled", enabled.into())]));
}

pub fn lock_overlay() {
    call_and_forget("lock_overlay", JsValue::UNDEFINED);
}

pub fn detach_panel(panel: &str) {
    call_and_forget("detach_panel", args(&[("panel", panel.into())]));
}

pub fn attach_panel(panel: &str) {
    call_and_forget("attach_panel", args(&[("panel", panel.into())]));
}

pub async fn lock_shortcut() -> Option<String> {
    call("get_lock_shortcut", &JsValue::UNDEFINED)
        .await
        .ok()?
        .as_string()
}

pub async fn set_lock_shortcut(accel: &str) -> Result<Option<String>, String> {
    let saved = call("set_lock_shortcut", &args(&[("accel", accel.into())])).await?;
    Ok(saved.as_string())
}

pub async fn is_locked() -> bool {
    call("is_locked", &JsValue::UNDEFINED)
        .await
        .is_ok_and(|locked| locked.as_bool() == Some(true))
}

pub async fn detached_panels() -> Option<Vec<String>> {
    let panels = call("detached_panels", &JsValue::UNDEFINED).await.ok()?;
    Some(string_list(&panels))
}

fn from_json<T: DeserializeOwned>(value: &JsValue) -> Option<T> {
    let text = js_sys::JSON::stringify(value).ok()?.as_string()?;
    serde_json::from_str(&text).ok()
}

fn property(target: &JsValue, name: &str) -> Option<JsValue> {
    js_sys::Reflect::get(target, &JsValue::from_str(name))
        .ok()
        .filter(|value| !value.is_undefined())
}

pub fn listen_json<T: DeserializeOwned + 'static>(
    event: &str,
    mut handler: impl FnMut(T) + 'static,
) {
    let Some(events) =
        property(&window(), TAURI_GLOBAL).and_then(|tauri| property(&tauri, "event"))
    else {
        return;
    };
    let Some(listen) =
        property(&events, "listen").and_then(|f| f.dyn_into::<js_sys::Function>().ok())
    else {
        return;
    };
    let callback = Closure::<dyn FnMut(JsValue)>::new(move |message: JsValue| {
        if let Some(value) = property(&message, "payload").as_ref().and_then(from_json) {
            handler(value);
        }
    });
    let _ = listen.call2(&events, &JsValue::from_str(event), callback.as_ref());
    callback.forget();
}

pub async fn snapshot() -> Option<Snapshot> {
    from_json(&call("get_snapshot", &JsValue::UNDEFINED).await.ok()?)
}

pub fn reset_meter() {
    call_and_forget("reset_meter", JsValue::UNDEFINED);
}

pub fn reset_kinah() {
    call_and_forget("reset_kinah", JsValue::UNDEFINED);
}

pub fn set_idle_secs(secs: u32) {
    call_and_forget("set_idle_secs", args(&[("secs", secs.into())]));
}

pub fn set_recent_secs(secs: u32) {
    call_and_forget("set_recent_secs", args(&[("secs", secs.into())]));
}

pub fn set_pinned_pets(ids: &[u32]) {
    let list: js_sys::Array = ids.iter().map(|&id| JsValue::from(id)).collect();
    call_and_forget("set_pinned_pets", args(&[("ids", list.into())]));
}
