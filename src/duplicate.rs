use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use sha2::{Digest, Sha256};
use rayon::prelude::*;

use crate::config::DuplicateCriteriaConfig;
use crate::model::FileEntry;
use crate::platform::{get_file_id_and_links, FileId};

pub const PREFIX_HASH_LEN: usize = 64 * 1024;
pub const SUFFIX_HASH_LEN: usize = 64 * 1024;
pub const STREAM_BUFFER_SIZE: usize = 1024 * 1024;

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
        if self.are_all_copies_selected() && !self.is_group_selected {
            if let Some(first) = self.occurrences.first() {
                return Err(first.path.clone());
            }
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

#[derive(Debug, Clone)]
pub enum DuplicateMessage {
    Progress { stage: String, done: usize, total: usize },
    Completed(Vec<DuplicateCluster>),
}

pub use crate::duplicate_suggestion::{find_folder_suggestion, DuplicateSuggestion};

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
        for (_, sub) in map {
            if sub.len() >= 2 {
                result.push(sub);
            }
        }
    }
    result
}

pub struct DuplicateEngine;

impl DuplicateEngine {
    /// Detects duplicate files from a list of scanned entries according to criteria.
    /// Uses 3-stage verification (64KB Prefix Hash -> 64KB Suffix Hash -> 1MB Streamed Full SHA-256)
    /// with optional progress reporting.
    pub fn find_duplicates(
        entries: &[FileEntry],
        criteria: DuplicateCriteriaConfig,
        cancel_flag: &Arc<AtomicBool>,
        progress_tx: Option<&std::sync::mpsc::Sender<DuplicateMessage>>,
    ) -> Vec<DuplicateCluster> {
        let files: Vec<&FileEntry> = entries
            .iter()
            .filter(|e| !e.is_dir && e.size > 0)
            .collect();

        if files.len() < 2 {
            if let Some(tx) = progress_tx {
                let _ = tx.send(DuplicateMessage::Completed(Vec::new()));
            }
            return Vec::new();
        }

        if let Some(tx) = progress_tx {
            let _ = tx.send(DuplicateMessage::Progress {
                stage: "Partitioning candidate files by size...".into(),
                done: 0,
                total: files.len(),
            });
        }

        // Stage 1: Partition by file size (u64) if match_size is enabled
        let size_groups: Vec<Vec<&FileEntry>> = if criteria.match_size {
            let mut size_map: HashMap<u64, Vec<&FileEntry>> = HashMap::new();
            for file in files {
                if cancel_flag.load(Ordering::Relaxed) {
                    return Vec::new();
                }
                size_map.entry(file.size).or_default().push(file);
            }
            size_map.into_values().filter(|g| g.len() >= 2).collect()
        } else {
            vec![files]
        };

        if size_groups.is_empty() {
            if let Some(tx) = progress_tx {
                let _ = tx.send(DuplicateMessage::Completed(Vec::new()));
            }
            return Vec::new();
        }

        // Stage 2: Sub-partition by case-insensitive name if match_name is enabled
        let name_groups: Vec<Vec<&FileEntry>> = if criteria.match_name {
            if cancel_flag.load(Ordering::Relaxed) {
                return Vec::new();
            }
            subpartition_by(size_groups, |file| file.name.to_lowercase())
        } else {
            size_groups
        };

        if name_groups.is_empty() {
            if let Some(tx) = progress_tx {
                let _ = tx.send(DuplicateMessage::Completed(Vec::new()));
            }
            return Vec::new();
        }

        // Stage 3: Sub-partition by modification timestamp if match_timestamp is enabled
        let metadata_groups: Vec<Vec<&FileEntry>> = if criteria.match_timestamp {
            if cancel_flag.load(Ordering::Relaxed) {
                return Vec::new();
            }
            subpartition_by(name_groups, |file| file.modified)
        } else {
            name_groups
        };

        if metadata_groups.is_empty() {
            if let Some(tx) = progress_tx {
                let _ = tx.send(DuplicateMessage::Completed(Vec::new()));
            }
            return Vec::new();
        }

        let mut clusters = Vec::new();
        let mut cluster_idx = 1;

        if !criteria.verify_hash {
            for group in metadata_groups {
                if cancel_flag.load(Ordering::Relaxed) {
                    return Vec::new();
                }
                if group.len() >= 2 {
                    let cluster = Self::build_cluster_from_group(
                        format!("cluster_{cluster_idx}"),
                        &group,
                        None,
                    );
                    clusters.push(cluster);
                    cluster_idx += 1;
                }
            }
            clusters.sort_by(|a, b| b.wasted_bytes().cmp(&a.wasted_bytes()));
            if let Some(tx) = progress_tx {
                let _ = tx.send(DuplicateMessage::Completed(clusters.clone()));
            }
            return clusters;
        }

        // --- VERIFY HASH (PARALLEL MULTI-STAGE WITH RAYON & PROGRESS) ---
        let mut indexed_candidates: Vec<(usize, &FileEntry)> = Vec::new();
        for (g_idx, group) in metadata_groups.iter().enumerate() {
            for file in group {
                indexed_candidates.push((g_idx, *file));
            }
        }

        let total_prefix_files = indexed_candidates.len();
        if let Some(tx) = progress_tx {
            let _ = tx.send(DuplicateMessage::Progress {
                stage: "Hashing candidate headers (64 KB)...".into(),
                done: 0,
                total: total_prefix_files,
            });
        }

        // Pass 1: 64 KB Prefix Hash
        let prefix_counter = std::sync::atomic::AtomicUsize::new(0);
        let prefix_results: Vec<Option<(usize, [u8; 32], &FileEntry)>> = indexed_candidates
            .par_iter()
            .map(|(g_idx, file)| {
                if cancel_flag.load(Ordering::Relaxed) {
                    return None;
                }
                let res = match Self::hash_prefix(&file.path, cancel_flag) {
                    Ok(Some(hash)) => Some((*g_idx, hash, *file)),
                    _ => None,
                };
                let done = prefix_counter.fetch_add(1, Ordering::Relaxed) + 1;
                if done % 200 == 0 || done == total_prefix_files {
                    if let Some(tx) = progress_tx {
                        let _ = tx.send(DuplicateMessage::Progress {
                            stage: "Hashing candidate headers (64 KB)...".into(),
                            done,
                            total: total_prefix_files,
                        });
                    }
                }
                res
            })
            .collect();

        if cancel_flag.load(Ordering::Relaxed) {
            return Vec::new();
        }

        // Group survivors by (g_idx, prefix_hash)
        let mut prefix_groups: HashMap<(usize, [u8; 32]), Vec<&FileEntry>> = HashMap::new();
        for item in prefix_results.into_iter().flatten() {
            prefix_groups.entry((item.0, item.1)).or_default().push(item.2);
        }

        let mut survivor_groups: Vec<((usize, [u8; 32]), Vec<&FileEntry>)> = Vec::new();
        for (key, candidates) in prefix_groups {
            if candidates.len() >= 2 {
                survivor_groups.push((key, candidates));
            }
        }

        // Separate small files (<= 64KB, completely verified by prefix hash)
        let mut suffix_hash_items: Vec<(usize, [u8; 32], &FileEntry)> = Vec::new();
        for ((g_idx, prefix_hash), candidates) in survivor_groups {
            if candidates[0].size <= PREFIX_HASH_LEN as u64 {
                let cluster = Self::build_cluster_from_group(
                    format!("cluster_h{cluster_idx}"),
                    &candidates,
                    Some(prefix_hash),
                );
                clusters.push(cluster);
                cluster_idx += 1;
            } else {
                for file in candidates {
                    suffix_hash_items.push((g_idx, prefix_hash, file));
                }
            }
        }

        // Pass 2: 64 KB Suffix Hash for files > 64KB
        let total_suffix_files = suffix_hash_items.len();
        let mut full_hash_items: Vec<(usize, [u8; 32], &FileEntry)> = Vec::new();

        if total_suffix_files > 0 {
            if let Some(tx) = progress_tx {
                let _ = tx.send(DuplicateMessage::Progress {
                    stage: "Hashing candidate suffixes (64 KB)...".into(),
                    done: 0,
                    total: total_suffix_files,
                });
            }

            let suffix_counter = std::sync::atomic::AtomicUsize::new(0);
            let suffix_results: Vec<Option<(usize, [u8; 32], [u8; 32], &FileEntry)>> = suffix_hash_items
                .par_iter()
                .map(|(g_idx, prefix_hash, file)| {
                    if cancel_flag.load(Ordering::Relaxed) {
                        return None;
                    }
                    let res = match Self::hash_suffix(&file.path, file.size, cancel_flag) {
                        Ok(Some(shash)) => Some((*g_idx, *prefix_hash, shash, *file)),
                        _ => None,
                    };
                    let done = suffix_counter.fetch_add(1, Ordering::Relaxed) + 1;
                    if done % 100 == 0 || done == total_suffix_files {
                        if let Some(tx) = progress_tx {
                            let _ = tx.send(DuplicateMessage::Progress {
                                stage: "Hashing candidate suffixes (64 KB)...".into(),
                                done,
                                total: total_suffix_files,
                            });
                        }
                    }
                    res
                })
                .collect();

            if cancel_flag.load(Ordering::Relaxed) {
                return Vec::new();
            }

            let mut suffix_groups: HashMap<(usize, [u8; 32], [u8; 32]), Vec<&FileEntry>> = HashMap::new();
            for item in suffix_results.into_iter().flatten() {
                suffix_groups.entry((item.0, item.1, item.2)).or_default().push(item.3);
            }

            for ((g_idx, prefix_hash, _), candidates) in suffix_groups {
                if candidates.len() >= 2 {
                    if candidates[0].size <= (PREFIX_HASH_LEN + SUFFIX_HASH_LEN) as u64 {
                        // Entire file covered by prefix + suffix
                        let cluster = Self::build_cluster_from_group(
                            format!("cluster_h{cluster_idx}"),
                            &candidates,
                            Some(prefix_hash),
                        );
                        clusters.push(cluster);
                        cluster_idx += 1;
                    } else {
                        for file in candidates {
                            full_hash_items.push((g_idx, prefix_hash, file));
                        }
                    }
                }
            }
        }

        // Pass 3: Full Streaming SHA-256 (1MB buffer) for files > 128KB
        let total_full_files = full_hash_items.len();
        if total_full_files > 0 {
            if let Some(tx) = progress_tx {
                let _ = tx.send(DuplicateMessage::Progress {
                    stage: "Verifying exact SHA-256 hashes...".into(),
                    done: 0,
                    total: total_full_files,
                });
            }

            let full_counter = std::sync::atomic::AtomicUsize::new(0);
            let full_results: Vec<Option<(usize, [u8; 32], &FileEntry)>> = full_hash_items
                .par_iter()
                .map(|(g_idx, _, file)| {
                    if cancel_flag.load(Ordering::Relaxed) {
                        return None;
                    }
                    let res = match Self::hash_full_file(&file.path, file.size, cancel_flag) {
                        Ok(Some(hash)) => Some((*g_idx, hash, *file)),
                        _ => None,
                    };
                    let done = full_counter.fetch_add(1, Ordering::Relaxed) + 1;
                    if done % 10 == 0 || done == total_full_files {
                        if let Some(tx) = progress_tx {
                            let _ = tx.send(DuplicateMessage::Progress {
                                stage: "Verifying exact SHA-256 hashes...".into(),
                                done,
                                total: total_full_files,
                            });
                        }
                    }
                    res
                })
                .collect();

            if cancel_flag.load(Ordering::Relaxed) {
                return Vec::new();
            }

            let mut full_groups: HashMap<(usize, [u8; 32]), Vec<&FileEntry>> = HashMap::new();
            for item in full_results.into_iter().flatten() {
                full_groups.entry((item.0, item.1)).or_default().push(item.2);
            }

            for ((_, full_hash), files) in full_groups {
                if files.len() >= 2 {
                    let cluster = Self::build_cluster_from_group(
                        format!("cluster_h{cluster_idx}"),
                        &files,
                        Some(full_hash),
                    );
                    clusters.push(cluster);
                    cluster_idx += 1;
                }
            }
        }

        clusters.sort_by(|a, b| b.wasted_bytes().cmp(&a.wasted_bytes()));
        if let Some(tx) = progress_tx {
            let _ = tx.send(DuplicateMessage::Completed(clusters.clone()));
        }
        clusters
    }

