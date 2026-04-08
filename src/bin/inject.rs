use std::path::PathBuf;

use windows_sys::Win32::Foundation::{
    CloseHandle, FALSE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE,
    PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, WaitForSingleObject, PROCESS_ALL_ACCESS,
};

#[link(name = "kernel32")]
extern "system" {
    fn WriteProcessMemory(
        hprocess: HANDLE,
        lpbaseaddress: *const core::ffi::c_void,
        lpbuffer: *const core::ffi::c_void,
        nsize: usize,
        lpnumberofbyteswritten: *mut usize,
    ) -> i32;

    fn CreateRemoteThread(
        hprocess: HANDLE,
        lpthreadattributes: *const core::ffi::c_void,
        dwstacksize: usize,
        lpstartaddress: *const core::ffi::c_void,
        lpparameter: *const core::ffi::c_void,
        dwcreationflags: u32,
        lpthreadid: *mut u32,
    ) -> HANDLE;

    fn GetProcAddress(hmodule: HANDLE, lpprocname: *const u8) -> *const core::ffi::c_void;
    fn GetModuleHandleA(lpmodulename: *const u8) -> HANDLE;
}

fn usage() -> ! {
    eprintln!("usage: inject <pid|process_name> [dll_path]");
    eprintln!("  dll_path defaults to syobon_quicksave.dll next to this exe");
    std::process::exit(1);
}

/// プロセス名 (日本語対応) から PID を検索
fn find_pid_by_name(name: &str) -> Option<u32> {
    // 比較用: 小文字化・末尾 .exe 正規化
    let name_lower = name.to_lowercase();
    let name_exe = if name_lower.ends_with(".exe") {
        name_lower.clone()
    } else {
        format!("{}.exe", name_lower)
    };

    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) == FALSE {
            CloseHandle(snap);
            return None;
        }
        loop {
            // szExeFile は [u16; 260]: UTF-16LE → String
            let exe = String::from_utf16_lossy(
                entry.szExeFile.iter().take_while(|&&c| c != 0).cloned()
                    .collect::<Vec<u16>>().as_slice(),
            )
            .to_lowercase();

            if exe == name_lower || exe == name_exe {
                let pid = entry.th32ProcessID;
                CloseHandle(snap);
                return Some(pid);
            }
            entry = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32NextW(snap, &mut entry) == FALSE {
                break;
            }
        }
        CloseHandle(snap);
    }
    None
}

fn inject(pid: u32, dll_path: &str) -> Result<(), String> {
    // UTF-16LE に変換 (LoadLibraryW 用)。日本語パスも正しく扱える。
    let dll_wide: Vec<u16> = dll_path.encode_utf16().chain(std::iter::once(0)).collect();
    let dll_bytes = unsafe {
        std::slice::from_raw_parts(dll_wide.as_ptr() as *const u8, dll_wide.len() * 2)
    };

    unsafe {
        let hprocess = OpenProcess(PROCESS_ALL_ACCESS, FALSE, pid);
        if hprocess == 0 as HANDLE {
            return Err(format!("OpenProcess({pid}) failed (管理者権限が必要かもしれません)"));
        }

        let remote_mem = VirtualAllocEx(
            hprocess,
            core::ptr::null(),
            dll_bytes.len(),
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        if remote_mem.is_null() {
            CloseHandle(hprocess);
            return Err("VirtualAllocEx failed".to_string());
        }

        let ok = WriteProcessMemory(
            hprocess,
            remote_mem,
            dll_bytes.as_ptr() as *const _,
            dll_bytes.len(),
            core::ptr::null_mut(),
        );
        if ok == 0 {
            VirtualFreeEx(hprocess, remote_mem, 0, MEM_RELEASE);
            CloseHandle(hprocess);
            return Err("WriteProcessMemory failed".to_string());
        }

        // LoadLibraryW (Unicode) を使ってパスの日本語を正しく扱う
        let k32 = GetModuleHandleA(b"kernel32.dll\0".as_ptr());
        let loadlib = GetProcAddress(k32, b"LoadLibraryW\0".as_ptr());
        if loadlib.is_null() {
            VirtualFreeEx(hprocess, remote_mem, 0, MEM_RELEASE);
            CloseHandle(hprocess);
            return Err("GetProcAddress(LoadLibraryW) failed".to_string());
        }

        let hthread = CreateRemoteThread(
            hprocess,
            core::ptr::null(),
            0,
            loadlib,
            remote_mem,
            0,
            core::ptr::null_mut(),
        );
        if hthread == 0 as HANDLE {
            VirtualFreeEx(hprocess, remote_mem, 0, MEM_RELEASE);
            CloseHandle(hprocess);
            return Err("CreateRemoteThread failed".to_string());
        }

        WaitForSingleObject(hthread, 5000);
        CloseHandle(hthread);
        VirtualFreeEx(hprocess, remote_mem, 0, MEM_RELEASE);
        CloseHandle(hprocess);
    }

    Ok(())
}

// 引数なしで使うデフォルトのプロセス名
const DEFAULT_PROCESS: &str = "しょぼんのアクション";

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let process_name = if args.len() >= 2 { args[1].as_str() } else { DEFAULT_PROCESS };

    // 引数なし・または --help
    if args.get(1).map(|s| s.as_str()) == Some("--help") {
        usage();
    }

    let pid: u32 = if let Ok(n) = process_name.parse::<u32>() {
        n
    } else {
        find_pid_by_name(process_name).unwrap_or_else(|| {
            eprintln!("プロセス '{}' が見つかりません。ゲームを先に起動してください。", process_name);
            std::process::exit(1);
        })
    };

    let dll_path = if args.len() >= 3 {
        PathBuf::from(&args[2])
    } else {
        let mut p = std::env::current_exe().unwrap();
        p.pop();
        p.push("syobon_quicksave.dll");
        p
    };

    let dll_abs = dll_path.canonicalize().unwrap_or_else(|_| dll_path.clone());
    let dll_str = dll_abs.to_str().unwrap_or_else(|| {
        eprintln!("DLL path contains non-UTF-8 characters");
        std::process::exit(1);
    });

    println!("PID {} にインジェクト中: {}", pid, dll_str);
    match inject(pid, dll_str) {
        Ok(()) => println!("完了！  F5 = セーブ  /  F9 = ロード"),
        Err(e) => {
            eprintln!("エラー: {}", e);
            std::process::exit(1);
        }
    }
}
