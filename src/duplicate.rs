use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::config::DuplicateCriteriaConfig;
use crate::model::FileEntry;
use crate::platform::{get_file_id_and_links, FileId};

pub const PREFIX_HASH_LEN: usize = 4096;
pub const STREAM_BUFFER_SIZE: usize = 64 * 1024;

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
    pub hash: Option<[u8; 32]>,
    pub is_group_selected: bool,
    pub occurrences: Vec<DuplicateOccurrence>,
}

impl DuplicateCluster {
    /// Total bytes wasted by redundant copies (excluding 1 original if not all selected).
    pub fn wasted_bytes(&self) -> u64 {
        if self.occurrences.len() <= 1 {
            0
        } else {
            (self.occurrences.len() - 1) as u64 * self.file_size
        }
    }

    /// Total bytes that will be reclaimed based on current selection.
    pub fn reclaimable_bytes(&self) -> u64 {
        let selected_count = self.occurrences.iter().filter(|o| o.is_selected).count();
        selected_count as u64 * self.file_size
    }

    /// Checks if all copies are selected.
    pub fn are_all_copies_selected(&self) -> bool {
        !self.occurrences.is_empty() && self.occurrences.iter().all(|o| o.is_selected)
    }

    /// Validates the selection hierarchy according to user rule:
    /// If all copies are selected but the file header checkbox is NOT checked,
    /// complete purge is blocked. Returns Ok(()) or Err(first_occurrence_path).
    pub fn validate_deletion_safety(&self) -> Result<(), PathBuf> {
        if self.are_all_copies_selected() && !self.is_group_selected {
            if let Some(first) = self.occurrences.first() {
                return Err(first.path.clone());
            }
        }
        Ok(())
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
        self.deletable_paths().len() as u64 * self.file_size
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
            if occ.path == path {
                if !occ.is_system_protected {
                    occ.is_selected = selected;
                }
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

pub struct DuplicateSuggestion {
    pub folder_path: PathBuf,
    pub folder_name: String,
    pub candidate_paths: Vec<PathBuf>,
    pub reclaimable_bytes: u64,
}

pub struct DuplicateEngine;

impl DuplicateEngine {
    /// Detects duplicate files from a list of scanned entries according to criteria.
    /// Uses two-pass verification (4KB Prefix Hash -> 64KB Chunked Full SHA-256).
    pub fn find_duplicates(
        entries: &[FileEntry],
        criteria: DuplicateCriteriaConfig,
        cancel_flag: &Arc<AtomicBool>,
    ) -> Vec<DuplicateCluster> {
        let files: Vec<&FileEntry> = entries
            .iter()
            .filter(|e| !e.is_dir && e.size > 0)
            .collect();

        if files.len() < 2 {
            return Vec::new();
        }

        // Pass 0: Group by criteria keys (size, name, timestamp)
        let mut initial_clusters: HashMap<String, Vec<&FileEntry>> = HashMap::new();
        for file in files {
            if cancel_flag.load(Ordering::Relaxed) {
                return Vec::new();
            }
            if let Some(key) = Self::build_key(file, &criteria) {
                initial_clusters.entry(key).or_default().push(file);
            }
        }

        let mut clusters = Vec::new();
        let mut cluster_idx = 1;

        for (_key, group) in initial_clusters {
            if cancel_flag.load(Ordering::Relaxed) {
                return Vec::new();
            }
            if group.len() < 2 {
                continue;
            }

            if !criteria.verify_hash {
                // Return metadata-matched duplicates directly
                let cluster = Self::build_cluster_from_group(
                    format!("cluster_{cluster_idx}"),
                    &group,
                    None,
                );
                clusters.push(cluster);
                cluster_idx += 1;
                continue;
            }

            // Pass 1: 4 KB Prefix Hash
            let mut prefix_map: HashMap<[u8; 32], Vec<&FileEntry>> = HashMap::new();
            for file in group {
                if cancel_flag.load(Ordering::Relaxed) {
                    return Vec::new();
                }
                if let Ok(Some(prefix_hash)) = Self::hash_prefix(&file.path, cancel_flag) {
                    prefix_map.entry(prefix_hash).or_default().push(file);
                }
            }

            // Pass 2: Full Streaming SHA-256 for survivors
            for (prefix_hash, candidates) in prefix_map {
                if candidates.len() < 2 {
                    continue;
                }

                let mut full_hash_map: HashMap<[u8; 32], Vec<&FileEntry>> = HashMap::new();

                for file in candidates {
                    if cancel_flag.load(Ordering::Relaxed) {
                        return Vec::new();
                    }
                    if file.size <= PREFIX_HASH_LEN as u64 {
                        // Prefix is already the entire file
                        full_hash_map.entry(prefix_hash).or_default().push(file);
                    } else if let Ok(Some(full_hash)) =
                        Self::hash_full_file(&file.path, file.size, cancel_flag)
                    {
                        full_hash_map.entry(full_hash).or_default().push(file);
                    }
                }

                for (verified_hash, verified_files) in full_hash_map {
                    if verified_files.len() >= 2 {
                        let cluster = Self::build_cluster_from_group(
                            format!("cluster_h{cluster_idx}"),
                            &verified_files,
                            Some(verified_hash),
                        );
                        clusters.push(cluster);
                        cluster_idx += 1;
                    }
                }
            }
        }

        clusters.sort_by(|a, b| b.wasted_bytes().cmp(&a.wasted_bytes()));
        clusters
    }

    fn build_key(file: &FileEntry, criteria: &DuplicateCriteriaConfig) -> Option<String> {
        let mut parts = Vec::new();
        if criteria.match_size {
            parts.push(format!("S:{}", file.size));
        }
        if criteria.match_name {
            parts.push(format!("N:{}", file.name.to_lowercase()));
        }
        if criteria.match_timestamp {
            if let Some(m) = file.modified {
                if let Ok(dur) = m.duration_since(SystemTime::UNIX_EPOCH) {
                    parts.push(format!("T:{}", dur.as_secs()));
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("|"))
        }
    }

    fn hash_prefix(path: &Path, cancel: &AtomicBool) -> std::io::Result<Option<[u8; 32]>> {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }

        let mut file = File::open(path)?;
        let mut buffer = [0u8; PREFIX_HASH_LEN];
        let n = file.read(&mut buffer)?;

        let mut hasher = Sha256::new();
        hasher.update(&buffer[..n]);
        Ok(Some(hasher.finalize().into()))
    }

    fn hash_full_file(
        path: &Path,
        expected_size: u64,
        cancel: &AtomicBool,
    ) -> std::io::Result<Option<[u8; 32]>> {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }

        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; STREAM_BUFFER_SIZE];
        let mut total_read = 0u64;

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            total_read += n as u64;
        }

        if total_read != expected_size {
            // File was modified during hashing
            return Ok(None);
        }

        Ok(Some(hasher.finalize().into()))
    }

