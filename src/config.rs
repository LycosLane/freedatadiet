use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::model::{SortBy, SortDirection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColumnId {
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
            ColumnId::Name => 0.0, // Flexible / remaining space
            ColumnId::Size => 95.0,
            ColumnId::AllocatedSize => 95.0,
            ColumnId::Usage => 130.0,
            ColumnId::Items => 85.0,
            ColumnId::Modified => 135.0,
        }
    }

    pub fn sort_by(&self) -> Option<SortBy> {
        match self {
            ColumnId::Name => Some(SortBy::Name),
            ColumnId::Size => Some(SortBy::Size),
            ColumnId::AllocatedSize => Some(SortBy::AllocatedSize),
            ColumnId::Usage => Some(SortBy::Size),
            ColumnId::Items => Some(SortBy::ItemCount),
            ColumnId::Modified => Some(SortBy::Modified),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub column_order: Vec<ColumnId>,
    pub sort_by: SortBy,
    pub sort_direction: SortDirection,
    pub ignore_dev_caches: bool,
    pub custom_excludes: String,
    pub last_path: Option<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            column_order: ColumnId::default_order(),
            sort_by: SortBy::Size,
            sort_direction: SortDirection::Descending,
            ignore_dev_caches: true,
            custom_excludes: ".git, node_modules, target, .cache, .venv".to_string(),
            last_path: None,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.join("config.json");
            }
        }
        PathBuf::from("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&contents) {
                    for col in ColumnId::all() {
                        if !cfg.column_order.contains(col) {
                            cfg.column_order.push(*col);
                        }
                    }
                    return cfg;
                }
            }
        }
        let cfg = Self::default();
        let _ = cfg.save();
        cfg
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)?;
        Ok(())
    }
}
