# AGENTS.md

Guide for coding agents working on **aion2-metric**: a passive DPS meter and pet-soul tracker
for AION 2 (EU, Windows). Rust everywhere: Npcap capture, local agent, Leptos UI, Tauri v2 overlay.
License MIT. Owner: Latrostra (Hubert). He writes casual Polish; answer him in Polish, short
and direct. He wants things working right away and verified live, not long plans.

## Hard rules

- **No comments in code.** Not in `.rs`, `.css`, `.toml`, `.html`. Code must explain itself
  through names. Do not add any.
- **Passive only.** Never read game memory, never inject, never send packets to the game,
  never bypass anti-cheat, never break encryption.
- **Overlay only, no server.** The app is only the Tauri overlay. No HTTP/WebSocket server,
  no open ports, no browser mode. UI and overlay talk only through Tauri events and commands.
- **Licensing.** Everything is MIT. Never copy code or data from GPL projects. Fan databases
  (aion2.th.gl, talentbuilds, MikeZeen/aion2-pet-tracker) are reference only; nothing from them
  is committed to the repo.
- **No NCSOFT data in the repo** (item prices etc.).
- **`.pcap` files never go to git** (`captures/` is ignored). They contain real nicknames.
  In test fixtures replace nicknames with fake ones of the **same byte length**.
- **No nickname cache.** Entity IDs change between sessions. Names are filled only from
  packets of the current session; unknown name shows `…`.
- **No pet persistence on disk.** Pet progress comes only from the game's login packet.
  If it was not seen (overlay started after login), the UI tells the user to relog. Pet names
  are downloaded at start into memory only. Do not add any save/load module for pets.
- **Simple and immediate.** No artificial delays, debouncing "just in case", or extra
  mechanisms nobody asked for. Window position is saved immediately on change.
- **UI text is short and instructional.** Tell the user what to do, never explain why.
  English by default, Polish translation via `tr("English", "Polski")` (`meter-web/src/i18n.rs`).
  Every new user-facing string goes through `tr`.

## Layout

| crate | role |
|---|---|
| `meter-proto` | JSON types shared by agent and UI (`Snapshot`, `Status`, `EncounterView`, `ActorRow`, `PetView`) |
| `meter-core` | TCP reassembly, framing, `opcodes.rs`, `parser.rs` (→ `GameEvent`), `combat.rs` (`Meter`) |
| `meter-capture` | Npcap capture, game connection locator, pcap read/write |
| `meter-agent` | `engine.rs` (in-process meter: source worker, state, throttled snapshot publishing), `a2m` CLI tools, `pets.rs`, `pet_names.rs` |
| `meter-web` | Leptos CSR UI (trunk) of the overlay windows; talks to the overlay only through Tauri (`snapshot` event + commands) |
| `meter-overlay` | Tauri v2 overlay `a2m-overlay.exe`: runs the agent in-process, windows, tray, hotkey, docking |

## Build, test, run

```
cargo test --workspace --exclude meter-web
cargo clippy --workspace --exclude meter-web --all-targets -- -D warnings
cargo clippy -p meter-web --target wasm32-unknown-unknown -- -D warnings
cargo fmt --all

cd crates/meter-web && trunk build --release && cd ../..
touch crates/meter-overlay/build.rs
cargo build --release -p meter-overlay -p meter-agent
target/release/a2m-overlay.exe
```

- The overlay embeds `meter-web/dist` at compile time: after UI changes run trunk **and**
  touch `meter-overlay/build.rs`, otherwise the old UI is shipped.
- Kill the overlay before rebuilding (`taskkill /F /IM a2m-overlay.exe`, single slash in
  git-bash). A running `a2m live --record` locks `target/release/a2m.exe`; use the debug build then.
- Do not chain `git commit` with `&&` after a piped `cargo clippy ... | tail` (the pipe hides failures).
- Before committing: tests, both clippy runs, fmt. All must be clean.

## Protocol notes