    fn hash_suffix(
        path: &Path,
        file_size: u64,
        cancel: &AtomicBool,
    ) -> std::io::Result<Option<[u8; 32]>> {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }

        let mut file = File::open(path)?;
        let offset = file_size.saturating_sub(SUFFIX_HASH_LEN as u64);
        file.seek(SeekFrom::Start(offset))?;
        let mut buffer = vec![0u8; SUFFIX_HASH_LEN];
        let n = file.read(&mut buffer)?;

        let mut hasher = Sha256::new();
        hasher.update(&buffer[..n]);
        Ok(Some(hasher.finalize().into()))
    }

    fn hash_prefix(path: &Path, cancel: &AtomicBool) -> std::io::Result<Option<[u8; 32]>> {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }

        let mut file = File::open(path)?;
        let mut buffer = vec![0u8; PREFIX_HASH_LEN];
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
        let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];
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
        crate::duplicate_suggestion::find_folder_suggestion(clusters, selected_paths)
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

        let clusters = DuplicateEngine::find_duplicates(&entries, criteria, &cancel, None);
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
            hash: None,
            is_group_selected: false,
            occurrences: vec![occ1_a, occ1_b],
        };
        let cluster2 = DuplicateCluster {
            cluster_id: "c2".into(),
            primary_name: "photo2.jpg".into(),
            file_size: 600,
            hash: None,
            is_group_selected: false,
            occurrences: vec![occ2_a, occ2_b],
        };

        let clusters = vec![cluster1, cluster2];
        let mut selected = HashSet::new();
        selected.insert(PathBuf::from("C:\\Sync\\photo1.jpg"));

        let sugg = DuplicateEngine::find_folder_suggestion(&clusters, &selected).expect("Must find folder pattern suggestion");
        assert_eq!(sugg.folder_path, PathBuf::from("C:\\Sync"));
        assert_eq!(sugg.candidate_paths, vec![PathBuf::from("C:\\Sync\\photo2.jpg")]);
        assert_eq!(sugg.reclaimable_bytes, 600);
        assert!(sugg.message.contains("Pattern Match"));
        assert!(sugg.message.contains("Sync"));
        assert!(sugg.message.contains("Keep"));
    }
}
