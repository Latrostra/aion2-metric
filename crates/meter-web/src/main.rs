mod app;
mod controls;
mod dps;
mod i18n;
mod income;
mod limits;
mod meter;
mod overlay;
mod panels;
mod pets;
mod prefs;
mod settings;
mod shortcut;
mod tauri;

use leptos::prelude::document;

fn main() {
    console_error_panic_hook::set_once();
    let lang = prefs::load_lang();
    i18n::set(lang);
    if let Some(root) = document().document_element() {
        let _ = root.set_attribute("lang", lang.key());
    }
    overlay::mark_body_classes();
    leptos::mount::mount_to_body(app::App);
}
