use leptos::prelude::*;
use meter_proto::KinahView;

use crate::i18n::tr;
use crate::limits::group_digits;

const UNKNOWN_VALUE: &str = "?";

fn kinah_entry(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="pet">
            <span class="pet-name">{label}</span>
            <span class="pet-count">{value}</span>
        </div>
    }
}

#[component]
pub fn KinahPanel(kinah: Signal<KinahView>) -> impl IntoView {
    let waiting_banner = move || {
        (!kinah.get().counting).then(|| {
            view! {
                <p class="pets-warn">
                    {tr(
                        "Counting starts after the first kina change or a relog.",
                        "Liczenie zacznie się po pierwszej zmianie kiny albo przelogowaniu.",
                    )}
                </p>
            }
        })
    };
    view! {
        <div class="pets">
            {waiting_banner}
            {move || {
                let kinah = kinah.get();
                let per_hour = kinah
                    .per_hour
                    .map(group_digits)
                    .unwrap_or_else(|| UNKNOWN_VALUE.to_string());
                view! {
                    {kinah_entry(tr("Earned kina", "Zarobiona kina"), group_digits(kinah.earned))}
                    {kinah_entry(tr("Kina/h", "Kina/h"), per_hour)}
                }
            }}
        </div>
    }
}
