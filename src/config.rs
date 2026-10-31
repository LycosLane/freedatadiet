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
    pub ctrl_f_filter: bool,
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
            ctrl_f_filter: true,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedLocation {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub is_favorite: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StartupLocationMode {
    FirstFavorite,
    #[default]
    LastVisitedPath,
    PrimaryDrive,
    CustomPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateCriteriaConfig {
    #[serde(default = "default_true")]
    pub match_size: bool,
    #[serde(default = "default_true")]
    pub match_name: bool,
    #[serde(default)]
    pub match_timestamp: bool,
    #[serde(default)]
    pub verify_hash: bool,
}

impl Default for DuplicateCriteriaConfig {
    fn default() -> Self {
        Self {
            match_size: true,
            match_name: true,
            match_timestamp: false,
            verify_hash: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryFilterConfig {
    #[serde(default = "default_true")]
    pub show_images: bool,
    #[serde(default = "default_true")]
    pub show_videos: bool,
    #[serde(default = "default_true")]
    pub show_audio: bool,
    #[serde(default = "default_true")]
    pub show_documents: bool,
    #[serde(default = "default_true")]
    pub show_archives: bool,
}

impl Default for CategoryFilterConfig {
    fn default() -> Self {
        Self {
            show_images: true,
            show_videos: true,
            show_audio: true,
            show_documents: true,
            show_archives: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExcludeScope {
    #[default]
    HideFromScan,
    IgnoreForDuplicates,
}

impl ExcludeScope {
    pub fn label(&self) -> &'static str {
        match self {
            ExcludeScope::HideFromScan => "Hide from Scan",
            ExcludeScope::IgnoreForDuplicates => "Ignore for Duplicates",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExcludedLocation {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub scope: ExcludeScope,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DuplicateRulePreference {
    #[default]
    Off,
    KeepOldest,
    KeepNewest,
    KeepFirst,
}

impl DuplicateRulePreference {
    pub fn label(&self) -> &'static str {
        match self {
            DuplicateRulePreference::Off => "Off",
            DuplicateRulePreference::KeepOldest => "Keep Oldest",
            DuplicateRulePreference::KeepNewest => "Keep Newest",
            DuplicateRulePreference::KeepFirst => "Keep First",
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            DuplicateRulePreference::Off => DuplicateRulePreference::KeepOldest,
            DuplicateRulePreference::KeepOldest => DuplicateRulePreference::KeepNewest,
            DuplicateRulePreference::KeepNewest => DuplicateRulePreference::KeepFirst,
            DuplicateRulePreference::KeepFirst => DuplicateRulePreference::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryDuplicateRules {
    #[serde(default)]
    pub images: DuplicateRulePreference,
    #[serde(default)]
    pub videos: DuplicateRulePreference,
    #[serde(default)]
    pub audio: DuplicateRulePreference,
    #[serde(default)]
    pub documents: DuplicateRulePreference,
    #[serde(default)]
    pub archives: DuplicateRulePreference,
}

impl Default for CategoryDuplicateRules {
    fn default() -> Self {
        Self {
            images: DuplicateRulePreference::KeepOldest,
            videos: DuplicateRulePreference::KeepOldest,
            audio: DuplicateRulePreference::KeepNewest,
            documents: DuplicateRulePreference::KeepNewest,
            archives: DuplicateRulePreference::KeepNewest,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExcludePattern {
    pub pattern: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub is_preset: bool,
}

fn default_config_version() -> u32 {
    1
}

fn default_custom_excludes() -> String {
    ".git, node_modules, target, .cache, .venv".to_string()
}

pub fn default_excluded_patterns() -> Vec<ExcludePattern> {
    vec![
        ExcludePattern { pattern: ".git".into(), enabled: true, is_preset: true },
        ExcludePattern { pattern: "node_modules".into(), enabled: true, is_preset: true },
        ExcludePattern { pattern: "target".into(), enabled: true, is_preset: true },
        ExcludePattern { pattern: ".cache".into(), enabled: true, is_preset: true },
        ExcludePattern { pattern: ".venv".into(), enabled: true, is_preset: true },
        ExcludePattern { pattern: "$RECYCLE.BIN".into(), enabled: true, is_preset: true },
        ExcludePattern { pattern: "System Volume Information".into(), enabled: true, is_preset: true },
    ]
}

pub fn default_saved_locations() -> Vec<SavedLocation> {
    let mut locs = Vec::new();
    #[cfg(windows)]
    {
        if let Some(user_profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
            let downloads = user_profile.join("Downloads");
            if downloads.exists() {
                locs.push(SavedLocation {
                    id: "fav_downloads".into(),
                    name: "Downloads".into(),
                    path: downloads,
                    is_favorite: true,
                });
            }
            let docs = user_profile.join("Documents");
            if docs.exists() {
                locs.push(SavedLocation {
                    id: "fav_documents".into(),
                    name: "Documents".into(),
                    path: docs,
                    is_favorite: true,
                });
            }
            let desktop = user_profile.join("Desktop");
            if desktop.exists() {
                locs.push(SavedLocation {
                    id: "fav_desktop".into(),
                    name: "Desktop".into(),
                    path: desktop,
                    is_favorite: true,
                });
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let downloads = home.join("Downloads");
            if downloads.exists() {
                locs.push(SavedLocation {
                    id: "fav_downloads".into(),
                    name: "Downloads".into(),
                    path: downloads,
                    is_favorite: true,
                });
            }
            let docs = home.join("Documents");
            if docs.exists() {
                locs.push(SavedLocation {
                    id: "fav_documents".into(),
                    name: "Documents".into(),
                    path: docs,
                    is_favorite: true,
                });
            }
        }
    }
    locs
}

fn default_ui_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_config_version")]
    pub config_version: u32,

    #[serde(default = "ColumnId::default_order")]
    pub column_order: Vec<ColumnId>,
    #[serde(default = "default_sort_rules")]
    pub sort_rules: Vec<SortRule>,

    #[serde(default = "default_true")]
    pub ignore_dev_caches: bool,
    #[serde(default = "default_true")]
    pub ignore_media_caches: bool,
    #[serde(default = "default_true")]
    pub ignore_temp_caches: bool,
    #[serde(default = "default_custom_excludes")]
    pub custom_excludes: String,
    #[serde(default = "default_excluded_patterns")]
    pub excluded_patterns: Vec<ExcludePattern>,

    #[serde(default = "default_saved_locations")]
    pub saved_locations: Vec<SavedLocation>,
    #[serde(default)]
    pub startup_mode: StartupLocationMode,
    #[serde(default)]
    pub custom_startup_path: Option<PathBuf>,

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
    pub flatten_subfolders: bool,
    #[serde(default)]
    pub category_filters: CategoryFilterConfig,

    #[serde(default)]
    pub show_duplicates_only: bool,
    #[serde(default)]
    pub duplicate_criteria: DuplicateCriteriaConfig,

    #[serde(default)]
    pub shortcuts: ShortcutSettings,
    #[serde(default)]
    pub quick_search_mode: QuickSearchMode,
    #[serde(default)]
    pub theme: AppTheme,
    #[serde(default = "default_true")]
    pub show_sidebar: bool,
    #[serde(default = "default_true")]
    pub show_shortcuts_in_settings: bool,
    #[serde(default = "default_true")]
    pub enable_duplicate_suggestions: bool,
    #[serde(default)]
    pub category_duplicate_rules: CategoryDuplicateRules,
    #[serde(default)]
    pub excluded_locations: Vec<ExcludedLocation>,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            config_version: 2,
            column_order: ColumnId::default_order(),
            sort_rules: default_sort_rules(),
            ignore_dev_caches: true,
            ignore_media_caches: true,
            ignore_temp_caches: true,
            custom_excludes: default_custom_excludes(),
            excluded_patterns: default_excluded_patterns(),
            saved_locations: default_saved_locations(),
            startup_mode: StartupLocationMode::LastVisitedPath,
            custom_startup_path: None,
            last_path: None,
            remember_last_path: true,
            usage_basis: UsageBasis::Folder,
            show_hidden_files: false,
            show_system_files: false,
            show_locked_folders: true,
            flatten_subfolders: false,
            category_filters: CategoryFilterConfig::default(),
            show_duplicates_only: false,
            duplicate_criteria: DuplicateCriteriaConfig::default(),
            shortcuts: ShortcutSettings::default(),
            quick_search_mode: QuickSearchMode::JumpToPrefix,
            theme: AppTheme::Dark,
            show_sidebar: true,
            show_shortcuts_in_settings: true,
            enable_duplicate_suggestions: true,
            category_duplicate_rules: CategoryDuplicateRules::default(),
            excluded_locations: Vec::new(),
            ui_scale: 1.0,
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

    pub fn migrate_legacy(mut self) -> Self {
        if self.config_version < 2 {
            for item in self.custom_excludes.split(',') {
                let trimmed = item.trim();
                if !trimmed.is_empty()
                    && !self
                        .excluded_patterns
                        .iter()
                        .any(|p| p.pattern.eq_ignore_ascii_case(trimmed))
                {
                    self.excluded_patterns.push(ExcludePattern {
                        pattern: trimmed.to_string(),
                        enabled: self.ignore_dev_caches,
                        is_preset: false,
                    });
                }
            }
            if self.saved_locations.is_empty() {
                self.saved_locations = default_saved_locations();
            }
            self.config_version = 2;
        }
        self
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&contents) {
                    cfg = cfg.migrate_legacy();
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
        if std::fs::rename(&tmp, &path).is_err() {
            // Fallback for filesystem locking (e.g. Windows AV/file indexer holding handle)
            std::fs::copy(&tmp, &path)?;
            let _ = std::fs::remove_file(&tmp);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_v1_config_migration() {
        let legacy_json = r#"{
            "column_order": ["Type", "Name", "Size"],
            "sort_rules": [{"column": "Size", "direction": "Descending"}],
            "ignore_dev_caches": true,
            "custom_excludes": ".git, node_modules, custom_cache",
            "last_path": null,
            "remember_last_path": true
        }"#;

        let parsed: AppConfig = serde_json::from_str(legacy_json).expect("Legacy v1 config must deserialize with defaults");
        let migrated = parsed.migrate_legacy();

        assert_eq!(migrated.config_version, 2);
        assert!(migrated.excluded_patterns.iter().any(|p| p.pattern == "custom_cache"));
        assert!(migrated.category_filters.show_images);
        assert!(migrated.duplicate_criteria.match_size);
        assert!(!migrated.show_duplicates_only);
        assert!(migrated.enable_duplicate_suggestions);
        assert_eq!(migrated.category_duplicate_rules.images, DuplicateRulePreference::KeepOldest);
        assert!(migrated.excluded_locations.is_empty());
    }

    #[test]
    fn test_duplicate_rule_cycle() {
        let rule = DuplicateRulePreference::Off;
        let rule = rule.cycle();
        assert_eq!(rule, DuplicateRulePreference::KeepOldest);
        let rule = rule.cycle();
        assert_eq!(rule, DuplicateRulePreference::KeepNewest);
        let rule = rule.cycle();
        assert_eq!(rule, DuplicateRulePreference::KeepFirst);
        let rule = rule.cycle();
        assert_eq!(rule, DuplicateRulePreference::Off);
    }
}
