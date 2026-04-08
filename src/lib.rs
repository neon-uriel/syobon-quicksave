use std::sync::atomic::{AtomicI32, AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::{BOOL, HMODULE};
use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_F5, VK_F9,
};

// ゲームのメモリアドレス (ASLR なし・ImageBase 0x400000 固定)
const REGION1_START: usize = 0x5A1000;
const REGION1_SIZE: usize = 0x1000;
const REGION2_START: usize = 0x87A000;
const REGION2_SIZE: usize = 0x16000;
const GAME_STATE_ADDR: usize = 0x5A1274;

type HANDLE = isize;
type LPTHREAD_START_ROUTINE =
    Option<unsafe extern "system" fn(*mut core::ffi::c_void) -> u32>;

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
}

// 設定可能なキー (デフォルト: F5/F9)
static SAVE_VK: AtomicI32 = AtomicI32::new(VK_F5 as i32);
static LOAD_VK: AtomicI32 = AtomicI32::new(VK_F9 as i32);
// DllMain で受け取った自分自身の HMODULE
static DLL_HMODULE: AtomicIsize = AtomicIsize::new(0);

// セーブバッファ
static mut SAVE_REGION1: [u8; REGION1_SIZE] = [0u8; REGION1_SIZE];
static mut SAVE_REGION2: [u8; REGION2_SIZE] = [0u8; REGION2_SIZE];
static mut HAS_SAVE: bool = false;

/// DLL 自身のディレクトリパスを取得
fn dll_dir() -> Option<std::path::PathBuf> {
    let hmod = DLL_HMODULE.load(Ordering::Relaxed);
    let mut buf = [0u16; 512];
    let len = unsafe { GetModuleFileNameW(hmod, buf.as_mut_ptr(), buf.len() as u32) };
    if len == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_path_buf())
}

/// quicksave.cfg を読んでキー設定を更新
fn load_config() {
    let Some(dir) = dll_dir() else { return };
    let cfg = dir.join("quicksave.cfg");
    let Ok(content) = std::fs::read_to_string(&cfg) else { return };
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
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

unsafe fn do_save() {
    core::ptr::copy_nonoverlapping(
        REGION1_START as *const u8,
        SAVE_REGION1.as_mut_ptr(),
        REGION1_SIZE,
    );
    core::ptr::copy_nonoverlapping(
        REGION2_START as *const u8,
        SAVE_REGION2.as_mut_ptr(),
        REGION2_SIZE,
    );
    HAS_SAVE = true;
    Beep(1000, 80);
}

unsafe fn do_load() {
    if !HAS_SAVE {
        Beep(300, 150);
        return;
    }
    let game_state = *(GAME_STATE_ADDR as *const i32);
    if game_state != 1 {
        return;
    }
    core::ptr::copy_nonoverlapping(
        SAVE_REGION1.as_ptr(),
        REGION1_START as *mut u8,
        REGION1_SIZE,
    );
    core::ptr::copy_nonoverlapping(
        SAVE_REGION2.as_ptr(),
        REGION2_START as *mut u8,
        REGION2_SIZE,
    );
    Beep(600, 80);
}

unsafe extern "system" fn main_thread(_param: *mut core::ffi::c_void) -> u32 {
    // 設定ファイルを読む
    load_config();

    // 起動確認ビープ
    Beep(800, 200);

    let mut prev_save = false;
    let mut prev_load = false;

    loop {
        let save_vk = SAVE_VK.load(Ordering::Relaxed);
        let load_vk = LOAD_VK.load(Ordering::Relaxed);

        let save_down = GetAsyncKeyState(save_vk) as u16 & 0x8000 != 0;
        let load_down = GetAsyncKeyState(load_vk) as u16 & 0x8000 != 0;

        if save_down && !prev_save {
            do_save();
        }
        if load_down && !prev_load {
            do_load();
        }

        prev_save = save_down;
        prev_load = load_down;

        Sleep(16);
    }
}

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
                core::ptr::null(),
                0,
                Some(main_thread),
                core::ptr::null(),
                0,
                core::ptr::null_mut(),
            );
        },
        DLL_PROCESS_DETACH => {}
        _ => {}
    }
    1
}
