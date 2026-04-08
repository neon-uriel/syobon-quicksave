#![windows_subsystem = "windows"]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

// ─── Win32 型 ─────────────────────────────────────────
type HWND     = isize;
type HINSTANCE= isize;
type HMENU    = isize;
type HICON    = isize;
type HCURSOR  = isize;
type HBRUSH   = isize;
type WPARAM   = usize;
type LPARAM   = isize;
type LRESULT  = isize;
type WNDPROC  = Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>;

#[repr(C)]
struct WNDCLASSEXW {
    cb_size:         u32,
    style:           u32,
    lpfn_wnd_proc:   WNDPROC,
    cb_cls_extra:    i32,
    cb_wnd_extra:    i32,
    h_instance:      HINSTANCE,
    h_icon:          HICON,
    h_cursor:        HCURSOR,
    hbr_background:  HBRUSH,
    lpsz_menu_name:  *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm:       HICON,
}

#[repr(C)]
struct MSG {
    hwnd:    HWND,
    message: u32,
    wparam:  WPARAM,
    lparam:  LPARAM,
    time:    u32,
    pt_x:    i32,
    pt_y:    i32,
}

// ─── Win32 定数 ──────────────────────────────────────
const WS_OVERLAPPED:    u32 = 0x00000000;
const WS_CAPTION:       u32 = 0x00C00000;
const WS_SYSMENU:       u32 = 0x00080000;
const WS_CHILD:         u32 = 0x40000000;
const WS_VISIBLE:       u32 = 0x10000000;
const WS_EX_CLIENTEDGE: u32 = 0x00000200;
const CS_HREDRAW:       u32 = 0x0002;
const CS_VREDRAW:       u32 = 0x0001;
const SS_LEFT:          u32 = 0x00000000;
const SS_CENTER:        u32 = 0x00000001;
const ES_CENTER:        u32 = 0x0001;
const ES_READONLY:      u32 = 0x0800;
const BS_PUSHBUTTON:    u32 = 0x00000000;
const BS_DEFPUSHBUTTON: u32 = 0x00000001;
const COLOR_BTNFACE:    u32 = 15;
const IDC_ARROW:        usize = 32512;
const SW_SHOW:          i32 = 5;
const SM_CXSCREEN:      i32 = 0;
const SM_CYSCREEN:      i32 = 1;
const WM_CREATE:        u32 = 0x0001;
const WM_DESTROY:       u32 = 0x0002;
const WM_COMMAND:       u32 = 0x0111;
const WM_TIMER:         u32 = 0x0113;

