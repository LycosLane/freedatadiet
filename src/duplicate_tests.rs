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
fn test_filtered_deletable_paths_respects_path_exclusions() {
    let occ = |p: &str, sel: bool| DuplicateOccurrence {
        path: PathBuf::from(p),
        name: "data.bin".into(),
        size: 1000,
        modified: None,
        file_id: None,
        is_hardlink: false,
        is_system_protected: false,
        is_selected: sel,
    };

    let mut cluster = DuplicateCluster {
        cluster_id: "c_filt".into(),
        primary_name: "data.bin".into(),
        file_size: 1000,
        is_group_selected: false,
        occurrences: vec![
            occ("C:\\Work\\data.bin", false),
            occ("C:\\Backup\\data.bin", true),
            occ("D:\\Archive\\data.bin", true),
        ],
    };

    // Without filters: Backup and Archive are marked for deletion
    let terms_none = filter_terms("");
    assert_eq!(cluster.filtered_deletable_paths(&terms_none).len(), 2);
    assert_eq!(cluster.filtered_deletable_bytes(&terms_none), 2000);

    // With --backup path filter: Backup copy is hidden from UI and MUST NOT be deleted!
    let terms_no_backup = filter_terms("--backup");
    let del_paths = cluster.filtered_deletable_paths(&terms_no_backup);
    assert_eq!(del_paths, vec![PathBuf::from("D:\\Archive\\data.bin")]);
    assert_eq!(cluster.filtered_deletable_bytes(&terms_no_backup), 1000);

    // Keep first with --backup filter active:
    // Surviving copies are C:\Work (kept) and D:\Archive (selected). Backup remains unselected!
    cluster.occurrences[1].is_selected = true;
    cluster.occurrences[2].is_selected = false;
    cluster.keep_one_filtered(crate::config::DuplicateRulePreference::KeepFirst, &terms_no_backup);
    assert!(!cluster.occurrences[0].is_selected); // Work is keeper
    assert!(!cluster.occurrences[1].is_selected); // Backup was excluded by filter -> untouched!
    assert!(cluster.occurrences[2].is_selected);  // Archive is marked
}

#[test]
fn test_incomplete_escape_does_not_blank_out_results() {
    // Lone escape prefix -\ or --\ should not produce empty string queries
    let terms1 = filter_terms(r"-\");
    assert!(terms1.is_empty() || terms1.iter().all(|t| !matches!(t, DuplicateFilterTerm::ExcludeGroup(s) if s.is_empty())));

    let terms2 = filter_terms(r"--\");
    assert!(terms2.is_empty() || terms2.iter().all(|t| !matches!(t, DuplicateFilterTerm::ExcludePath(s) if s.is_empty())));
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
fn test_filter_matches_any_full_path_of_the_group() {
    let occ = |p: &str| DuplicateOccurrence {
        path: PathBuf::from(p),
        name: "a.jpg".into(),
        size: 10,
        modified: None,
        file_id: None,
        is_hardlink: false,
        is_system_protected: false,
        is_selected: false,
    };
    let cluster = DuplicateCluster {
        cluster_id: "c".into(),
        primary_name: "a.jpg".into(),
        file_size: 10,
        is_group_selected: false,
        occurrences: vec![
            occ("C:\\Photos\\Holiday\\a.jpg"),
            occ("C:\\Backup\\a.jpg"),
            occ("C:\\Work\\Project\\a.jpg"),
        ],
    };

    // Basic inclusion
    assert!(cluster.matches_filter(&filter_terms("")));
    assert!(cluster.matches_filter(&filter_terms(".JPG")));
    assert!(cluster.matches_filter(&filter_terms("holiday backup")));
    assert!(!cluster.matches_filter(&filter_terms("holiday .png")));

    // Single minus (-): Group exclusion (Variante A)
    assert!(!cluster.matches_filter(&filter_terms("-backup")));
    assert!(cluster.matches_filter(&filter_terms("-synology")));

    // Double minus (--): Path exclusion (Variante B)
    // If we exclude backup, 2 occurrences remain (Photos, Work) -> matches!
    assert!(cluster.matches_filter(&filter_terms("--backup")));
    let filtered = cluster.filtered_occurrences(&filter_terms("--backup")).unwrap();
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered, vec![0, 2]);

    // If we exclude both backup and work, only 1 occurrence remains -> group hidden!
    assert!(!cluster.matches_filter(&filter_terms("--backup --work")));

    // Escaped literal minus (\-)
    let dash_cluster = DuplicateCluster {
        cluster_id: "c2".into(),
        primary_name: "doc-v1.txt".into(),
        file_size: 10,
        is_group_selected: false,
        occurrences: vec![occ("C:\\A\\doc-v1.txt"), occ("C:\\B\\doc-v1.txt")],
    };
    assert!(dash_cluster.matches_filter(&filter_terms(r"\-")));
    assert!(dash_cluster.matches_filter(&filter_terms(r"\-v1")));
    assert!(!dash_cluster.matches_filter(&filter_terms(r"\-v2")));

    // Exclude group with literal dash (-\-)
    assert!(!dash_cluster.matches_filter(&filter_terms(r"-\-v1")));
    assert!(dash_cluster.matches_filter(&filter_terms(r"-\-v2")));

    // Exclude path with shortcut (--- or --\-)
    let three_dash_cluster = DuplicateCluster {
        cluster_id: "c3".into(),
        primary_name: "item.txt".into(),
        file_size: 10,
        is_group_selected: false,
        occurrences: vec![
            occ("C:\\A\\item-old.txt"),
            occ("C:\\B\\item.txt"),
            occ("C:\\C\\item.txt"),
        ],
    };
    // '---' excludes paths with '-' (i.e. C:\A\item-old.txt). 2 remain -> visible
    assert!(three_dash_cluster.matches_filter(&filter_terms("---")));
    let rem = three_dash_cluster.filtered_occurrences(&filter_terms("---")).unwrap();
    assert_eq!(rem, vec![1, 2]);
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
