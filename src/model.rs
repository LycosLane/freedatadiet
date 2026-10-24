use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone)]
pub struct DriveInfo {
    pub name: String,
    pub path: PathBuf,
    pub total_space: u64,
    pub available_space: u64,
}

#[allow(dead_code)]
impl DriveInfo {
    pub fn used_space(&self) -> u64 {
        self.total_space.saturating_sub(self.available_space)
    }
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub allocated_size: u64,
    pub modified: Option<SystemTime>,
    pub item_count: usize,
    pub is_access_denied: bool,
    pub is_partially_locked: bool,
    pub is_system_protected: bool,
    pub is_hidden: bool,
    pub is_reparse_point: bool,
    pub is_scanning: bool,
}

impl FileEntry {
    pub fn new(
        name: String,
        path: PathBuf,
        is_dir: bool,
        size: u64,
        allocated_size: u64,
        modified: Option<SystemTime>,
        item_count: usize,
        is_access_denied: bool,
        is_partially_locked: bool,
    ) -> Self {
        let (is_system_protected, is_hidden, is_reparse_point) = detect_path_attributes(&path, &name);
        Self {
            name,
            path,
            is_dir,
            size,
            allocated_size,
            modified,
            item_count,
            is_access_denied,
            is_partially_locked,
            is_system_protected,
            is_hidden,
            is_reparse_point,
            is_scanning: false,
        }
    }
}

pub fn detect_path_attributes(path: &Path, name: &str) -> (bool, bool, bool) {
    let is_sys_path = is_system_path(path);
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            let attrs = meta.file_attributes();
            let is_hidden = (attrs & 0x0000_0002) != 0 || name.starts_with('.');
            let is_system = (attrs & 0x0000_0004) != 0 || is_sys_path;
            // Only real links count: Windows also sets the reparse flag on cloud placeholders (OneDrive etc.)
            let is_reparse = meta.file_type().is_symlink();
            return (is_system, is_hidden, is_reparse);
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            let is_hidden = name.starts_with('.');
            let is_reparse = meta.file_type().is_symlink();
            return (is_sys_path, is_hidden, is_reparse);
        }
    }
    (is_sys_path, name.starts_with('.'), false)
}

pub fn is_system_path(path: &Path) -> bool {
    let s = path.to_string_lossy().to_lowercase();
    #[cfg(windows)]
    {
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()).to_lowercase();
        let program_files = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string()).to_lowercase();
        let program_files_x86 = std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".to_string()).to_lowercase();
        s == system_root
            || s.starts_with(&format!("{}\\", system_root))
            || s == program_files
            || s.starts_with(&format!("{}\\", program_files))
            || s == program_files_x86
            || s.starts_with(&format!("{}\\", program_files_x86))
            || s.contains("system volume information")
            || s.contains("documents and settings")
            || s.contains("$recycle.bin")
            || s.ends_with("hiberfil.sys")
            || s.ends_with("pagefile.sys")
            || s.ends_with("swapfile.sys")
            || s.ends_with("dumpstack.log.tmp")
    }
    #[cfg(not(windows))]
    {
        const ROOTS: [&str; 15] = [
            "/boot", "/etc", "/bin", "/sbin", "/lib", "/usr", "/system", "/library", "/private", "/proc", "/sys",
            "/dev", "/run", "/root", "/var",
        ];
        s == "/" || ROOTS.iter().any(|r| s == *r || s.starts_with(&format!("{}/", r)))
    }
}

pub fn is_root_or_system(path: &Path) -> bool {
    if path.parent().is_none() {
        return true;
    }
    let s = path.to_string_lossy();
    if s == "/" || s == "\\" || (s.len() <= 3 && s.contains(':')) {
        return true;
    }
    is_system_path(path)
}

pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_system_time(time: SystemTime) -> String {
    let dur = match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => return "-".to_string(),
    };
    format_epoch_secs(dur.as_secs())
}

pub fn format_epoch_secs(secs: u64) -> String {
    // Corrupt timestamps (e.g. u64::MAX) would otherwise spin the year loop for ages on the UI thread
    if secs > 253_402_300_799 {
        return "-".to_string();
    }
    let min = ((secs / 60) % 60) as u32;
    let hour = ((secs / 3600) % 24) as u32;
    let mut days = (secs / 86400) as i64;

    let mut year = 1970;
    loop {
        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if is_leap { 366 } else { 365 };
        if days >= days_in_year {
            days -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }

    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = [
        31, if is_leap { 29 } else { 28 }, 31, 30, 31, 30,
        31, 31, 30, 31, 30, 31,
    ];

    let mut month = 1;
    for &m_days in &month_days {
        if days >= m_days {
            days -= m_days;
            month += 1;
        } else {
            break;
        }
    }
    let day = days + 1;

    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hour, min)
}
