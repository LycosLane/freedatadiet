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
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            let wide = to_wide(&root);
            let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
            // 2: DRIVE_REMOVABLE, 3: DRIVE_FIXED, 4: DRIVE_REMOTE, 5: DRIVE_CDROM, 6: DRIVE_RAMDISK
            if drive_type >= 2 && drive_type <= 6 {
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
            ((file_size + 4095) / 4096) * 4096
        }
    }

    pub fn restart_as_admin(current_path: Option<&Path>) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut cmd = std::process::Command::new("powershell");
        let arg_path = current_path
            .map(|p| format!(" -ArgumentList '\"{}\"'", p.display()))
            .unwrap_or_default();
        let script = format!(
            "Start-Process -FilePath '{}'{} -Verb RunAs",
            exe.display(),
            arg_path
        );
        cmd.args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script]);
        cmd.spawn().map_err(|e| e.to_string())?;
        std::process::exit(0);
    }
}

#[cfg(not(windows))]
mod unix {
    use std::path::Path;
    use crate::model::DriveInfo;

    pub fn is_admin() -> bool {
        unsafe { libc::geteuid() == 0 }
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
        let c_path = CString::new(path).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
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

    pub fn restart_as_admin(current_path: Option<&Path>) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut cmd = std::process::Command::new("pkexec");
        cmd.arg(&exe);
        if let Some(p) = current_path {
            cmd.arg(p);
        }
        cmd.spawn().map_err(|e| e.to_string())?;
        std::process::exit(0);
    }

    pub fn show_properties(path: &Path) -> Result<(), String> {
        crate::platform::open_in_file_manager(path).map_err(|e| e.to_string())
    }
}

#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
pub use unix::*;

pub fn open_in_file_manager(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        if path.is_file() {
            std::process::Command::new("explorer")
                .arg(format!("/select,\"{}\"", path.display()))
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
            .arg(target)
            .spawn()?;
    }

    Ok(())
}

pub fn open_file(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &path.display().to_string()])
            .spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()?;
    }

    Ok(())
}
