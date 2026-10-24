use std::path::Path;

#[cfg(windows)]
mod win {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use crate::model::DriveInfo;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut u64,
            lpTotalNumberOfBytes: *mut u64,
            lpTotalNumberOfFreeBytes: *mut u64,
        ) -> i32;

        fn GetDriveTypeW(lpRootPathName: *const u16) -> u32;
        fn GetCompressedFileSizeW(lpFileName: *const u16, lpFileSizeHigh: *mut u32) -> u32;
        fn SetErrorMode(uMode: u32) -> u32;
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn IsUserAnAdmin() -> i32;
        fn ShellExecuteExW(pExecInfo: *mut SHELLEXECUTEINFOW) -> i32;
    }

    #[repr(C)]
    struct SHELLEXECUTEINFOW {
        cb_size: u32,
        f_mask: u32,
        hwnd: isize,
        lp_verb: *const u16,
        lp_file: *const u16,
        lp_parameters: *const u16,
        lp_directory: *const u16,
        n_show: i32,
        h_inst_app: isize,
        lp_id_list: *mut std::ffi::c_void,
        lp_class: *const u16,
        hkey_class: isize,
        dw_hot_key: u32,
        h_icon_or_monitor: isize,
        h_process: isize,
    }

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn is_admin() -> bool {
        unsafe { IsUserAnAdmin() != 0 }
    }

    pub fn is_system_dark_mode() -> bool {
        #[link(name = "advapi32")]
        unsafe extern "system" {
            fn RegGetValueW(
                hkey: isize,
                lpSubKey: *const u16,
                lpValue: *const u16,
                dwFlags: u32,
                pdwType: *mut u32,
                pvData: *mut std::ffi::c_void,
                pcbData: *mut u32,
            ) -> i32;
        }

        let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        let val_name = to_wide("AppsUseLightTheme");
        let mut data: u32 = 0;
        let mut data_len = std::mem::size_of::<u32>() as u32;
        let mut val_type: u32 = 0;

        let res = unsafe {
            RegGetValueW(
                0x8000_0001_u32 as i32 as isize, // HKEY_CURRENT_USER
                subkey.as_ptr(),
                val_name.as_ptr(),
                0x00000010, // RRF_RT_REG_DWORD
                &mut val_type,
                &mut data as *mut u32 as *mut std::ffi::c_void,
                &mut data_len,
            )
        };

        if res == 0 {
            data == 0
        } else {
            true
        }
    }

    pub fn show_properties(path: &Path) -> Result<(), String> {
        let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let verb = to_wide("properties");

        let mut sei = SHELLEXECUTEINFOW {
            cb_size: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            f_mask: 0x0000_000C, // SEE_MASK_INVOKEIDLIST
            hwnd: 0,
            lp_verb: verb.as_ptr(),
            lp_file: wide_path.as_ptr(),
            lp_parameters: std::ptr::null(),
            lp_directory: std::ptr::null(),
            n_show: 5, // SW_SHOW
            h_inst_app: 0,
            lp_id_list: std::ptr::null_mut(),
            lp_class: std::ptr::null(),
            hkey_class: 0,
            dw_hot_key: 0,
            h_icon_or_monitor: 0,
            h_process: 0,
        };

        let ret = unsafe { ShellExecuteExW(&mut sei) };
        if ret != 0 {
            Ok(())
        } else {
            Err("Failed to open Windows properties".to_string())
        }
    }

    pub fn get_drives() -> Vec<DriveInfo> {
        let mut drives = Vec::new();
        // Suppress "No disk in drive" system dialogs for empty removable drives.
        // SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX
        unsafe { SetErrorMode(0x8001) };
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            let wide = to_wide(&root);
            let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
            // 2: DRIVE_REMOVABLE, 3: DRIVE_FIXED, 5: DRIVE_CDROM, 6: DRIVE_RAMDISK.
            // 4 (DRIVE_REMOTE) is skipped: an unreachable network share can block startup for a minute.
            if matches!(drive_type, 2 | 3 | 5 | 6) {
                let mut free_avail = 0u64;
                let mut total = 0u64;
                let mut free_total = 0u64;

                let success = unsafe {
                    GetDiskFreeSpaceExW(
                        wide.as_ptr(),
                        &mut free_avail,
                        &mut total,
                        &mut free_total,
                    )
                };

                if success != 0 && total > 0 {
                    drives.push(DriveInfo {
                        name: format!("{}:", letter as char),
                        path: PathBuf::from(&root),
                        total_space: total,
                        available_space: free_avail,
                    });
                }
            }
        }
        drives
    }

    pub fn get_allocated_size(path: &Path, file_size: u64) -> u64 {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let mut high = 0u32;
        let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &mut high) };
        if low != 0xFFFF_FFFF || high != 0 {
            ((high as u64) << 32) | (low as u64)
        } else if file_size == 0 {
            0
        } else {
            file_size.div_ceil(4096) * 4096
        }
    }

    /// Runs a shell verb on a path through the Win32 shell directly (no cmd.exe, no PowerShell),
    /// so file names containing &, ^, % or quotes are never interpreted as commands.
    fn shell_execute(verb: &str, file: &Path, params: Option<&str>) -> bool {
        let wide_file: Vec<u16> = file.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let wide_verb = to_wide(verb);
        let wide_params = params.map(to_wide);

        let mut sei = SHELLEXECUTEINFOW {
            cb_size: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            f_mask: 0,
            hwnd: 0,
            lp_verb: wide_verb.as_ptr(),
            lp_file: wide_file.as_ptr(),
            lp_parameters: wide_params.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
            lp_directory: std::ptr::null(),
            n_show: 1, // SW_SHOWNORMAL
            h_inst_app: 0,
            lp_id_list: std::ptr::null_mut(),
            lp_class: std::ptr::null(),
            hkey_class: 0,
            dw_hot_key: 0,
            h_icon_or_monitor: 0,
            h_process: 0,
        };
        unsafe { ShellExecuteExW(&mut sei) != 0 }
    }

    pub fn open_file_win(path: &Path) -> Result<(), std::io::Error> {
        if shell_execute("open", path, None) {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    pub fn restart_as_admin(current_path: Option<&Path>) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let params = current_path.map(|p| {
            let clean = p.to_string_lossy().replace('"', "");
            format!("\"{}\"", clean)
        });
        // Only exit once the elevation prompt was accepted; cancelling keeps this instance alive.
        if shell_execute("runas", &exe, params.as_deref()) {
            std::process::exit(0);
        }
        Err("Elevation was cancelled or failed".to_string())
    }
}

