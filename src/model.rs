use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileCategory {
    Folder,
    Image,
    Video,
    Audio,
    Document,
    Archive,
    Other,
}

impl FileCategory {
    pub fn from_path_and_dir(path: &Path, is_dir: bool) -> Self {
        if is_dir {
            return FileCategory::Folder;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();

        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "webp" | "avif" | "heic" | "dng" | "raw" | "gif" | "bmp"
            | "svg" | "ico" | "tiff" => FileCategory::Image,
            "mp4" | "mkv" | "mov" | "avi" | "webm" | "3gp" | "ts" | "m4v" | "wmv" | "flv" => {
                FileCategory::Video
            }
            "mp3" | "flac" | "m4a" | "wav" | "ogg" | "opus" | "aac" | "wma" => FileCategory::Audio,
            "pdf" | "doc" | "docx" | "txt" | "epub" | "xlsx" | "xls" | "pptx" | "ppt" | "csv"
            | "md" | "odt" | "rtf" => FileCategory::Document,
            "zip" | "rar" | "7z" | "tar" | "gz" | "apk" | "obb" | "bz2" | "xz" | "zst" | "iso" => {
                FileCategory::Archive
            }
            _ => FileCategory::Other,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            FileCategory::Folder => "Folder",
            FileCategory::Image => "Image",
            FileCategory::Video => "Video",
            FileCategory::Audio => "Audio",
            FileCategory::Document => "Document",
            FileCategory::Archive => "Archive",
            FileCategory::Other => "File",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DriveInfo {
    pub name: String,
    pub path: PathBuf,
    pub total_space: u64,
    pub available_space: u64,
}

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
    pub is_system_file: bool,
    pub is_hidden: bool,
    pub is_reparse_point: bool,
    pub is_scanning: bool,
    pub category: FileCategory,
}

impl FileEntry {
    /// Creates an entry with zero size and default flags. Set measured values
    /// with struct update syntax: `FileEntry { size, ..FileEntry::new(..) }`.
    pub fn new(name: String, path: PathBuf, is_dir: bool) -> Self {
        let (is_system_protected, is_system_file, is_hidden, is_reparse_point) =
            detect_path_attributes(&path, &name);
        let category = FileCategory::from_path_and_dir(&path, is_dir);
        Self {
            name,
            path,
            is_dir,
            size: 0,
            allocated_size: 0,
            modified: None,
            item_count: usize::from(!is_dir),
            is_access_denied: false,
            is_partially_locked: false,
            is_system_protected,
            is_system_file,
            is_hidden,
            is_reparse_point,
            is_scanning: false,
            category,
        }
    }
}

pub fn is_os_system_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "$recycle.bin"
            | "system volume information"
            | "pagefile.sys"
            | "hiberfil.sys"
            | "swapfile.sys"
            | "dumpstack.log.tmp"
            | "bootmgr"
            | "bootnxt"
            | "config.msi"
            | "documents and settings"
    )
}

pub fn detect_path_attributes(path: &Path, name: &str) -> (bool, bool, bool, bool) {
    let is_sys_path = is_system_path(path);
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            let attrs = meta.file_attributes();
            let is_hidden = (attrs & 0x0000_0002) != 0 || name.starts_with('.');
            let is_sys_attr = (attrs & 0x0000_0004) != 0 || is_os_system_name(name);
            let is_reparse = meta.file_type().is_symlink();
            let is_protected = is_sys_attr || is_sys_path;
            return (is_protected, is_sys_attr, is_hidden, is_reparse);
        }
    }
    #[cfg(not(windows))]
    {
        let is_sys_attr = matches!(path.to_str(), Some("/proc" | "/sys" | "/dev"));
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            let is_hidden = name.starts_with('.');
            let is_reparse = meta.file_type().is_symlink();
            let is_protected = is_sys_attr || is_sys_path;
            return (is_protected, is_sys_attr, is_hidden, is_reparse);
        }
    }
    let is_sys_attr = is_os_system_name(name);
    (is_sys_path || is_sys_attr, is_sys_attr, name.starts_with('.'), false)
}

