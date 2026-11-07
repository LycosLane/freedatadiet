use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::SystemTime;


use crate::config::{DuplicateCriteriaConfig, DuplicateRulePreference};
use crate::duplicate_hashing::{
    hash_full, hash_prefix, hash_suffix, refine, Group, Hash, ScanContext, PREFIX_HASH_LEN,
    SUFFIX_HASH_LEN,
};
use crate::model::FileEntry;
use crate::platform::{get_file_id_and_links, FileId};


#[derive(Debug, Clone)]
pub struct DuplicateOccurrence {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub file_id: Option<FileId>,
    pub is_hardlink: bool,
    pub is_system_protected: bool,
    pub is_selected: bool,
}

#[derive(Debug, Clone)]
pub struct DuplicateCluster {
    pub cluster_id: String,
    pub primary_name: String,
    pub file_size: u64,
    pub is_group_selected: bool,
    pub occurrences: Vec<DuplicateOccurrence>,
}

impl DuplicateCluster {
    /// Total bytes wasted by redundant copies (excluding 1 original if not all selected).
    pub fn wasted_bytes(&self) -> u64 {
        if self.occurrences.len() <= 1 {
            0
        } else {
            ((self.occurrences.len() - 1) as u64).saturating_mul(self.file_size)
        }
    }

    /// Total bytes that will be reclaimed based on current selection.
    pub fn reclaimable_bytes(&self) -> u64 {
        let selected_count = self.occurrences.iter().filter(|o| o.is_selected).count();
        (selected_count as u64).saturating_mul(self.file_size)
    }

    /// Checks if all copies are selected.
    pub fn are_all_copies_selected(&self) -> bool {
        !self.occurrences.is_empty() && self.occurrences.iter().all(|o| o.is_selected)
    }

    /// Validates the selection hierarchy according to user rule:
    /// If all copies are selected but the file header checkbox is NOT checked,
    /// complete purge is blocked. Returns Ok(()) or Err(first_occurrence_path).
    pub fn validate_deletion_safety(&self) -> Result<(), PathBuf> {
        if self.are_all_copies_selected() && !self.is_group_selected
            && let Some(first) = self.occurrences.first() {
                return Err(first.path.clone());
            }
        Ok(())
    }

    /// Number of copies eligible for deletion without allocating a vector.
    pub fn deletable_count(&self) -> usize {
        if self.are_all_copies_selected() && !self.is_group_selected {
            0
        } else {
            self.occurrences
                .iter()
                .filter(|o| o.is_selected && !o.is_system_protected)
                .count()
        }
    }

    /// Returns the paths in this cluster that are eligible for deletion.
    /// When all copies are selected but the file header checkbox (`is_group_selected`) is NOT checked,
    /// none of the copies in this cluster will be deleted.
    pub fn deletable_paths(&self) -> Vec<PathBuf> {
        if self.are_all_copies_selected() && !self.is_group_selected {
            return Vec::new();
        }
        self.occurrences
            .iter()
            .filter(|o| o.is_selected && !o.is_system_protected)
            .map(|o| o.path.clone())
            .collect()
    }

    /// Total bytes eligible for deletion in this cluster (0 if all copies are selected without header confirmation).
    pub fn deletable_bytes(&self) -> u64 {
        (self.deletable_count() as u64).saturating_mul(self.file_size)
    }

    /// Selects every copy except one keeper chosen by `pref` (first, oldest or
    /// newest by modification time). System-protected copies are never selected.
    /// Does nothing for `Off`.
    pub fn keep_one(&mut self, pref: DuplicateRulePreference) {
        if pref == DuplicateRulePreference::Off || self.occurrences.is_empty() {
            return;
        }
        let mut keep = 0;
        for (i, occ) in self.occurrences.iter().enumerate().skip(1) {
            if let (Some(cur), Some(best)) = (occ.modified, self.occurrences[keep].modified) {
                let better = match pref {
                    DuplicateRulePreference::KeepOldest => cur < best,
                    DuplicateRulePreference::KeepNewest => cur > best,
                    _ => false,
                };
                if better {
                    keep = i;
                }
            }
        }
        self.is_group_selected = false;
        for (i, occ) in self.occurrences.iter_mut().enumerate() {
            occ.is_selected = i != keep && !occ.is_system_protected;
        }
    }
    /// Toggles the group-level checkbox.
    /// When checked, automatically selects all individual copies for purge.
    /// When unchecked, deselects all copies.
    pub fn set_group_selected(&mut self, selected: bool) {
        self.is_group_selected = selected;
        for occ in &mut self.occurrences {
            if !occ.is_system_protected {
                occ.is_selected = selected;
            }
        }
    }