- Local client `192.168.1.39`, game servers `193.202.112.x:13328`.
- Opcodes (`meter-core/src/opcodes.rs`): Heartbeat `0036`, Damage `0438`, DoT `0538`,
  Cast `0238`, RemainHp `008D`, MapLoad `2136`, Spawn `4136`, Death `4236`, SelfInfo `3336`/`4A36`,
  PlayerInfo `4536`/`4436`, party `x97` (`0297` roster, `0B97` join, `1697` member left,
  `1D97` left, `1F97` update), PetCollection `0090`, PetSouls `0D90`. Unparsed: equipment `1156`,
  stats `8356`/`8456`.
- Players: skill IDs `10_000_000..=19_999_999`; NPC skills have 7 digits. DoT is attributed
  only to a skill of the same family within 60 s. Summons are folded into their owner in views.
- Party: `0297` with `b[4] == 4` is the real roster (others are group-finder ads). Character
  IDs are stable across zones.
- Pets: `0090` is the full collection, sent **only at character login** (not on teleports,
  not when opening the pet window). `0D90` = picked souls `(pet id, amount)`. Pet IDs
  `1002..=1219`, e.g. Black Spirit = `1214`. Goals: level 0→5, 1→25, 2→75, level 3 = max.
  Packets carry IDs only; names come from the downloaded pet list.
- Daily limits: `a5ff` (login) / `a6ff` (update) = kinah gained today, `a7ff` (login) /
  `a8ff` (update) = gathered today; both are u64 value + u64 timestamp. `abff` (login and every
  gain) = abyss points u64 current + u64 limit. The kinah (1,000,000) and gathering (1,000) limits are
  never sent by the server (checked over a full login), so they are constants in `engine.rs`.
- Kinah balance: kinah is inventory item template `0x3770333F`. `1b56` (item update) has the
  template at offset 11 followed by the u64 amount; `1156` (inventory at login) has the same
  record. There is no separate loot amount packet, so earned kinah is the sum of balance gains.
- Record new captures with `a2m live --record captures/<name>.pcap --seconds N --quiet`;
  inspect with `a2m dump`, `a2m events`, `a2m stats`, `a2m fights`.

## Meter semantics

- Default mode "whole map": totals until map change (map id), death does not reset; ↻ resets.
  "Per fight" mode splits on a gap longer than `idle_secs` (3–120 s, default 15).
- Three DPS values: **average** = damage / fight time (main, ACT-style EncDPS),
  **rolling** = last N seconds (3–60, default 10), **active** = clock runs hit to hit and stops on
  the last hit; in map mode gaps longer than idle are skipped.
- Pets list: pets with souls picked this session (newest first) + pinned pets on top.
  Pins live in UI `localStorage` (`a2m.pinned_pets`) and are pushed to the agent.

## Overlay and UI conventions

- The whole `.meter` panel is the window drag region. In overlay mode
  `.overlay .meter *` has `pointer-events: none`; **every new clickable element must be added
  to the `pointer-events: auto` list in `style.css`**, or clicks will drag the window instead.
- Main window `main` is the parent; panels (`dps`, `pets`) are tabs. A tab dragged out becomes
  window `panel-<name>`. Dropped within 16 px of another window's edge it docks (snap happens
  only after mouse release, never during the drag); docked chains move with their parent.
  Dropped over the parent's tab bar it becomes a tab again. Layout is in `layout.json`,
  main window position in `window.json` (`%APPDATA%\dev.latrostra.aion2metric\`).
  Do not use the Tauri window-state plugin (it deadlocked while dragging).
- UI prefs live in `localStorage` under `a2m.*`. Panel windows reload on `storage` events.
- Settings: one row per option (`srow`), segmented buttons (`seg`), switches (`switch`),
  sliders (`range_u32`). No explanatory hints.

## Testing the overlay live

- Debug webviews with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222"`
  and drive them over CDP (`Runtime.evaluate`).
- Back up and restore the user's settings around tests: `localStorage` `a2m.*`,
  `window.json`, `layout.json`.
- Never move the real mouse unless the target window was found. Move windows with
  `SetWindowPos` instead. In PowerShell name the handle `$hwnd` (`$H`/`$h` collide).
- Simulated key presses do not trigger global shortcuts; the user tests hotkeys himself.
- Things that need a real mouse (tab tear-off gesture, drop highlight) must be confirmed by
  the user; say so explicitly instead of claiming they work.
