use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{AppConfig, SavedLocation, StartupLocationMode};

#[derive(Debug, Clone)]
pub struct LocationStatus {
    pub is_accessible: bool,
    pub last_checked: Instant,
}

#[derive(Clone)]
pub struct LocationManager {
    statuses: Arc<Mutex<HashMap<PathBuf, LocationStatus>>>,
}

impl Default for LocationManager {
    fn default() -> Self {
        Self {
            statuses: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl LocationManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Non-blocking check of whether a path is accessible.
    /// Uses cached results if checked within the last 5 seconds.
    pub fn is_accessible(&self, path: &Path) -> bool {
        let now = Instant::now();
        let map = self.statuses.lock().unwrap_or_else(|p| p.into_inner());

        if let Some(status) = map.get(path)
            && now.duration_since(status.last_checked) < Duration::from_secs(5) {
                return status.is_accessible;
            }

        // Spawn background probe if expired or missing so UI never blocks on offline SMB/USB
        let path_clone = path.to_path_buf();
        let statuses_clone = Arc::clone(&self.statuses);

        thread::spawn(move || {
            let accessible = path_clone.exists();
            let mut map = statuses_clone.lock().unwrap_or_else(|p| p.into_inner());
            map.insert(
                path_clone,
                LocationStatus {
                    is_accessible: accessible,
                    last_checked: Instant::now(),
                },
            );
        });

        // Default to true while first check is running to avoid flickering
        map.get(path).map(|s| s.is_accessible).unwrap_or(true)
    }

    /// Resolves the startup path based on configuration and availability.
    pub fn resolve_startup_path(&self, config: &AppConfig, fallback_drive: &Path) -> PathBuf {
        match config.startup_mode {
            StartupLocationMode::FirstFavorite => {
                if let Some(first_fav) = config.saved_locations.iter().find(|l| l.is_favorite)
                    && first_fav.path.exists() {
                        return first_fav.path.clone();
                    }
                if let Some(first) = config.saved_locations.first()
                    && first.path.exists() {
                        return first.path.clone();
                    }
                if config.remember_last_path
                    && let Some(last) = &config.last_path
                        && last.exists() {
                            return last.clone();
                        }
                fallback_drive.to_path_buf()
            }
            StartupLocationMode::LastVisitedPath => {
                if config.remember_last_path
                    && let Some(last) = &config.last_path
                        && last.exists() {
                            return last.clone();
                        }
                fallback_drive.to_path_buf()
            }
            StartupLocationMode::PrimaryDrive => fallback_drive.to_path_buf(),
            StartupLocationMode::CustomPath => {
                if let Some(custom) = &config.custom_startup_path
                    && custom.exists() {
                        return custom.clone();
                    }
                fallback_drive.to_path_buf()
            }
        }
    }

    /// Adds a directory to saved locations.
    pub fn add_location(config: &mut AppConfig, path: PathBuf, name: Option<String>) {
        let canonical_str = path.to_string_lossy();
        if config.saved_locations.iter().any(|l| l.path == path) {
            return;
        }

        let display_name = name.unwrap_or_else(|| {
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| canonical_str.to_string())
        });

        let id = format!("loc_{}", config.saved_locations.len() + 1);
        config.saved_locations.push(SavedLocation {
            id,
            name: display_name,
            path,
            is_favorite: true,
        });
        let _ = config.save();
    }

    /// Removes a saved location by path.
    pub fn remove_location(config: &mut AppConfig, path: &Path) {
        config.saved_locations.retain(|l| l.path != path);
        let _ = config.save();
    }

    /// Toggles favorite status for a path.
    pub fn toggle_favorite(config: &mut AppConfig, path: &Path) -> bool {
        if let Some(existing) = config.saved_locations.iter_mut().find(|l| l.path == path) {
            existing.is_favorite = !existing.is_favorite;
            let status = existing.is_favorite;
            let _ = config.save();
            status
        } else {
            Self::add_location(config, path.to_path_buf(), None);
            true
        }
    }

    /// Reorders a location up or down.
    pub fn move_location(config: &mut AppConfig, index: usize, up: bool) {
        if up && index > 0 {
            config.saved_locations.swap(index, index - 1);
            let _ = config.save();
        } else if !up && index + 1 < config.saved_locations.len() {
            config.saved_locations.swap(index, index + 1);
            let _ = config.save();
        }
    }

    /// Sets the given index as the default startup entry (index 0).
    pub fn set_as_startup(config: &mut AppConfig, index: usize) {
        if index < config.saved_locations.len() {
            let item = config.saved_locations.remove(index);
            config.saved_locations.insert(0, item);
            config.startup_mode = StartupLocationMode::FirstFavorite;
            let _ = config.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_manager_reorder_and_startup() {
        let mut config = AppConfig::default();
        config.saved_locations.clear();

        LocationManager::add_location(&mut config, PathBuf::from("C:\\FolderA"), Some("Folder A".into()));
        LocationManager::add_location(&mut config, PathBuf::from("C:\\FolderB"), Some("Folder B".into()));
        LocationManager::add_location(&mut config, PathBuf::from("C:\\FolderC"), Some("Folder C".into()));

        assert_eq!(config.saved_locations.len(), 3);
        assert_eq!(config.saved_locations[0].name, "Folder A");
        assert_eq!(config.saved_locations[1].name, "Folder B");
        assert_eq!(config.saved_locations[2].name, "Folder C");

        // Move Folder B up
        LocationManager::move_location(&mut config, 1, true);
        assert_eq!(config.saved_locations[0].name, "Folder B");
        assert_eq!(config.saved_locations[1].name, "Folder A");

        // Set Folder C as startup (promotes to index 0)
        LocationManager::set_as_startup(&mut config, 2);
        assert_eq!(config.saved_locations[0].name, "Folder C");
        assert_eq!(config.startup_mode, StartupLocationMode::FirstFavorite);

        // Toggle favorite
        let is_fav = LocationManager::toggle_favorite(&mut config, &PathBuf::from("C:\\FolderC"));
        assert!(!is_fav);
        let is_fav_again = LocationManager::toggle_favorite(&mut config, &PathBuf::from("C:\\FolderC"));
        assert!(is_fav_again);

        // Remove location
        LocationManager::remove_location(&mut config, &PathBuf::from("C:\\FolderB"));
        assert_eq!(config.saved_locations.len(), 2);
        assert!(!config.saved_locations.iter().any(|l| l.name == "Folder B"));
    }
}
