use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::model::SortDirection;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColumnId {
    Type,
    Name,
    Size,
    AllocatedSize,
    Usage,
    Items,
    Modified,
}

impl ColumnId {
    pub fn all() -> &'static [ColumnId] {
        &[
            ColumnId::Type,
            ColumnId::Name,
            ColumnId::Size,
            ColumnId::AllocatedSize,
            ColumnId::Usage,
            ColumnId::Items,
            ColumnId::Modified,
        ]
    }

    pub fn default_order() -> Vec<ColumnId> {
        vec![
            ColumnId::Type,
            ColumnId::Name,
            ColumnId::Size,
            ColumnId::AllocatedSize,
            ColumnId::Usage,
            ColumnId::Items,
            ColumnId::Modified,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            ColumnId::Type => "Type",
            ColumnId::Name => "Name",
            ColumnId::Size => "Size",
            ColumnId::AllocatedSize => "On Disk",
            ColumnId::Usage => "Usage",
            ColumnId::Items => "Items",
            ColumnId::Modified => "Date",
        }
    }

    pub fn fixed_width(&self) -> f32 {
        match self {
            ColumnId::Type => 52.0,
            ColumnId::Name => 0.0, // Flexible / remaining space
            ColumnId::Size => 95.0,
            ColumnId::AllocatedSize => 95.0,
            ColumnId::Usage => 130.0,
            ColumnId::Items => 85.0,
            ColumnId::Modified => 135.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortRule {
    pub column: ColumnId,
    pub direction: SortDirection,
}

pub fn default_sort_rules() -> Vec<SortRule> {
    vec![SortRule {
        column: ColumnId::Size,
        direction: SortDirection::Descending,
    }]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UsageBasis {
    LargestItem,
    #[default]
    Folder,
    DriveUsed,
    DriveTotal,
}

impl UsageBasis {
    pub fn title(&self) -> &'static str {
        match self {
            UsageBasis::LargestItem => "Usage (Relative)",
            UsageBasis::Folder => "Usage (Folder)",
            UsageBasis::DriveUsed => "Usage (Used)",
            UsageBasis::DriveTotal => "Usage (Drive)",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            UsageBasis::LargestItem => "Relative to largest item",
            UsageBasis::Folder => "Current folder total",
            UsageBasis::DriveUsed => "Used drive space",
            UsageBasis::DriveTotal => "Total drive capacity",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            UsageBasis::LargestItem => "Largest item fills the bar; others scale proportionally.",
            UsageBasis::Folder => "Each item's share of this folder.",
            UsageBasis::DriveUsed => "Each item's share of all used space on the drive.",
            UsageBasis::DriveTotal => "Each item's share of the entire drive capacity.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AppTheme {
    #[default]
    Dark,
    Light,
    System,
}

impl AppTheme {
    pub fn label(&self) -> &'static str {
        match self {
            AppTheme::Dark => "Dark Mode",
            AppTheme::Light => "Light Mode",
            AppTheme::System => "System (Auto)",
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QuickSearchMode {
    #[default]
    JumpToPrefix,
    LiveFilter,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutSettings {
    #[serde(default = "default_true")]
    pub f2_search_mode: bool,
    #[serde(default = "default_true")]
    pub f3_search_filter: bool,
    #[serde(default = "default_true")]
    pub f5_refresh: bool,
    #[serde(default = "default_true")]
    pub f11_fullscreen: bool,
    #[serde(default = "default_true")]
    pub f12_settings: bool,
    #[serde(default = "default_true")]
    pub ctrl_comma_settings: bool,
    #[serde(default = "default_true")]
    pub ctrl_h_hidden: bool,
    #[serde(default = "default_true")]
    pub ctrl_shift_h_system: bool,
    #[serde(default = "default_true")]
    pub del_trash: bool,
    #[serde(default = "default_true")]
    pub shift_del_permanent: bool,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            f2_search_mode: true,
            f3_search_filter: true,
            f5_refresh: true,
            f11_fullscreen: true,
            f12_settings: true,
            ctrl_comma_settings: true,
            ctrl_h_hidden: true,
            ctrl_shift_h_system: true,
            del_trash: true,
            shift_del_permanent: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub column_order: Vec<ColumnId>,
    #[serde(default = "default_sort_rules")]
    pub sort_rules: Vec<SortRule>,
    pub ignore_dev_caches: bool,
    pub custom_excludes: String,
    pub last_path: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub remember_last_path: bool,
    #[serde(default)]
    pub usage_basis: UsageBasis,
    #[serde(default)]
    pub show_hidden_files: bool,
    #[serde(default)]
    pub show_system_files: bool,
    #[serde(default = "default_true")]
    pub show_locked_folders: bool,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
    #[serde(default)]
    pub quick_search_mode: QuickSearchMode,
    #[serde(default)]
    pub theme: AppTheme,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            column_order: ColumnId::default_order(),
            sort_rules: default_sort_rules(),
            ignore_dev_caches: true,
            custom_excludes: ".git, node_modules, target, .cache, .venv".to_string(),
            last_path: None,
            remember_last_path: true,
            usage_basis: UsageBasis::Folder,
            show_hidden_files: false,
            show_system_files: false,
            show_locked_folders: true,
            shortcuts: ShortcutSettings::default(),
            quick_search_mode: QuickSearchMode::JumpToPrefix,
            theme: AppTheme::Dark,
        }
    }
}

impl AppConfig {
    /// Location of `config.json`.
    /// Portable: next to the executable if a config already lives there (or, on Windows, the folder is writable).
    /// Otherwise the per-user config directory, so installs in /usr/bin, AppImages and .app bundles still persist settings.
    pub fn config_path() -> PathBuf {
        let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf()));

        if let Some(dir) = &exe_dir {
            let portable = dir.join("config.json");
            if portable.is_file() {
                return portable;
            }
            #[cfg(windows)]
            if Self::dir_is_writable(dir) {
                return portable;
            }
        }

        #[cfg(windows)]
        let base = std::env::var_os("APPDATA").map(PathBuf::from);
        #[cfg(target_os = "macos")]
        let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
        #[cfg(all(unix, not(target_os = "macos")))]
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));

        if let Some(base) = base {
            let dir = base.join("FreeDataDiet");
            if std::fs::create_dir_all(&dir).is_ok() {
                return dir.join("config.json");
            }
        }
        exe_dir.map(|d| d.join("config.json")).unwrap_or_else(|| PathBuf::from("config.json"))
    }

    #[cfg(windows)]
    fn dir_is_writable(dir: &std::path::Path) -> bool {
        let probe = dir.join(".fdd_write_test");
        let ok = std::fs::write(&probe, b"").is_ok();
        let _ = std::fs::remove_file(&probe);
        ok
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&contents) {
                    if !cfg.column_order.contains(&ColumnId::Type) {
                        cfg.column_order.insert(0, ColumnId::Type);
                    }
                    for col in ColumnId::all() {
                        if !cfg.column_order.contains(col) {
                            cfg.column_order.push(*col);
                        }
                    }
                    if cfg.sort_rules.is_empty() {
                        cfg.sort_rules = default_sort_rules();
                    }
                    return cfg;
                }
            }
            // Unreadable or corrupt file: keep a copy instead of silently overwriting the user's settings
            let _ = std::fs::rename(&path, path.with_extension("json.bak"));
        }
        let cfg = Self::default();
        let _ = cfg.save();
        cfg
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        // Write to a temp file and rename so a crash mid-write cannot leave a truncated config
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}
