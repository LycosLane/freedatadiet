use std::collections::HashSet;
use std::path::PathBuf;

use crate::duplicate::{DuplicateCluster, DuplicateOccurrence};

#[derive(Debug, Clone)]
pub struct DuplicateSuggestion {
    pub folder_path: PathBuf,
    pub folder_name: String,
    pub candidate_paths: Vec<PathBuf>,
    pub reclaimable_bytes: u64,
    pub message: String,
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

            let mut keeper_names = HashSet::new();
            for cluster in clusters {
                let has_cand = cluster.occurrences.iter().any(|o| candidates.contains(&o.path));
                if has_cand {
                    for occ in &cluster.occurrences {
                        if occ.path.parent() != Some(&dir) && !selected_paths.contains(&occ.path) {
                            if let Some(p) = occ.path.parent() {
                                let kn = p
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| p.to_string_lossy().to_string());
                                keeper_names.insert(kn);
                            }
                        }
                    }
                }
            }

            let message = if keeper_names.len() == 1 {
                let keeper_name = keeper_names.into_iter().next().unwrap();
                format!(
                    "Pattern Match: Found {} sibling copies in \"{}\" with preserved copies in \"{}\" ({} reclaimable).",
                    candidates.len(),
                    folder_name,
                    keeper_name,
                    crate::model::format_size(bytes)
                )
            } else {
                format!(
                    "Duplicate Assistant: Found {} sibling duplicate copies in folder \"{}\" ({} reclaimable).",
                    candidates.len(),
                    folder_name,
                    crate::model::format_size(bytes)
                )
            };

            return Some(DuplicateSuggestion {
                folder_path: dir,
                folder_name,
                candidate_paths: candidates,
                reclaimable_bytes: bytes,
                message,
            });
        }
    }

    None
}