    /// Toggles an individual copy checkbox.
    /// If unchecking a copy, group-level selection is also cleared.
    pub fn set_copy_selected(&mut self, path: &Path, selected: bool) {
        let mut all_now_selected = true;
        for occ in &mut self.occurrences {
            if occ.path == path
                && !occ.is_system_protected {
                    occ.is_selected = selected;
                }
            if !occ.is_selected {
                all_now_selected = false;
            }
        }
        if !selected {
            self.is_group_selected = false;
        } else if all_now_selected {
            // Keep group unselected until user explicitly confirms file header
            // as required by the safety gate.
            self.is_group_selected = false;
        }
    }
}

#[derive(Debug, Clone)]
pub enum DuplicateMessage {
    Progress { stage: String, done: usize, total: usize },
    Completed(Vec<DuplicateCluster>),
}


fn subpartition_by<T, K, F>(groups: Vec<Vec<T>>, key_fn: F) -> Vec<Vec<T>>
where
    K: std::hash::Hash + Eq,
    F: Fn(&T) -> K,
{
    let mut result = Vec::new();
    for group in groups {
        let mut map: HashMap<K, Vec<T>> = HashMap::new();
        for item in group {
            map.entry(key_fn(&item)).or_default().push(item);
        }
        result.extend(map.into_values().filter(|sub| sub.len() >= 2));
    }
    result
}

/// Moves groups whose files are fully covered by `covered_bytes` into
/// `finished`; returns the groups that still need a deeper hash pass.
fn split_finished<'a>(
    hashed: Vec<(Group<'a>, Hash)>,
    covered_bytes: u64,
    finished: &mut Vec<Group<'a>>,
) -> Vec<Group<'a>> {
    let mut pending = Vec::new();
    for (group, _) in hashed {
        if group[0].size <= covered_bytes {
            finished.push(group);
        } else {
            pending.push(group);
        }
    }
    pending
}

pub struct DuplicateEngine;

impl DuplicateEngine {
    /// Detects duplicate files from a list of scanned entries according to criteria.
    /// Metadata partitions (size, name, timestamp) run first, then optional
    /// 3-stage verification (64KB prefix, 64KB suffix, streamed full SHA-256).
    /// The final result is also sent as `DuplicateMessage::Completed` when a
    /// progress channel is given. A cancelled scan returns an empty list.
    /// min_size is read live, so raising it mid-search skips the remaining small files.
    pub fn find_duplicates(
        entries: &[FileEntry],
        criteria: DuplicateCriteriaConfig,
        cancel_flag: &Arc<AtomicBool>,
        min_size: &AtomicU64,
        progress_tx: Option<&Sender<DuplicateMessage>>,
    ) -> Vec<DuplicateCluster> {
        let ctx = ScanContext { cancel: cancel_flag, min_size, tx: progress_tx };
        let Some(clusters) = Self::run_pipeline(entries, &criteria, &ctx) else {
            return Vec::new();
        };
        if let Some(tx) = progress_tx {
            let _ = tx.send(DuplicateMessage::Completed(clusters.clone()));
        }
        clusters
    }

    /// Returns `None` when cancelled.
    fn run_pipeline(
        entries: &[FileEntry],
        criteria: &DuplicateCriteriaConfig,
        ctx: &ScanContext,
    ) -> Option<Vec<DuplicateCluster>> {
        let files: Vec<&FileEntry> = entries.iter().filter(|e| !e.is_dir && e.size > 0).collect();
        if files.len() < 2 {
            return Some(Vec::new());
        }
        ctx.progress("Partitioning candidate files by size...", 0, files.len());

        let mut groups: Vec<Group> = vec![files];
        if criteria.match_size {
            groups = subpartition_by(groups, |f| f.size);
        }
        if criteria.match_name {
            groups = subpartition_by(groups, |f| f.name.to_lowercase());
        }
        if criteria.match_timestamp {
            groups = subpartition_by(groups, |f| f.modified);
        }
        if ctx.cancelled() {
            return None;
        }

        let mut finished: Vec<Group> = Vec::new();
        if criteria.verify_hash {
            let prefix_len = PREFIX_HASH_LEN as u64;
            let covered_by_both = (PREFIX_HASH_LEN + SUFFIX_HASH_LEN) as u64;

            let hashed = refine(&groups, "Hashing candidate headers (64 KB)...", 200, ctx, |f| {
                hash_prefix(f).map(Some)
            })?;
            let groups = split_finished(hashed, prefix_len, &mut finished);

            let hashed = refine(&groups, "Hashing candidate suffixes (64 KB)...", 100, ctx, |f| {
                hash_suffix(f).map(Some)
            })?;
            let groups = split_finished(hashed, covered_by_both, &mut finished);

            let hashed = refine(&groups, "Verifying exact SHA-256 hashes...", 10, ctx, |f| {
                hash_full(f, ctx.cancel)
            })?;
            finished.extend(hashed.into_iter().map(|(group, _)| group));
        } else {
            finished = groups;
        }

        let mut clusters: Vec<DuplicateCluster> =
            finished.iter().map(|group| Self::build_cluster(group)).collect();
        clusters.sort_by_key(|c| std::cmp::Reverse(c.wasted_bytes()));
        Some(clusters)
    }