#[cfg(not(windows))]
mod unix {
    use std::path::{Path, PathBuf};
    use crate::model::DriveInfo;

    pub fn is_admin() -> bool {
        unsafe { libc::geteuid() == 0 }
    }

    pub fn is_system_dark_mode() -> bool {
        true
    }

    pub fn get_drives() -> Vec<DriveInfo> {
        let mut drives = Vec::new();
        if let Ok(content) = std::fs::read_to_string("/proc/mounts") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let device = parts[0];
                    let mountpoint = parts[1];
                    let fstype = parts[2];

                    if (device.starts_with("/dev/") || fstype == "zfs" || fstype == "btrfs")
                        && !mountpoint.starts_with("/var/lib/docker")
                        && !mountpoint.starts_with("/snap")
                    {
                        if let Ok(stat) = statvfs_path(mountpoint) {
                            drives.push(DriveInfo {
                                name: mountpoint.to_string(),
                                path: PathBuf::from(mountpoint),
                                total_space: stat.0,
                                available_space: stat.1,
                            });
                        }
                    }
                }
            }
        }

        if drives.is_empty() {
            // macOS has no /proc/mounts: external disks and secondary volumes live in /Volumes.
            if let Ok(entries) = std::fs::read_dir("/Volumes") {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if let Some(stat) = p.to_str().and_then(|s| statvfs_path(s).ok()) {
                        drives.push(DriveInfo {
                            name: entry.file_name().to_string_lossy().to_string(),
                            path: p,
                            total_space: stat.0,
                            available_space: stat.1,
                        });
                    }
                }
            }
        }

        if drives.is_empty() {
            if let Ok(stat) = statvfs_path("/") {
                drives.push(DriveInfo {
                    name: "/".to_string(),
                    path: PathBuf::from("/"),
                    total_space: stat.0,
                    available_space: stat.1,
                });
            }
        }
        drives
    }

    fn statvfs_path(path: &str) -> Result<(u64, u64), std::io::Error> {
        use std::ffi::CString;
        let c_path = CString::new(path).map_err(std::io::Error::other)?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let res = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
        if res == 0 {
            let block_size = stat.f_frsize as u64;
            let total = stat.f_blocks as u64 * block_size;
            let available = stat.f_bavail as u64 * block_size;
            Ok((total, available))
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    pub fn get_allocated_size(_path: &Path, _file_size: u64) -> u64 {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = _path.metadata() {
            meta.blocks() * 512
        } else {
            _file_size
        }
    }

    pub fn restart_as_admin(_current_path: Option<&Path>) -> Result<(), String> {
        // pkexec/sudo scrub DISPLAY/WAYLAND_DISPLAY and cannot report a cancelled prompt back,
        // so relaunching would either crash on startup or silently close this window.
        Err("Start FreeDataDiet from a terminal with: sudo -E freedatadiet".to_string())
    }
}

#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
pub use unix::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FileId {
    pub device: u64,
    pub inode: u64,
}

#[cfg(windows)]
pub fn get_file_id_and_links(path: &Path, meta: &std::fs::Metadata) -> (Option<FileId>, u32) {
    if !meta.is_dir() {
        // On Windows, inspecting hardlinks for every regular file requires opening
        // a file handle (CreateFileW), which degrades directory scan throughput.
        return (None, 1);
    }
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::fs::OpenOptionsExt;

    let file = match std::fs::OpenOptions::new()
        .access_mode(0x0080) // FILE_READ_ATTRIBUTES
        .custom_flags(0x02000000) // FILE_FLAG_BACKUP_SEMANTICS
        .open(path)
    {
        Ok(f) => f,
        Err(_) => return (None, 1),
    };

    #[repr(C)]
    struct BY_HANDLE_FILE_INFORMATION {
        dw_file_attributes: u32,
        ft_creation_time: [u32; 2],
        ft_last_access_time: [u32; 2],
        ft_last_write_time: [u32; 2],
        dw_volume_serial_number: u32,
        n_file_size_high: u32,
        n_file_size_low: u32,
        n_number_of_links: u32,
        n_file_index_high: u32,
        n_file_index_low: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            h_file: isize,
            lp_file_information: *mut BY_HANDLE_FILE_INFORMATION,
        ) -> i32;
    }

    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    let res = unsafe {
        GetFileInformationByHandle(file.as_raw_handle() as isize, info.as_mut_ptr())
    };

    if res != 0 {
        let info = unsafe { info.assume_init() };
        let file_index = ((info.n_file_index_high as u64) << 32) | (info.n_file_index_low as u64);
        (
            Some(FileId {
                device: info.dw_volume_serial_number as u64,
                inode: file_index,
            }),
            info.n_number_of_links,
        )
    } else {
        (None, 1)
    }
}

