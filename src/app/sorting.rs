use eframe::egui::Color32;

use crate::config::{ColumnId, SortRule};
use crate::model::{FileEntry, SortDirection};

use super::FreeDataDietApp;

pub(super) fn get_type_sort_key(e: &FileEntry) -> (u8, String) {
    if e.is_dir {
        if e.is_reparse_point {
            (1, e.name.to_lowercase())
        } else {
            (0, e.name.to_lowercase())
        }
    } else if let Some(ext) = e.path.extension().and_then(|s| s.to_str()) {
        (2, ext.to_lowercase())
    } else {
        (3, e.name.to_lowercase())
    }
}

pub fn get_type_badge(entry: &FileEntry, is_dark: bool) -> (String, Color32, &'static str) {
    if entry.is_reparse_point {
        (
            "[LINK]".to_string(),
            if is_dark { Color32::from_rgb(200, 160, 255) } else { Color32::from_rgb(130, 70, 200) },
            "Link: Directory junction or symbolic link to another folder",
        )
    } else if entry.is_system_file {
        (
            "[SYS]".to_string(),
            if is_dark { Color32::from_rgb(255, 175, 40) } else { Color32::from_rgb(200, 110, 0) },
            "System: Protected system directory or file",
        )
    } else if entry.is_access_denied {
        (
            "[LOCK]".to_string(),
            if is_dark { Color32::from_rgb(255, 90, 90) } else { Color32::from_rgb(200, 30, 30) },
            "Locked: Permission denied, folder cannot be scanned",
        )
    } else if entry.is_partially_locked {
        (
            "[PART]".to_string(),
            if is_dark { Color32::from_rgb(255, 190, 70) } else { Color32::from_rgb(210, 130, 10) },
            "Partially Locked: Accessible, but some subfolders were restricted",
        )
    } else if entry.is_dir {
        (
            "[DIR]".to_string(),
            if is_dark { Color32::from_rgb(80, 200, 255) } else { Color32::from_rgb(0, 120, 210) },
            "Directory: Standard folder",
        )
    } else if let Some(ext) = entry.path.extension().and_then(|e| e.to_str()) {
        let ext_upper = ext.to_uppercase();
        if ext_upper.len() <= 4 {
            (
                format!("[{}]", ext_upper),
                if is_dark { Color32::from_rgb(160, 185, 205) } else { Color32::from_rgb(60, 90, 120) },
                "File",
            )
        } else {
            (
                "[FILE]".to_string(),
                if is_dark { Color32::from_rgb(150, 160, 175) } else { Color32::from_rgb(90, 100, 115) },
                "File",
            )
        }
    } else {
        (
            "[FILE]".to_string(),
            if is_dark { Color32::from_rgb(150, 160, 175) } else { Color32::from_rgb(90, 100, 115) },
            "File",
        )
    }
}

impl FreeDataDietApp {
    pub(super) fn sort_entries(&mut self) {
        let rules = self.config.sort_rules.clone();

        self.entries.sort_by(|a, b| {
            for rule in &rules {
                let directed = match rule.column {
                    ColumnId::Type => {
                        let a_key = get_type_sort_key(a);
                        let b_key = get_type_sort_key(b);
                        let cmp = a_key.cmp(&b_key);
                        match rule.direction {
                            SortDirection::Ascending => cmp,
                            SortDirection::Descending => cmp.reverse(),
                        }
                    }
                    ColumnId::Name => {
                        let cmp = a.name.chars().flat_map(char::to_lowercase).cmp(b.name.chars().flat_map(char::to_lowercase));
                        match rule.direction {
                            SortDirection::Ascending => cmp,
                            SortDirection::Descending => cmp.reverse(),
                        }
                    }
                    ColumnId::Size | ColumnId::Usage => {
                        if a.is_scanning != b.is_scanning {
                            match rule.direction {
                                SortDirection::Descending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Less
                                    } else {
                                        std::cmp::Ordering::Greater
                                    }
                                }
                                SortDirection::Ascending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Greater
                                    } else {
                                        std::cmp::Ordering::Less
                                    }
                                }
                            }
                        } else {
                            let cmp = a.size.cmp(&b.size);
                            match rule.direction {
                                SortDirection::Ascending => cmp,
                                SortDirection::Descending => cmp.reverse(),
                            }
                        }
                    }
                    ColumnId::AllocatedSize => {
                        if a.is_scanning != b.is_scanning {
                            match rule.direction {
                                SortDirection::Descending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Less
                                    } else {
                                        std::cmp::Ordering::Greater
                                    }
                                }
                                SortDirection::Ascending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Greater
                                    } else {
                                        std::cmp::Ordering::Less
                                    }
                                }
                            }
                        } else {
                            let cmp = a.allocated_size.cmp(&b.allocated_size);
                            match rule.direction {
                                SortDirection::Ascending => cmp,
                                SortDirection::Descending => cmp.reverse(),
                            }
                        }
                    }
                    ColumnId::Items => {
                        if a.is_scanning != b.is_scanning {
                            match rule.direction {
                                SortDirection::Descending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Less
                                    } else {
                                        std::cmp::Ordering::Greater
                                    }
                                }
                                SortDirection::Ascending => {
                                    if a.is_scanning {
                                        std::cmp::Ordering::Greater
                                    } else {
                                        std::cmp::Ordering::Less
                                    }
                                }
                            }
                        } else {
                            let cmp = a.item_count.cmp(&b.item_count);
                            match rule.direction {
                                SortDirection::Ascending => cmp,
                                SortDirection::Descending => cmp.reverse(),
                            }
                        }
                    }
                    ColumnId::Modified => {
                        let cmp = a.modified.cmp(&b.modified);
                        match rule.direction {
                            SortDirection::Ascending => cmp,
                            SortDirection::Descending => cmp.reverse(),
                        }
                    }
                };

                if directed != std::cmp::Ordering::Equal {
                    return directed;
                }
            }

            a.name.chars().flat_map(char::to_lowercase).cmp(b.name.chars().flat_map(char::to_lowercase))
        });

        self.visible_indices_dirty = true;
        self.update_visible_indices();
    }

    pub(super) fn toggle_sort_column(&mut self, col: ColumnId) {
        if let Some(pos) = self.config.sort_rules.iter().position(|r| r.column == col) {
            let default_dir = match col {
                ColumnId::Name | ColumnId::Type => SortDirection::Ascending,
                _ => SortDirection::Descending,
            };
            if self.config.sort_rules[pos].direction == default_dir {
                self.config.sort_rules[pos].direction = match default_dir {
                    SortDirection::Ascending => SortDirection::Descending,
                    SortDirection::Descending => SortDirection::Ascending,
                };
            } else {
                self.config.sort_rules.remove(pos);
            }
        } else {
            let default_dir = match col {
                ColumnId::Name | ColumnId::Type => SortDirection::Ascending,
                _ => SortDirection::Descending,
            };
            self.config.sort_rules.push(SortRule {
                column: col,
                direction: default_dir,
            });
        }
        self.sort_entries();
        self.save_config();
    }
}
