use std::sync::atomic::{AtomicI32, AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::{BOOL, HMODULE};
use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_F5, VK_F9,
};

// ─── ゲームメモリアドレス ─────────────────────────────
const REGION1_START:   usize = 0x5A1000;
const REGION1_SIZE:    usize = 0x1000;
const REGION2_START:   usize = 0x87A000;
const REGION2_SIZE:    usize = 0x16000;
const GAME_STATE_ADDR: usize = 0x5A1274;
const VEL_X_ADDR:      usize = 0x87A544;
const VEL_Y_ADDR:      usize = 0x880B24;
const POS_X_ADDR:      usize = 0x8806F4;
const POS_Y_ADDR:      usize = 0x88AD7C;

// ─── Win32 型エイリアス ───────────────────────────────
type HANDLE = isize;
type HWND   = isize;
type HDC    = isize;
type HFONT  = isize;
type LPTHREAD_START_ROUTINE =
    Option<unsafe extern "system" fn(*mut core::ffi::c_void) -> u32>;

// ─── kernel32 ────────────────────────────────────────
#[link(name = "kernel32")]
extern "system" {
    fn CreateThread(
        lpthreadattributes: *const core::ffi::c_void,
        dwstacksize: usize,
        lpstartaddress: LPTHREAD_START_ROUTINE,
        lpparameter: *const core::ffi::c_void,
        dwcreationflags: u32,
        lpthreadid: *mut u32,
    ) -> HANDLE;
    fn Sleep(dwmilliseconds: u32);
    fn Beep(dwfreq: u32, dwduration: u32) -> BOOL;
    fn GetModuleFileNameW(hmodule: HANDLE, lpfilename: *mut u16, nsize: u32) -> u32;
    fn GetCurrentProcessId() -> u32;
}

// ─── user32 ──────────────────────────────────────────
#[link(name = "user32")]
extern "system" {
    fn EnumWindows(
        lpenumfunc: Option<unsafe extern "system" fn(HWND, isize) -> i32>,
        lparam: isize,
    ) -> i32;
    fn GetWindowThreadProcessId(hwnd: HWND, lpdwprocessid: *mut u32) -> u32;
    fn IsWindowVisible(hwnd: HWND) -> i32;
    fn GetDC(hwnd: HWND) -> HDC;
    fn ReleaseDC(hwnd: HWND, hdc: HDC) -> i32;
}

// ─── gdi32 ───────────────────────────────────────────
#[link(name = "gdi32")]
extern "system" {
    fn SetBkMode(hdc: HDC, ibbkmode: i32) -> i32;
    fn SetTextColor(hdc: HDC, color: u32) -> u32;
    fn TextOutW(hdc: HDC, x: i32, y: i32, lpstring: *const u16, c: i32) -> i32;
    fn CreateFontW(
        cheight: i32, cwidth: i32, cescapement: i32, corientation: i32,
        cweight: i32, bitalic: u32, bunderline: u32, bstrikeout: u32,
        icharset: u32, ioutprecision: u32, iclipprecision: u32,
        iquality: u32, ipitchandfamily: u32, pszfacename: *const u16,
    ) -> HFONT;
    fn SelectObject(hdc: HDC, hobject: isize) -> isize;
    fn DeleteObject(hobject: isize) -> i32;
}

// ─── グローバル状態 ───────────────────────────────────
static SAVE_VK:     AtomicI32   = AtomicI32::new(VK_F5 as i32);
static LOAD_VK:     AtomicI32   = AtomicI32::new(VK_F9 as i32);
static DLL_HMODULE: AtomicIsize = AtomicIsize::new(0);
static GAME_HWND:   AtomicIsize = AtomicIsize::new(0);
static CACHED_FONT: AtomicIsize = AtomicIsize::new(0);

static mut SAVE_REGION1: [u8; REGION1_SIZE] = [0u8; REGION1_SIZE];
static mut SAVE_REGION2: [u8; REGION2_SIZE] = [0u8; REGION2_SIZE];
static mut HAS_SAVE: bool = false;

// ─── HWND 検索 ────────────────────────────────────────
static FOUND_HWND: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn enum_proc(hwnd: HWND, _: isize) -> i32 {
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == GetCurrentProcessId() && IsWindowVisible(hwnd) != 0 {
        FOUND_HWND.store(hwnd, Ordering::Relaxed);
        return 0; // 列挙停止
    }
    1
}

unsafe fn find_game_hwnd() -> HWND {
    FOUND_HWND.store(0, Ordering::Relaxed);
    EnumWindows(Some(enum_proc), 0);
    FOUND_HWND.load(Ordering::Relaxed)
}

// ─── velocity オーバーレイ描画 ────────────────────────
const FONT_FACE: &[u16] = &[
    b'C' as u16, b'o' as u16, b'u' as u16, b'r' as u16, b'i' as u16,
    b'e' as u16, b'r' as u16, b' ' as u16, b'N' as u16, b'e' as u16,
    b'w' as u16, 0,
];

unsafe fn get_font() -> HFONT {
    let cached = CACHED_FONT.load(Ordering::Relaxed);
    if cached != 0 { return cached; }
    let font = CreateFontW(
        16, 0, 0, 0,
        700,  // FW_BOLD
        0, 0, 0,
        0,    // DEFAULT_CHARSET
        0, 0,
        2,    // PROOF_QUALITY
        0x31, // FIXED_PITCH | FF_MODERN
        FONT_FACE.as_ptr(),
    );
    CACHED_FONT.store(font, Ordering::Relaxed);
    font
}