// ─── Win32 API ───────────────────────────────────────
#[link(name = "user32")]
extern "system" {
    fn RegisterClassExW(lpwndclassex: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        dwexstyle: u32, lpclassname: *const u16, lpwindowname: *const u16,
        dwstyle: u32, x: i32, y: i32, nwidth: i32, nheight: i32,
        hwndparent: HWND, hmenu: HMENU, hinstance: HINSTANCE,
        lpparam: *const core::ffi::c_void,
    ) -> HWND;
    fn DefWindowProcW(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
    fn GetMessageW(lpmsg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
    fn TranslateMessage(lpmsg: *const MSG) -> i32;
    fn DispatchMessageW(lpmsg: *const MSG) -> LRESULT;
    fn PostQuitMessage(nexitcode: i32);
    fn ShowWindow(hwnd: HWND, ncmdshow: i32) -> i32;
    fn UpdateWindow(hwnd: HWND) -> i32;
    fn SetWindowTextW(hwnd: HWND, lpstring: *const u16) -> i32;
    fn EnableWindow(hwnd: HWND, benable: i32) -> i32;
    fn SetTimer(hwnd: HWND, nidevent: usize, uelapse: u32,
        lptimerfunc: Option<unsafe extern "system" fn(HWND, u32, usize, u32)>) -> usize;
    fn KillTimer(hwnd: HWND, nidevent: usize) -> i32;
    fn GetSystemMetrics(nindex: i32) -> i32;
    fn LoadCursorW(hinstance: HINSTANCE, lpcursorname: usize) -> HCURSOR;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(lpmodulename: *const u16) -> HINSTANCE;
}

// ─── ヘルパー ─────────────────────────────────────────
fn w(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn vk_name(vk: u32) -> String {
    match vk {
        0x70..=0x7B => format!("F{}", vk - 0x6F),
        0x41..=0x5A => format!("{}", (b'A' + (vk - 0x41) as u8) as char),
        0x30..=0x39 => format!("{}", vk - 0x30),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x20 => "Space".into(),
        0x0D => "Enter".into(),
        0x1B => "Esc".into(),
        0x09 => "Tab".into(),
        0x08 => "BackSpace".into(),
        0x2E => "Delete".into(),
        0x2D => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0x25 => "←".into(),
        0x26 => "↑".into(),
        0x27 => "→".into(),
        0x28 => "↓".into(),
        _ => format!("VK 0x{:02X}", vk),
    }
}

// ─── config ──────────────────────────────────────────
fn config_path() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    p.push("quicksave.cfg");
    p
}

fn read_config() -> (u32, u32) {
    let mut save: u32 = 0x74;
    let mut load: u32 = 0x78;
    if let Ok(text) = std::fs::read_to_string(config_path()) {
        for line in text.lines() {
            let line = line.trim();
            if let Some(v) = line.strip_prefix("save=") {
                if let Ok(n) = u32::from_str_radix(v.trim(), 16) { save = n; }
            } else if let Some(v) = line.strip_prefix("load=") {
                if let Ok(n) = u32::from_str_radix(v.trim(), 16) { load = n; }
            }
        }
    }
    (save, load)
}

fn write_config(save: u32, load: u32) {
    let s = format!(
        "# しょぼんのアクション クイックセーブ設定\nsave={:02X}\nload={:02X}\n",
        save, load
    );
    std::fs::write(config_path(), s.as_bytes()).ok();
}

// ─── グローバル状態 ──────────────────────────────────
const ID_BTN_SAVE:    usize = 103;
const ID_BTN_LOAD:    usize = 104;
const ID_BTN_OK:      usize = 105;
const ID_BTN_CANCEL:  usize = 106;
const ID_TIMER:       usize = 1;

static mut G_SAVE_VK:      u32  = 0x74;
static mut G_LOAD_VK:      u32  = 0x78;
static mut G_WAITING:      u32  = 0;   // 0=なし 1=セーブ待ち 2=ロード待ち
static mut G_EDIT_SAVE:    HWND = 0;
static mut G_EDIT_LOAD:    HWND = 0;
static mut G_LABEL_STATUS: HWND = 0;
static mut G_BTN_SAVE:     HWND = 0;
static mut G_BTN_LOAD:     HWND = 0;

unsafe fn update_edits() {
    SetWindowTextW(G_EDIT_SAVE, w(&vk_name(G_SAVE_VK)).as_ptr());
    SetWindowTextW(G_EDIT_LOAD, w(&vk_name(G_LOAD_VK)).as_ptr());
}

unsafe fn end_wait(hwnd: HWND) {
    KillTimer(hwnd, ID_TIMER);
    SetWindowTextW(G_LABEL_STATUS, w("").as_ptr());
    EnableWindow(G_BTN_SAVE, 1);
    EnableWindow(G_BTN_LOAD, 1);
    G_WAITING = 0;
}

// ─── ウィンドウプロシージャ ──────────────────────────
unsafe extern "system" fn wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let hi = GetModuleHandleW(std::ptr::null());

            // セーブキー行
            CreateWindowExW(0, w("STATIC").as_ptr(), w("セーブキー:").as_ptr(),
                WS_CHILD | WS_VISIBLE | SS_LEFT,
                12, 18, 82, 20, hwnd, 0, hi, std::ptr::null());
            G_EDIT_SAVE = CreateWindowExW(WS_EX_CLIENTEDGE,
                w("EDIT").as_ptr(), std::ptr::null(),
                WS_CHILD | WS_VISIBLE | ES_READONLY | ES_CENTER,
                98, 15, 110, 24, hwnd, 0, hi, std::ptr::null());
            G_BTN_SAVE = CreateWindowExW(0, w("BUTTON").as_ptr(), w("変更").as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON,
                218, 14, 62, 26, hwnd, ID_BTN_SAVE as HMENU, hi, std::ptr::null());

            // ロードキー行
            CreateWindowExW(0, w("STATIC").as_ptr(), w("ロードキー:").as_ptr(),
                WS_CHILD | WS_VISIBLE | SS_LEFT,
                12, 56, 82, 20, hwnd, 0, hi, std::ptr::null());
            G_EDIT_LOAD = CreateWindowExW(WS_EX_CLIENTEDGE,
                w("EDIT").as_ptr(), std::ptr::null(),
                WS_CHILD | WS_VISIBLE | ES_READONLY | ES_CENTER,
                98, 53, 110, 24, hwnd, 0, hi, std::ptr::null());
            G_BTN_LOAD = CreateWindowExW(0, w("BUTTON").as_ptr(), w("変更").as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON,
                218, 52, 62, 26, hwnd, ID_BTN_LOAD as HMENU, hi, std::ptr::null());

            // ステータス
            G_LABEL_STATUS = CreateWindowExW(0, w("STATIC").as_ptr(), w("").as_ptr(),
                WS_CHILD | WS_VISIBLE | SS_CENTER,
                12, 90, 272, 18, hwnd, 0, hi, std::ptr::null());

            // OK / キャンセル
            CreateWindowExW(0, w("BUTTON").as_ptr(), w("OK").as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_DEFPUSHBUTTON,
                62, 116, 80, 28, hwnd, ID_BTN_OK as HMENU, hi, std::ptr::null());
            CreateWindowExW(0, w("BUTTON").as_ptr(), w("キャンセル").as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON,
                154, 116, 80, 28, hwnd, ID_BTN_CANCEL as HMENU, hi, std::ptr::null());

            update_edits();
            0
        }

        WM_COMMAND => {
            let id = wparam & 0xFFFF;
            match id {
                ID_BTN_SAVE => {
                    G_WAITING = 1;
                    EnableWindow(G_BTN_SAVE, 0);
                    EnableWindow(G_BTN_LOAD, 0);
                    SetWindowTextW(G_LABEL_STATUS,
                        w("セーブキーを押してください… (Esc でキャンセル)").as_ptr());
                    SetTimer(hwnd, ID_TIMER, 16, None);
                }
                ID_BTN_LOAD => {
                    G_WAITING = 2;
                    EnableWindow(G_BTN_SAVE, 0);
                    EnableWindow(G_BTN_LOAD, 0);
                    SetWindowTextW(G_LABEL_STATUS,
                        w("ロードキーを押してください… (Esc でキャンセル)").as_ptr());
                    SetTimer(hwnd, ID_TIMER, 16, None);
                }
                ID_BTN_OK => {
                    write_config(G_SAVE_VK, G_LOAD_VK);
                    PostQuitMessage(0);
                }
                ID_BTN_CANCEL => PostQuitMessage(0),
                _ => {}
            }
            0
        }

        WM_TIMER => {
            if G_WAITING != 0 {
                if GetAsyncKeyState(0x1B) as u16 & 0x8000 != 0 {
                    // Esc キャンセル
                    end_wait(hwnd);
                } else {
                    for vk in 1u32..=0xFE {
                        if matches!(vk,
                            0x01..=0x06 |  // マウスボタン
                            0x10..=0x12 |  // Shift, Ctrl, Alt
                            0x14 |         // CapsLock
                            0x15..=0x1A |  // IME (Kana, Junja, Final, Hanja, Kanji, IME-off)
                            0x1B |         // Esc (キャンセル用)
                            0x5B | 0x5C |  // Windows キー
                            0x90 | 0x91    // NumLock, ScrollLock
                        ) { continue; }
                        if GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 {
                            if G_WAITING == 1 { G_SAVE_VK = vk; }
                            else              { G_LOAD_VK = vk; }
                            end_wait(hwnd);
                            update_edits();
                            break;
                        }
                    }
                }
            }
            0
        }

        WM_DESTROY => { PostQuitMessage(0); 0 }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ─── main ────────────────────────────────────────────
fn main() {
    unsafe {
        let (sv, lv) = read_config();
        G_SAVE_VK = sv;
        G_LOAD_VK = lv;

        let hi        = GetModuleHandleW(std::ptr::null());
        let cls_name  = w("QSConfig");
        let cursor    = LoadCursorW(0, IDC_ARROW);

        let wc = WNDCLASSEXW {
            cb_size:         std::mem::size_of::<WNDCLASSEXW>() as u32,
            style:           CS_HREDRAW | CS_VREDRAW,
            lpfn_wnd_proc:   Some(wnd_proc),
            cb_cls_extra:    0,
            cb_wnd_extra:    0,
            h_instance:      hi,
            h_icon:          0,
            h_cursor:        cursor,
            hbr_background:  (COLOR_BTNFACE + 1) as HBRUSH,
            lpsz_menu_name:  std::ptr::null(),
            lpsz_class_name: cls_name.as_ptr(),
            h_icon_sm:       0,
        };
        RegisterClassExW(&wc);

        let win_w = 310;
        let win_h = 188;
        let sx = GetSystemMetrics(SM_CXSCREEN);
        let sy = GetSystemMetrics(SM_CYSCREEN);

        let hwnd = CreateWindowExW(
            0,
            cls_name.as_ptr(),
            w("クイックセーブ設定").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            (sx - win_w) / 2, (sy - win_h) / 2, win_w, win_h,
            0, 0, hi, std::ptr::null(),
        );

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
