use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::duplicate::DuplicateCluster;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionAction {
    Select,
    Unselect,
}

#[derive(Debug, Clone)]
pub struct DuplicateSuggestion {
    pub folder_path: PathBuf,
    pub folder_name: String,
    pub candidate_paths: Vec<PathBuf>,
    pub reclaimable_bytes: u64,
    pub action: SuggestionAction,
    pub message: String,
}

/// Evaluates a manual selection or unselection of a duplicate occurrence.
/// Suggests bulk selecting or unselecting sibling duplicates residing in the same folder.
pub fn find_manual_folder_suggestion(
    clusters: &[DuplicateCluster],
    toggled_path: &Path,
    is_now_selected: bool,
) -> Option<DuplicateSuggestion> {
    let dir = toggled_path.parent()?;

    let mut candidates = Vec::new();
    let mut bytes = 0u64;

    for cluster in clusters {
        for occ in &cluster.occurrences {
            if occ.path != toggled_path && occ.path.parent() == Some(dir) && !occ.is_system_protected {
                if is_now_selected {
                    // Looking for other unselected copies in this folder with originals kept outside
                    if !occ.is_selected {
                        let kept_outside = cluster
                            .occurrences
                            .iter()
                            .any(|o| o.path.parent() != Some(dir) && !o.is_selected);
                        if kept_outside {
                            candidates.push(occ.path.clone());
                            bytes += occ.size;
                        }
                    }
                } else {
                    // Looking for other selected copies in this folder to unselect
                    if occ.is_selected {
                        candidates.push(occ.path.clone());
                        bytes += occ.size;
                    }
                }
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    let folder_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.to_string_lossy().to_string());

    let (action, message) = if is_now_selected {
        let msg = format!(
            "Pattern Match: Found {} other duplicate copies in \"{}\" with preserved copies outside ({} reclaimable).",
            candidates.len(),
            folder_name,
            crate::model::format_size(bytes)
        );
        (SuggestionAction::Select, msg)
    } else {
        let msg = format!(
            "Pattern Match: Found {} other selected duplicate copies in \"{}\".",
            candidates.len(),
            folder_name
        );
        (SuggestionAction::Unselect, msg)
    };

    Some(DuplicateSuggestion {
        folder_path: dir.to_path_buf(),
        folder_name,
        candidate_paths: candidates,
        reclaimable_bytes: bytes,
        action,
        message,
    })
}

/// Legacy fallback for batch analysis if needed.
pub fn find_folder_suggestion(
    _clusters: &[DuplicateCluster],
    _selected_paths: &HashSet<PathBuf>,
) -> Option<DuplicateSuggestion> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duplicate::{DuplicateCluster, DuplicateOccurrence};

    #[test]
    fn test_manual_folder_suggestion_select_and_unselect() {
        let dir = PathBuf::from("C:\\Sync\\Folder");
        let path1 = dir.join("file1.mp4");
        let path2 = dir.join("file2.mp4");
        let outside = PathBuf::from("C:\\Archive\\file2.mp4");

        let cluster1 = DuplicateCluster {
            cluster_id: "c1".to_string(),
            primary_name: "file1.mp4".to_string(),
            file_size: 100,
            is_group_selected: false,
            occurrences: vec![
                DuplicateOccurrence {
                    path: path1.clone(),
                    name: "file1.mp4".to_string(),
                    size: 100,
                    modified: None,
                    file_id: None,
                    is_hardlink: false,
                    is_system_protected: false,
                    is_selected: true,
                },
                DuplicateOccurrence {
                    path: PathBuf::from("C:\\Archive\\file1.mp4"),
                    name: "file1.mp4".to_string(),
                    size: 100,
                    modified: None,
                    file_id: None,
                    is_hardlink: false,
                    is_system_protected: false,
                    is_selected: false,
                },
            ],
        };

        let cluster2 = DuplicateCluster {
            cluster_id: "c2".to_string(),
            primary_name: "file2.mp4".to_string(),
            file_size: 200,
            is_group_selected: false,
            occurrences: vec![
                DuplicateOccurrence {
                    path: path2.clone(),
                    name: "file2.mp4".to_string(),
                    size: 200,
                    modified: None,
                    file_id: None,
                    is_hardlink: false,
                    is_system_protected: false,
                    is_selected: false,
                },
                DuplicateOccurrence {
                    path: outside.clone(),
                    name: "file2.mp4".to_string(),
                    size: 200,
                    modified: None,
                    file_id: None,
                    is_hardlink: false,
                    is_system_protected: false,
                    is_selected: false,
                },
            ],
        };

        let clusters = vec![cluster1, cluster2];

        // When path1 is manually selected, path2 in same folder is suggested for selection
        let sugg_sel = find_manual_folder_suggestion(&clusters, &path1, true);
        assert!(sugg_sel.is_some());
        let s = sugg_sel.unwrap();
        assert_eq!(s.action, SuggestionAction::Select);
        assert_eq!(s.candidate_paths, vec![path2.clone()]);
        assert_eq!(s.reclaimable_bytes, 200);

        // When path2 is selected, then manually unselected, path1 is suggested for unselection
        let mut clusters_sel = clusters.clone();
        clusters_sel[1].occurrences[0].is_selected = true;
        let sugg_unsel = find_manual_folder_suggestion(&clusters_sel, &path2, false);
        assert!(sugg_unsel.is_some());
        let u = sugg_unsel.unwrap();
        assert_eq!(u.action, SuggestionAction::Unselect);
        assert_eq!(u.candidate_paths, vec![path1.clone()]);
    }
}