#[cfg(windows)]
static SYSTEM_PREFIXES: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
    let vars = ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"];
    let mut roots = Vec::new();
    for var in vars {
        if let Ok(val) = std::env::var(var) {
            roots.push(val.replace('/', "\\").to_lowercase());
        }
    }
    if roots.is_empty() {
        roots.push("c:\\windows".to_string());
        roots.push("c:\\program files".to_string());
        roots.push("c:\\program files (x86)".to_string());
        roots.push("c:\\programdata".to_string());
    }
    roots
});

pub fn is_dos_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    let s = stem.to_ascii_uppercase();
    matches!(
        s.as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
            | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
            | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    )
}

pub fn is_system_path(path: &Path) -> bool {
    let raw = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let s = if let Some(stripped) = raw.strip_prefix("\\\\?\\unc\\") {
        format!("\\\\{}", stripped)
    } else if let Some(stripped) = raw.strip_prefix("\\\\?\\") {
        stripped.to_string()
    } else {
        raw
    };

    #[cfg(windows)]
    {
        for root in SYSTEM_PREFIXES.iter() {
            if s == *root || s.starts_with(&format!("{root}\\")) {
                return true;
            }
        }

        // Multi-drive protection for secondary installations, 8.3 aliases, or drive-level critical folders
        if s.contains("\\system volume information")
            || s.contains("\\$recycle.bin")
            || s.contains("\\boot")
            || s.contains("\\efi")
            || s.contains("\\recovery")
            || s.contains("\\$winreagent")
            || s.contains("\\progra~1")
            || s.contains("\\progra~2")
            || s.contains("\\window~1")
            || s.contains("\\progra~3")
            || s.ends_with("\\windows")
            || s.contains("\\windows\\")
            || s.ends_with("\\program files")
            || s.contains("\\program files\\")
            || s.ends_with("\\programdata")
            || s.contains("\\programdata\\")
            || s.ends_with("hiberfil.sys")
            || s.ends_with("pagefile.sys")
            || s.ends_with("swapfile.sys")
            || s.ends_with("dumpstack.log.tmp")
        {
            return true;
        }
    }

    #[cfg(not(windows))]
    {
        const ROOTS: [&str; 15] = [
            "/boot", "/etc", "/bin", "/sbin", "/lib", "/usr", "/system", "/library", "/private",
            "/proc", "/sys", "/dev", "/run", "/root", "/var",
        ];
        if s == "/" || ROOTS.iter().any(|r| s == *r || s.starts_with(&format!("{}/", r))) {
            return true;
        }
    }

    false
}

pub fn is_drive_root(path: &Path) -> bool {
    if path.parent().is_none() {
        return true;
    }
    let raw = path.to_string_lossy().replace('/', "\\");
    let s = raw.strip_prefix("\\\\?\\").unwrap_or(&raw);
    if s == "\\" || (s.len() <= 3 && s.contains(':')) {
        return true;
    }
    // UNC share root check: \\server\share
    if s.starts_with("\\\\") {
        let parts: Vec<&str> = s.trim_start_matches('\\').split('\\').filter(|p| !p.is_empty()).collect();
        if parts.len() <= 2 {
            return true;
        }
    }
    false
}

pub fn is_root_or_system(path: &Path) -> bool {
    if is_drive_root(path) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_files_and_windows_are_not_system_files() {
        let pf = Path::new("C:\\Program Files");
        let (is_prot, is_sys, is_hidden, _) = detect_path_attributes(pf, "Program Files");
        assert!(!is_sys, "Program Files must not be marked as a hidden OS system file");
        assert!(!is_hidden, "Program Files is not a hidden directory");
        assert!(is_prot, "Program Files remains protected against accidental deletion");

        let win = Path::new("C:\\Windows");
        let (is_prot_win, is_sys_win, is_hidden_win, _) = detect_path_attributes(win, "Windows");
        assert!(!is_sys_win, "Windows must not be marked as a hidden OS system file");
        assert!(!is_hidden_win, "Windows directory is not hidden");
        assert!(is_prot_win, "Windows remains protected against accidental deletion");

        let sysvol = Path::new("C:\\System Volume Information");
        let (_, is_sys_vol, _, _) = detect_path_attributes(sysvol, "System Volume Information");
        assert!(is_sys_vol, "System Volume Information must be recognized as an OS system file");

        let pagefile = Path::new("C:\\pagefile.sys");
        let (_, is_sys_page, _, _) = detect_path_attributes(pagefile, "pagefile.sys");
        assert!(is_sys_page, "pagefile.sys must be recognized as an OS system file");
    }
}