    fn build_cluster_from_group(
        id: String,
        group: &[&FileEntry],
        hash: Option<[u8; 32]>,
    ) -> DuplicateCluster {
        let first = group[0];
        let mut seen_fids: HashSet<FileId> = HashSet::new();

        let occurrences = group
            .iter()
            .map(|entry| {
                let meta = std::fs::symlink_metadata(&entry.path).ok();
                let (fid_opt, _) = meta
                    .as_ref()
                    .map(|m| get_file_id_and_links(&entry.path, m))
                    .unwrap_or((None, 1));

                let is_hardlink = if let Some(fid) = fid_opt {
                    !seen_fids.insert(fid)
                } else {
                    false
                };

                DuplicateOccurrence {
                    path: entry.path.clone(),
                    name: entry.name.clone(),
                    size: entry.size,
                    modified: entry.modified,
                    file_id: fid_opt,
                    is_hardlink,
                    is_system_protected: entry.is_system_protected,
                    is_selected: false,
                }
            })
            .collect();

        DuplicateCluster {
            cluster_id: id,
            primary_name: first.name.clone(),
            file_size: first.size,
            hash,
            is_group_selected: false,
            occurrences,
        }
    }

    /// Analyzes selected duplicates and identifies sibling duplicates residing in
    /// the same folder where originals are preserved outside that directory.
    pub fn find_folder_suggestion(
        clusters: &[DuplicateCluster],
        selected_paths: &HashSet<PathBuf>,
    ) -> Option<DuplicateSuggestion> {
        if selected_paths.is_empty() {
            return None;
        }

        // Identify parent directories of currently selected duplicate copies
        let mut selected_dirs: HashSet<PathBuf> = HashSet::new();
        for cluster in clusters {
            for occ in &cluster.occurrences {
                if selected_paths.contains(&occ.path) {
                    if let Some(parent) = occ.path.parent() {
                        selected_dirs.insert(parent.to_path_buf());
                    }
                }
            }
        }

        if selected_dirs.is_empty() {
            return None;
        }

        // Inspect candidate directories for unselected sibling duplicates
        for dir in selected_dirs {
            let mut candidates = Vec::new();
            let mut bytes = 0u64;

            for cluster in clusters {
                // Must have unselected copy in this folder
                let unselected_in_dir: Vec<&DuplicateOccurrence> = cluster
                    .occurrences
                    .iter()
                    .filter(|o| {
                        o.path.parent() == Some(&dir)
                            && !selected_paths.contains(&o.path)
                            && !o.is_system_protected
                    })
                    .collect();

                // And must have a preserved copy outside this folder that is NOT selected
                let kept_outside_dir = cluster
                    .occurrences
                    .iter()
                    .any(|o| o.path.parent() != Some(&dir) && !selected_paths.contains(&o.path));

                if !unselected_in_dir.is_empty() && kept_outside_dir {
                    for occ in unselected_in_dir {
                        candidates.push(occ.path.clone());
                        bytes += occ.size;
                    }
                }
            }

            if !candidates.is_empty() {
                let folder_name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| dir.to_string_lossy().to_string());

                return Some(DuplicateSuggestion {
                    folder_path: dir,
                    folder_name,
                    candidate_paths: candidates,
                    reclaimable_bytes: bytes,
                });
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            hash: None,
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

        let e1 = FileEntry::new("orig.bin".into(), file1.clone(), false, 8192, 8192, None, 1, false, false);
        let e2 = FileEntry::new("copy.bin".into(), file2.clone(), false, 8192, 8192, None, 1, false, false);
        let e3 = FileEntry::new("diff.bin".into(), file3.clone(), false, 8192, 8192, None, 1, false, false);

        let entries = vec![e1, e2, e3];
        let cancel = Arc::new(AtomicBool::new(false));

        let criteria = DuplicateCriteriaConfig {
            match_size: true,
            match_name: false,
            match_timestamp: false,
            verify_hash: true,
        };

        let clusters = DuplicateEngine::find_duplicates(&entries, criteria, &cancel);
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
}
