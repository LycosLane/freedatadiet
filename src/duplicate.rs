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
        self.filtered_deletable_count(&[])
    }

    /// Number of copies eligible for deletion among surviving filter occurrences.
    pub fn filtered_deletable_count(&self, terms: &[DuplicateFilterTerm]) -> usize {
        let Some(valid_indices) = self.filtered_occurrences(terms) else {
            return 0;
        };

        let all_valid_selected = !valid_indices.is_empty()
            && valid_indices.iter().all(|&idx| self.occurrences[idx].is_selected);
        if all_valid_selected && !self.is_group_selected {
            return 0;
        }

        valid_indices
            .iter()
            .map(|&idx| &self.occurrences[idx])
            .filter(|o| o.is_selected && !o.is_system_protected)
            .count()
    }

    /// Returns the paths in this cluster that are eligible for deletion.
    /// When all copies are selected but the file header checkbox (`is_group_selected`) is NOT checked,
    /// none of the copies in this cluster will be deleted.
    pub fn deletable_paths(&self) -> Vec<PathBuf> {
        self.filtered_deletable_paths(&[])
    }

    /// Returns the paths eligible for deletion that survive the active filter terms.
    /// Occurrences excluded by `--term` are never returned for deletion.
    pub fn filtered_deletable_paths(&self, terms: &[DuplicateFilterTerm]) -> Vec<PathBuf> {
        let Some(valid_indices) = self.filtered_occurrences(terms) else {
            return Vec::new();
        };

        // Safety gate check: if all valid copies are selected without group header confirmation, block deletion
        let all_valid_selected = !valid_indices.is_empty()
            && valid_indices.iter().all(|&idx| self.occurrences[idx].is_selected);
        if all_valid_selected && !self.is_group_selected {
            return Vec::new();
        }

        valid_indices
            .iter()
            .map(|&idx| &self.occurrences[idx])
            .filter(|o| o.is_selected && !o.is_system_protected)
            .map(|o| o.path.clone())
            .collect()
    }

    /// Total bytes eligible for deletion in this cluster (0 if all copies are selected without header confirmation).
    pub fn deletable_bytes(&self) -> u64 {
        self.filtered_deletable_bytes(&[])
    }

    /// Total bytes eligible for deletion surviving the filter terms.
    pub fn filtered_deletable_bytes(&self, terms: &[DuplicateFilterTerm]) -> u64 {
        (self.filtered_deletable_count(terms) as u64).saturating_mul(self.file_size)
    }

    /// Wasted bytes in this cluster accounting for active filter terms.
    pub fn filtered_wasted_bytes(&self, terms: &[DuplicateFilterTerm]) -> u64 {
        if let Some(valid) = self.filtered_occurrences(terms) {
            if valid.len() >= 2 {
                ((valid.len() - 1) as u64).saturating_mul(self.file_size)
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Selects every copy except one keeper chosen by `pref` (first, oldest or
    /// newest by modification time). System-protected copies are never selected.
    /// Does nothing for `Off`.
    pub fn keep_one(&mut self, pref: DuplicateRulePreference) {
        self.keep_one_filtered(pref, &[]);
    }

    /// Like `keep_one`, but applies only to occurrences surviving active filter terms.
    /// Occurrences excluded by `--term` remain unselected.
    pub fn keep_one_filtered(&mut self, pref: DuplicateRulePreference, terms: &[DuplicateFilterTerm]) {
        if pref == DuplicateRulePreference::Off {
            return;
        }
        let Some(valid_indices) = self.filtered_occurrences(terms) else {
            return;
        };
        if valid_indices.is_empty() {
            return;
        }

        let mut keep = valid_indices[0];
        for &idx in valid_indices.iter().skip(1) {
            let occ = &self.occurrences[idx];
            if let (Some(cur), Some(best)) = (occ.modified, self.occurrences[keep].modified) {
                let better = match pref {
                    DuplicateRulePreference::KeepOldest => cur < best,
                    DuplicateRulePreference::KeepNewest => cur > best,
                    _ => false,
                };
                if better {
                    keep = idx;
                }
            }
        }
        self.is_group_selected = false;
        for (i, occ) in self.occurrences.iter_mut().enumerate() {
            if valid_indices.contains(&i) {
                occ.is_selected = i != keep && !occ.is_system_protected;
            } else {
                occ.is_selected = false;
            }
        }
    }
    /// True when the group matches the duplicate filter criteria:
    /// - All Include terms must match either primary name or an unexcluded occurrence path.
    /// - None of the ExcludeGroup terms may match primary name or ANY occurrence path.
    /// - After dropping occurrences matching ExcludePath terms, at least 2 occurrences must remain.
    pub fn matches_filter(&self, terms: &[DuplicateFilterTerm]) -> bool {
        self.filtered_occurrences(terms).is_some()
    }

    /// Evaluates filter terms against this group.
    /// Returns Some(valid_occurrences_indices) if the group survives the filter (with >= 2 occurrences),
    /// or None if the group is excluded.
    pub fn filtered_occurrences(&self, terms: &[DuplicateFilterTerm]) -> Option<Vec<usize>> {
        if terms.is_empty() {
            return Some((0..self.occurrences.len()).collect());
        }

        let prim_lower = self.primary_name.to_lowercase();

        // 1. Check ExcludeGroup: if any ExcludeGroup term matches primary name or ANY occurrence path, reject group
        for term in terms {
            if let DuplicateFilterTerm::ExcludeGroup(ex) = term {
                if prim_lower.contains(ex.as_str()) {
                    return None;
                }
                if self.occurrences.iter().any(|o| o.path.to_string_lossy().to_lowercase().contains(ex.as_str())) {
                    return None;
                }
            }
        }

        // 2. Filter occurrences by ExcludePath
        let mut surviving_indices: Vec<usize> = Vec::new();
        for (idx, occ) in self.occurrences.iter().enumerate() {
            let path_lower = occ.path.to_string_lossy().to_lowercase();
            let mut excluded = false;
            for term in terms {
                if let DuplicateFilterTerm::ExcludePath(ex) = term
                    && path_lower.contains(ex.as_str())
                {
                    excluded = true;
                    break;
                }
            }
            if !excluded {
                surviving_indices.push(idx);
            }
        }

        // Must still have at least 2 duplicate copies remaining
        if surviving_indices.len() < 2 {
            return None;
        }

        // 3. Check Include terms: every Include term must be satisfied by primary name or surviving paths
        for term in terms {
            if let DuplicateFilterTerm::Include(inc) = term {
                let matches_primary = prim_lower.contains(inc.as_str());
                let matches_any_surviving = surviving_indices.iter().any(|&i| {
                    self.occurrences[i].path.to_string_lossy().to_lowercase().contains(inc.as_str())
                });
                if !matches_primary && !matches_any_surviving {
                    return None;
                }
            }
        }

        Some(surviving_indices)
    }

    /// Toggles the group-level checkbox.
    /// When checked, automatically selects all individual copies for purge.
    /// When unchecked, deselects all copies.
    pub fn set_group_selected(&mut self, selected: bool) {
        self.set_group_selected_filtered(selected, &[]);
    }

    /// Toggles the group-level checkbox across occurrences surviving filter terms.
    pub fn set_group_selected_filtered(&mut self, selected: bool, terms: &[DuplicateFilterTerm]) {
        self.is_group_selected = selected;
        let Some(valid_indices) = self.filtered_occurrences(terms) else {
            return;
        };
        for (i, occ) in self.occurrences.iter_mut().enumerate() {
            if valid_indices.contains(&i) {
                if !occ.is_system_protected {
                    occ.is_selected = selected;
                }
            } else if selected {
                occ.is_selected = false;
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


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuplicateFilterTerm {
    /// Positive term: must match either primary name or at least one occurrence path.
    Include(String),
    /// Single minus -term: if this matches primary name or ANY occurrence path, exclude entire group.
    ExcludeGroup(String),
    /// Double minus --term: filter out individual occurrence paths matching this term.
    ExcludePath(String),
}

/// Parses the duplicate filter text into structured filter terms.
/// Handles -term (exclude group), --term (exclude individual occurrence path),
/// and \- / -\- / --\- escaping.
pub fn filter_terms(text: &str) -> Vec<DuplicateFilterTerm> {
    let mut terms = Vec::new();
    for token in text.split_whitespace().take(32) {
        if token.is_empty() {
            continue;
        }

        if let Some(rest) = token.strip_prefix("--") {
            if rest.is_empty() {
                // Lone --: treat as literal -- search
                terms.push(DuplicateFilterTerm::Include("--".to_string()));
            } else if let Some(escaped) = rest.strip_prefix('\\') {
                // --\-term: search for literal -term in path exclusion
                if !escaped.is_empty() {
                    terms.push(DuplicateFilterTerm::ExcludePath(escaped.to_lowercase()));
                }
            } else if rest.starts_with('-') {
                // --- or ---something: shortcut for --\-...
                let query = rest.strip_prefix('-').unwrap_or("").to_lowercase();
                if query.is_empty() {
                    terms.push(DuplicateFilterTerm::ExcludePath("-".to_string()));
                } else {
                    terms.push(DuplicateFilterTerm::ExcludePath(format!("-{query}")));
                }
            } else {
                terms.push(DuplicateFilterTerm::ExcludePath(rest.to_lowercase()));
            }
        } else if let Some(rest) = token.strip_prefix('-') {
            if rest.is_empty() {
                // Lone -: treat as literal - search
                terms.push(DuplicateFilterTerm::Include("-".to_string()));
            } else if let Some(escaped) = rest.strip_prefix('\\') {
                // -\-something: exclude group that contains literal -something
                if !escaped.is_empty() {
                    terms.push(DuplicateFilterTerm::ExcludeGroup(escaped.to_lowercase()));
                }
            } else {
                terms.push(DuplicateFilterTerm::ExcludeGroup(rest.to_lowercase()));
            }
        } else if let Some(escaped) = token.strip_prefix('\\') {
            // Escaped character like \-foo
            if !escaped.is_empty() {
                terms.push(DuplicateFilterTerm::Include(escaped.to_lowercase()));
            }
        } else {
            terms.push(DuplicateFilterTerm::Include(token.to_lowercase()));
        }
    }
    terms
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
#[path = "duplicate_tests.rs"]
mod tests;