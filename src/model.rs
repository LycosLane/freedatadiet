use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortBy {
    Size,
    AllocatedSize,
    Name,
    Modified,
    ItemCount,
}

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

    pub fn usage_ratio(&self) -> f32 {
        if self.total_space == 0 {
            0.0
        } else {
            (self.used_space() as f64 / self.total_space as f64).clamp(0.0, 1.0) as f32
        }
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
    pub is_system_protected: bool,
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
    ) -> Self {
        let is_system_protected = is_system_path(&path);
        Self {
            name,
            path,
            is_dir,
            size,
            allocated_size,
            modified,
            item_count,
            is_access_denied,
            is_system_protected,
        }
    }
}

pub fn is_system_path(path: &Path) -> bool {
    let s = path.to_string_lossy().to_lowercase();
    #[cfg(windows)]
    {
        s == "c:\\windows"
            || s.starts_with("c:\\windows\\")
            || s == "c:\\program files"
            || s == "c:\\program files (x86)"
            || s.contains("system volume information")
            || s.ends_with("$recycle.bin")
    }
    #[cfg(not(windows))]
    {
        s == "/boot"
            || s.starts_with("/boot/")
            || s == "/etc"
            || s.starts_with("/etc/")
            || s == "/bin"
            || s == "/sbin"
            || s == "/usr/bin"
            || s == "/System"
    }
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
