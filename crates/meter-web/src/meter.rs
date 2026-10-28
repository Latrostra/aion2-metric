use leptos::prelude::*;
use leptos::task::spawn_local;
use meter_proto::Snapshot;

use crate::tauri;

const SNAPSHOT_EVENT: &str = "snapshot";

pub fn reset_meter() {
    tauri::reset_meter();
}

pub fn send_idle_secs(secs: u32) {
    tauri::set_idle_secs(secs);
}

pub fn send_recent_secs(secs: u32) {
    tauri::set_recent_secs(secs);
}

pub fn send_pinned_pets(ids: &[u32]) {
    tauri::set_pinned_pets(ids);
}

pub fn connect(set_snapshot: WriteSignal<Option<Snapshot>>, set_online: WriteSignal<bool>) {
    let show = move |snapshot: Snapshot| {
        set_online.set(true);
        set_snapshot.set(Some(snapshot));
    };
    tauri::listen_json(SNAPSHOT_EVENT, show);
    spawn_local(async move {
        if let Some(snapshot) = tauri::snapshot().await {
            show(snapshot);
        }
    });
}