#[cfg(unix)]
pub fn get_file_id_and_links(_path: &Path, meta: &std::fs::Metadata) -> (Option<FileId>, u32) {
    use std::os::unix::fs::MetadataExt;
    let links = meta.nlink() as u32;
    (Some(FileId { device: meta.dev(), inode: meta.ino() }), links)
}

#[cfg(not(any(windows, unix)))]
pub fn get_file_id_and_links(_path: &Path, _meta: &std::fs::Metadata) -> (Option<FileId>, u32) {
    (None, 1)
}

pub fn open_in_file_manager(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        if path.is_file() {
            use std::os::windows::process::CommandExt;
            let clean = path.to_string_lossy().replace('"', "");
            std::process::Command::new("explorer")
                .raw_arg(format!("/select,\"{}\"", clean))
                .spawn()?;
        } else {
            std::process::Command::new("explorer")
                .arg(path)
                .spawn()?;
        }
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg("--")
            .arg(path)
            .spawn()?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        std::process::Command::new("xdg-open")
            .arg("--")
            .arg(target)
            .spawn()?;
    }

    Ok(())
}

fn is_executable_extension(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_ascii_lowercase();
        matches!(
            ext_lower.as_str(),
            "exe" | "bat" | "cmd" | "com" | "msi" | "scr" | "pif" | "vbs" | "vbe" | "js" | "jse" | "wsf" | "wsh" | "ps1" | "app" | "bin" | "sh"
        )
    } else {
        false
    }
}

pub fn open_file(path: &Path) -> Result<(), std::io::Error> {
    // Security safeguard: never execute arbitrary binaries or scripts directly from a storage analyzer tool.
    // Instead, safely reveal them in the file manager so the user can inspect them.
    if is_executable_extension(path) {
        return open_in_file_manager(path);
    }

    #[cfg(windows)]
    {
        win::open_file_win(path)?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("--")
            .arg(path)
            .spawn()?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg("--")
            .arg(path)
            .spawn()?;
    }

    Ok(())
}

#[cfg(windows)]
pub fn set_clipboard_text(text: &str) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(hwnd: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(uFormat: u32, hMem: isize) -> isize;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> isize;
        fn GlobalFree(hMem: isize) -> isize;
        fn GlobalLock(hMem: isize) -> *mut std::ffi::c_void;
        fn GlobalUnlock(hMem: isize) -> i32;
    }

    let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * std::mem::size_of::<u16>();

    unsafe {
        if OpenClipboard(0) == 0 {
            return false;
        }
        EmptyClipboard();

        // GMEM_MOVEABLE = 0x0002
        let h_mem = GlobalAlloc(0x0002, bytes);
        if h_mem == 0 {
            CloseClipboard();
            return false;
        }

        let ptr = GlobalLock(h_mem) as *mut u16;
        if ptr.is_null() {
            GlobalFree(h_mem);
            CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        GlobalUnlock(h_mem);

        // CF_UNICODETEXT = 13. On success the clipboard owns the memory; on failure we must free it.
        let ok = SetClipboardData(13, h_mem) != 0;
        if !ok {
            GlobalFree(h_mem);
        }
        CloseClipboard();
        ok
    }
}

#[cfg(not(windows))]
pub fn set_clipboard_text(_text: &str) -> bool {
    false
}