use leptos::prelude::*;
use meter_proto::PetView;

use crate::i18n::tr;

const UNKNOWN_PROGRESS: &str = "?";
const MAX_LEVEL_GOAL: u32 = 0;
const PINNED_ICON: &str = "★";
const UNPINNED_ICON: &str = "☆";

struct PetProgress {
    percent: f64,
    label: String,
    complete: bool,
}

fn pet_progress(pet: &PetView) -> PetProgress {
    if !pet.level_known {
        return PetProgress {
            percent: 0.0,
            label: UNKNOWN_PROGRESS.to_string(),
            complete: false,
        };
    }
    let level = format!("{} {}", tr("Lv", "poz."), pet.level);
    if pet.goal == MAX_LEVEL_GOAL {
        return PetProgress {
            percent: 100.0,
            label: format!("{level} · max"),
            complete: true,
        };
    }
    PetProgress {
        percent: (f64::from(pet.souls) / f64::from(pet.goal)).min(1.0) * 100.0,
        label: format!("{level} · {}/{}", pet.souls, pet.goal),
        complete: pet.souls >= pet.goal,
    }
}

fn toggle_pin(pinned: RwSignal<Vec<u32>>, id: u32) {
    pinned.update(|ids| {
        if let Some(index) = ids.iter().position(|&pinned_id| pinned_id == id) {
            ids.remove(index);
        } else {
            ids.insert(0, id);
        }
    });
}

fn pet_entry(pet: PetView, pinned: RwSignal<Vec<u32>>) -> impl IntoView {
    let progress = pet_progress(&pet);
    let gained = (pet.gained > 0).then(|| format!("+{}", pet.gained));
    let name = pet.name.unwrap_or_else(|| format!("Pet {}", pet.id));
    let id = pet.id;
    let pin_title = if pet.pinned {
        tr("Unpin", "Odepnij")
    } else {
        tr("Pin to top", "Przypnij na górze")
    };
    view! {
        <div class="pet" class:done=progress.complete class:pinned=pet.pinned title=format!("pet {id}")>
            <div class="pet-bar" style=format!("width:{:.1}%", progress.percent)></div>
            <button class="pet-pin" class:on=pet.pinned title=pin_title on:click=move |_| toggle_pin(pinned, id)>
                {if pet.pinned { PINNED_ICON } else { UNPINNED_ICON }}
            </button>
            <span class="pet-name">{name}</span>
            <span class="pet-gained">{gained}</span>
            <span class="pet-count">{progress.label}</span>
        </div>
    }
}

#[component]
pub fn PetsPanel(
    pets: Signal<Vec<PetView>>,
    loaded: Signal<bool>,
    pinned: RwSignal<Vec<u32>>,
) -> impl IntoView {
    let relog_banner = move || {
        (!loaded.get()).then(|| {
            view! {
                <p class="pets-warn">
                    {tr("Relog your character to load pet progress.", "Przeloguj postać, żeby wczytać postęp petów.")}
                </p>
            }
        })
    };

    let pet_list = move || {
        let pets = pets.get();
        if pets.is_empty() {
            return view! {
                <p class="pets-empty">{tr("Pick up a pet soul to see it here.", "Podnieś duszę peta, żeby go tu zobaczyć.")}</p>
            }
            .into_any();
        }
        pets.into_iter()
            .map(|pet| pet_entry(pet, pinned))
            .collect_view()
            .into_any()
    };

    view! {
        <div class="pets">
            {relog_banner}
            {pet_list}
        </div>
    }
}