    fn build_cluster(group: &[&FileEntry]) -> DuplicateCluster {
        let mut seen_ids: HashSet<FileId> = HashSet::new();
        let occurrences = group
            .iter()
            .map(|entry| {
                let file_id = std::fs::symlink_metadata(&entry.path)
                    .ok()
                    .and_then(|meta| get_file_id_and_links(&entry.path, &meta).0);
                DuplicateOccurrence {
                    path: entry.path.clone(),
                    name: entry.name.clone(),
                    size: entry.size,
                    modified: entry.modified,
                    file_id,
                    is_hardlink: file_id.is_some_and(|id| !seen_ids.insert(id)),
                    is_system_protected: entry.is_system_protected,
                    is_selected: false,
                }
            })
            .collect();

        let first = group[0];
        DuplicateCluster {
            // The first path is unique per cluster, so the id stays stable across rescans.
            cluster_id: first.path.to_string_lossy().into_owned(),
            primary_name: first.name.clone(),
            file_size: first.size,
            is_group_selected: false,
            occurrences,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_duplicate_selection_safety_hierarchy() {
        let occ1 = DuplicateOccurrence {
            path: PathBuf::from("C:\\A\\file.txt"),
            name: "file.txt".into(),
            size: 100,
            modified: None,
            file_id: None,
            is_hardlink: false,
            is_system_protected: false,
            is_selected: false,
        };
        let occ2 = DuplicateOccurrence {
            path: PathBuf::from("C:\\B\\file.txt"),
            name: "file.txt".into(),
            size: 100,
            modified: None,
            file_id: None,
            is_hardlink: false,
            is_system_protected: false,
            is_selected: false,
        };

        let mut cluster = DuplicateCluster {
            cluster_id: "c1".into(),
            primary_name: "file.txt".into(),
            file_size: 100,
            is_group_selected: false,
            occurrences: vec![occ1, occ2],
        };

        // Select only one copy: safe, validation passes
        cluster.set_copy_selected(&PathBuf::from("C:\\A\\file.txt"), true);
        assert!(!cluster.are_all_copies_selected());
        assert!(cluster.validate_deletion_safety().is_ok());
        assert_eq!(cluster.deletable_paths().len(), 1);

        // Select both copies individually, but group header unconfirmed:
        // Safety gate must trigger, return first occurrence path, and delete zero copies!
        cluster.set_copy_selected(&PathBuf::from("C:\\B\\file.txt"), true);
        assert!(cluster.are_all_copies_selected());
        assert!(!cluster.is_group_selected);
        let safety_result = cluster.validate_deletion_safety();
        assert!(safety_result.is_err());
        assert_eq!(safety_result.unwrap_err(), PathBuf::from("C:\\A\\file.txt"));
        assert!(cluster.deletable_paths().is_empty());

        // User explicitly checks the group header (file name checkbox):
        // Safety gate passes, complete purge authorized!
        cluster.set_group_selected(true);
        assert!(cluster.is_group_selected);
        assert!(cluster.are_all_copies_selected());
        assert!(cluster.validate_deletion_safety().is_ok());
        assert_eq!(cluster.deletable_paths().len(), 2);
    }

    #[test]
    fn test_two_pass_hash_detection() {
        let temp_dir = std::env::temp_dir().join("fdd_test_dups");
        let _ = std::fs::create_dir_all(&temp_dir);

        let file1 = temp_dir.join("orig.bin");
        let file2 = temp_dir.join("copy.bin");
        let file3 = temp_dir.join("diff.bin");

        // Create 8KB identical data for file1 and file2
        let mut data = vec![0x42u8; 8192];
        let mut f1 = File::create(&file1).unwrap();
        f1.write_all(&data).unwrap();

        let mut f2 = File::create(&file2).unwrap();
        f2.write_all(&data).unwrap();

        // file3 has same size (8192), but differs in byte 10
        data[10] = 0x99;
        let mut f3 = File::create(&file3).unwrap();
        f3.write_all(&data).unwrap();

        let e1 = FileEntry { size: 8192, allocated_size: 8192, modified: None, item_count: 1, is_access_denied: false, is_partially_locked: false, ..FileEntry::new("orig.bin".into(), file1.clone(), false) };
        let e2 = FileEntry { size: 8192, allocated_size: 8192, modified: None, item_count: 1, is_access_denied: false, is_partially_locked: false, ..FileEntry::new("copy.bin".into(), file2.clone(), false) };
        let e3 = FileEntry { size: 8192, allocated_size: 8192, modified: None, item_count: 1, is_access_denied: false, is_partially_locked: false, ..FileEntry::new("diff.bin".into(), file3.clone(), false) };

        let entries = vec![e1, e2, e3];
        let cancel = Arc::new(AtomicBool::new(false));

        let criteria = DuplicateCriteriaConfig {
            match_size: true,
            match_name: false,
            match_timestamp: false,
            verify_hash: true,
        };

        let clusters = DuplicateEngine::find_duplicates(&entries, criteria, &cancel, &AtomicU64::new(0), None);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].occurrences.len(), 2);
        let paths: Vec<PathBuf> = clusters[0].occurrences.iter().map(|o| o.path.clone()).collect();
        assert!(paths.contains(&file1));
        assert!(paths.contains(&file2));
        assert!(!paths.contains(&file3));

        let _ = std::fs::remove_file(&file1);
        let _ = std::fs::remove_file(&file2);
        let _ = std::fs::remove_file(&file3);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_large_files_differing_only_in_the_middle() {
        let dir = std::env::temp_dir().join("fdd_test_large_dups");
        let _ = std::fs::create_dir_all(&dir);
        let size = 400 * 1024;
        let mut data = vec![0x11u8; size];
        let mut entries = Vec::new();
        for (name, middle) in [("a.bin", 0x11u8), ("b.bin", 0x11), ("c.bin", 0x22)] {
            data[size / 2] = middle;
            let path = dir.join(name);
            File::create(&path).unwrap().write_all(&data).unwrap();
            entries.push(FileEntry { size: size as u64, allocated_size: size as u64, modified: None, item_count: 1, is_access_denied: false, is_partially_locked: false, ..FileEntry::new(name.into(), path, false) });
        }
        let criteria = DuplicateCriteriaConfig {
            match_size: true,
            match_name: false,
            match_timestamp: false,
            verify_hash: true,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let clusters = DuplicateEngine::find_duplicates(&entries, criteria, &cancel, &AtomicU64::new(0), None);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].occurrences.len(), 2);

        // A live minimum above the file size skips hashing them entirely.
        let skipped = DuplicateEngine::find_duplicates(&entries, criteria, &cancel, &AtomicU64::new(1_000_000), None);
        assert!(skipped.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_duplicate_folder_pattern_suggestion() {
        let occ1_a = DuplicateOccurrence {
            path: PathBuf::from("C:\\Keep\\photo1.jpg"),
            name: "photo1.jpg".into(),
            size: 500,
            modified: None,
            file_id: None,
            is_hardlink: false,
            is_system_protected: false,
            is_selected: false,
        };
        let occ1_b = DuplicateOccurrence {
            path: PathBuf::from("C:\\Sync\\photo1.jpg"),
            name: "photo1.jpg".into(),
            size: 500,
            modified: None,
            file_id: None,
            is_hardlink: false,
            is_system_protected: false,
            is_selected: true, // User selected this one to delete
        };

        let occ2_a = DuplicateOccurrence {
            path: PathBuf::from("C:\\Keep\\photo2.jpg"),
            name: "photo2.jpg".into(),
            size: 600,
            modified: None,
            file_id: None,
            is_hardlink: false,
            is_system_protected: false,
            is_selected: false,
        };
        let occ2_b = DuplicateOccurrence {
            path: PathBuf::from("C:\\Sync\\photo2.jpg"),
            name: "photo2.jpg".into(),
            size: 600,
            modified: None,
            file_id: None,
            is_hardlink: false,
            is_system_protected: false,
            is_selected: false, // Unselected candidate in same folder pair
        };

        let cluster1 = DuplicateCluster {
            cluster_id: "c1".into(),
            primary_name: "photo1.jpg".into(),
            file_size: 500,
            is_group_selected: false,
            occurrences: vec![occ1_a, occ1_b],
        };
        let cluster2 = DuplicateCluster {
            cluster_id: "c2".into(),
            primary_name: "photo2.jpg".into(),
            file_size: 600,
            is_group_selected: false,
            occurrences: vec![occ2_a, occ2_b],
        };

        let clusters = vec![cluster1, cluster2];
        let toggled = PathBuf::from("C:\\Sync\\photo1.jpg");
        let sugg = crate::duplicate_suggestion::find_manual_folder_suggestion(&clusters, &toggled, true)
            .expect("Must find manual folder pattern suggestion");
        assert_eq!(sugg.folder_path, PathBuf::from("C:\\Sync"));
        assert_eq!(sugg.candidate_paths, vec![PathBuf::from("C:\\Sync\\photo2.jpg")]);
        assert_eq!(sugg.reclaimable_bytes, 600);
        assert_eq!(sugg.action, crate::duplicate_suggestion::SuggestionAction::Select);
    }
}