unsafe fn draw_velocity() {
    let hwnd = GAME_HWND.load(Ordering::Relaxed);
    if hwnd == 0 { return; }

    let vel_x = *(VEL_X_ADDR as *const i32);
    let vel_y = *(VEL_Y_ADDR as *const i32);
    let pos_x = *(POS_X_ADDR as *const i32);
    let pos_y = *(POS_Y_ADDR as *const i32);

    let vel_text: Vec<u16> = format!("vX:{:+5} vY:{:+5}", vel_x, vel_y).encode_utf16().collect();
    let pos_text: Vec<u16> = format!("pX:{:+5} pY:{:+5}", pos_x, pos_y).encode_utf16().collect();

    let hdc = GetDC(hwnd);
    if hdc == 0 { return; }

    let font = get_font();
    let old_obj = SelectObject(hdc, font);
    SetBkMode(hdc, 1); // TRANSPARENT

    // velocity: 黄色 (BGR: 0x0000FFFF)
    SetTextColor(hdc, 0x00000000);
    TextOutW(hdc, 297, 5, vel_text.as_ptr(), vel_text.len() as i32);
    SetTextColor(hdc, 0x0000FFFF);
    TextOutW(hdc, 296, 4, vel_text.as_ptr(), vel_text.len() as i32);

    // position: シアン (BGR: 0x00FFFF00)
    SetTextColor(hdc, 0x00000000);
    TextOutW(hdc, 297, 21, pos_text.as_ptr(), pos_text.len() as i32);
    SetTextColor(hdc, 0x00FFFF00);
    TextOutW(hdc, 296, 20, pos_text.as_ptr(), pos_text.len() as i32);

    SelectObject(hdc, old_obj);
    ReleaseDC(hwnd, hdc);
}

// ─── config ───────────────────────────────────────────
fn dll_dir() -> Option<std::path::PathBuf> {
    let hmod = DLL_HMODULE.load(Ordering::Relaxed);
    let mut buf = [0u16; 512];
    let len = unsafe { GetModuleFileNameW(hmod, buf.as_mut_ptr(), buf.len() as u32) };
    if len == 0 { return None; }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    std::path::Path::new(&path).parent().map(|p| p.to_path_buf())
}

fn load_config() {
    let Some(dir) = dll_dir() else { return };
    let Ok(content) = std::fs::read_to_string(dir.join("quicksave.cfg")) else { return };
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() { continue; }
        if let Some(rest) = line.strip_prefix("save=") {
            if let Ok(vk) = u32::from_str_radix(rest.trim(), 16) {
                SAVE_VK.store(vk as i32, Ordering::Relaxed);
            }
        } else if let Some(rest) = line.strip_prefix("load=") {
            if let Ok(vk) = u32::from_str_radix(rest.trim(), 16) {
                LOAD_VK.store(vk as i32, Ordering::Relaxed);
            }
        }
    }
}

// ─── セーブ / ロード ──────────────────────────────────
unsafe fn do_save() {
    core::ptr::copy_nonoverlapping(
        REGION1_START as *const u8, SAVE_REGION1.as_mut_ptr(), REGION1_SIZE,
    );
    core::ptr::copy_nonoverlapping(
        REGION2_START as *const u8, SAVE_REGION2.as_mut_ptr(), REGION2_SIZE,
    );
    HAS_SAVE = true;
    Beep(1000, 80);
}

unsafe fn do_load() {
    if !HAS_SAVE { Beep(300, 150); return; }
    let game_state = *(GAME_STATE_ADDR as *const i32);
    if game_state != 1 { return; }
    core::ptr::copy_nonoverlapping(
        SAVE_REGION1.as_ptr(), REGION1_START as *mut u8, REGION1_SIZE,
    );
    core::ptr::copy_nonoverlapping(
        SAVE_REGION2.as_ptr(), REGION2_START as *mut u8, REGION2_SIZE,
    );
    Beep(600, 80);
}

// ─── メインスレッド ───────────────────────────────────
unsafe extern "system" fn main_thread(_param: *mut core::ffi::c_void) -> u32 {
    load_config();
    Beep(800, 200);

    // HWND を探す (最大2秒待つ: ウィンドウ生成前に走ることがある)
    for _ in 0..125 {
        let hwnd = find_game_hwnd();
        if hwnd != 0 {
            GAME_HWND.store(hwnd, Ordering::Relaxed);
            break;
        }
        Sleep(16);
    }

    let mut prev_save = false;
    let mut prev_load = false;

    loop {
        let save_vk = SAVE_VK.load(Ordering::Relaxed);
        let load_vk = LOAD_VK.load(Ordering::Relaxed);

        let save_down = GetAsyncKeyState(save_vk) as u16 & 0x8000 != 0;
        let load_down = GetAsyncKeyState(load_vk) as u16 & 0x8000 != 0;

        if save_down && !prev_save { do_save(); }
        if load_down && !prev_load { do_load(); }

        prev_save = save_down;
        prev_load = load_down;

        draw_velocity();

        Sleep(16);
    }
}

// ─── DllMain ──────────────────────────────────────────
#[no_mangle]
pub extern "system" fn DllMain(
    hmodule: HMODULE,
    reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> BOOL {
    match reason {
        DLL_PROCESS_ATTACH => unsafe {
            DLL_HMODULE.store(hmodule as isize, Ordering::Relaxed);
            CreateThread(
                core::ptr::null(), 0, Some(main_thread),
                core::ptr::null(), 0, core::ptr::null_mut(),
            );
        },
        DLL_PROCESS_DETACH => {
            // フォントを解放
            let font = CACHED_FONT.load(Ordering::Relaxed);
            if font != 0 { unsafe { DeleteObject(font); } }
        }
        _ => {}
    }
    1
}
