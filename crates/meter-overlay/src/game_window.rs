use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use meter_capture::locator::is_game_process;
use tauri::{AppHandle, Manager, WebviewWindow};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, EnumWindows,
    GetForegroundWindow, GetWindowThreadProcessId, IsWindow, IsWindowVisible, OBJID_WINDOW,
    SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

const CHILDID_SELF: i32 = 0;
const CONTINUE_ENUMERATION: i32 = 1;
const STOP_ENUMERATION: i32 = 0;

static APP: OnceLock<AppHandle> = OnceLock::new();
static HIDDEN_FOR_GAME: AtomicBool = AtomicBool::new(false);
static HIDE_WHEN_GAME_UNFOCUSED: AtomicBool = AtomicBool::new(true);
static GAME_HWND: AtomicIsize = AtomicIsize::new(0);
static GAME_PID: AtomicU32 = AtomicU32::new(0);

fn running_processes() -> Vec<(u32, String)> {
    let mut processes = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return processes;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut has_entry = Process32FirstW(snapshot, &mut entry) != 0;
        while has_entry {
            let name_len = entry
                .szExeFile
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(entry.szExeFile.len());
            processes.push((
                entry.th32ProcessID,
                String::from_utf16_lossy(&entry.szExeFile[..name_len]),
            ));
            has_entry = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    processes
}

fn game_pids() -> Vec<u32> {
    running_processes()
        .into_iter()
        .filter(|(_, name)| is_game_process(name))
        .map(|(pid, _)| pid)
        .collect()
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

fn is_game_window(hwnd: HWND) -> bool {
    if hwnd.is_null() {
        return false;
    }
    let pid = window_pid(hwnd);
    if pid == 0 {
        return false;
    }
    if pid == GAME_PID.load(Ordering::Relaxed) {
        return true;
    }
    let is_game = game_pids().contains(&pid);
    if is_game {
        GAME_PID.store(pid, Ordering::Relaxed);
    }
    is_game
}

struct GameWindowSearch {
    pids: Vec<u32>,
    found: HWND,
}

unsafe extern "system" fn check_game_window(hwnd: HWND, search: LPARAM) -> i32 {
    let search = unsafe { &mut *(search as *mut GameWindowSearch) };
    if search.pids.contains(&window_pid(hwnd)) && unsafe { IsWindowVisible(hwnd) } != 0 {
        search.found = hwnd;
        return STOP_ENUMERATION;
    }
    CONTINUE_ENUMERATION
}

fn find_game_window() -> Option<HWND> {
    let mut search = GameWindowSearch {
        pids: game_pids(),
        found: std::ptr::null_mut(),
    };
    if search.pids.is_empty() {
        return None;
    }
    unsafe {
        EnumWindows(
            Some(check_game_window),
            &mut search as *mut GameWindowSearch as LPARAM,
        )
    };
    (!search.found.is_null()).then_some(search.found)
}

fn remember_game_window(hwnd: HWND) {
    GAME_HWND.store(hwnd as isize, Ordering::Relaxed);
}

fn game_window_exists() -> bool {
    let hwnd = GAME_HWND.load(Ordering::Relaxed);
    hwnd != 0 && unsafe { IsWindow(hwnd as HWND) } != 0
}

fn native_handle(window: &WebviewWindow) -> Option<HWND> {
    window.hwnd().ok().map(|handle| handle.0 as HWND)
}

fn hide_all(app: &AppHandle) {
    let windows = app.webview_windows();
    if !windows
        .values()
        .any(|window| window.is_visible().unwrap_or(false))
    {
        return;
    }
    HIDDEN_FOR_GAME.store(true, Ordering::Relaxed);
    for window in windows.values() {
        if let Some(hwnd) = native_handle(window) {
            unsafe { ShowWindow(hwnd, SW_HIDE) };
        }
    }
}

fn show_all(app: &AppHandle) {
    if !HIDDEN_FOR_GAME.swap(false, Ordering::Relaxed) {
        return;
    }
    for window in app.webview_windows().values() {
        if let Some(hwnd) = native_handle(window) {
            unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        }
    }
}

pub fn set_follow_focus(app: &AppHandle, enabled: bool) {
    HIDE_WHEN_GAME_UNFOCUSED.store(enabled, Ordering::Relaxed);
    if enabled {
        on_foreground(app, unsafe { GetForegroundWindow() });
    } else {
        show_all(app);
    }
}

fn on_foreground(app: &AppHandle, hwnd: HWND) {
    if window_pid(hwnd) == std::process::id() {
        return;
    }
    if is_game_window(hwnd) {
        remember_game_window(hwnd);
        show_all(app);
    } else if HIDE_WHEN_GAME_UNFOCUSED.load(Ordering::Relaxed) {
        if game_window_exists() {
            hide_all(app);
        } else {
            show_all(app);
        }
    }
}

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if id_object != OBJID_WINDOW || id_child != CHILDID_SELF {
        return;
    }
    let Some(app) = APP.get() else {
        return;
    };
    match event {
        EVENT_SYSTEM_FOREGROUND => on_foreground(app, hwnd),
        EVENT_SYSTEM_MINIMIZESTART if is_game_window(hwnd) => {
            remember_game_window(hwnd);
            hide_all(app);
        }
        EVENT_SYSTEM_MINIMIZEEND
            if is_game_window(hwnd) && unsafe { GetForegroundWindow() } == hwnd =>
        {
            show_all(app);
        }
        _ => {}
    }
}

fn watch_events(first_event: u32, last_event: u32) -> bool {
    let hook = unsafe {
        SetWinEventHook(
            first_event,
            last_event,
            std::ptr::null_mut(),
            Some(on_win_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    !hook.is_null()
}

pub fn follow_game(app: &AppHandle) {
    if APP.set(app.clone()).is_err() {
        return;
    }
    if !watch_events(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND)
        || !watch_events(EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND)
    {
        eprintln!("could not watch the game window");
    }
    if let Some(hwnd) = find_game_window() {
        remember_game_window(hwnd);
        GAME_PID.store(window_pid(hwnd), Ordering::Relaxed);
    }
    on_foreground(app, unsafe { GetForegroundWindow() });
}
