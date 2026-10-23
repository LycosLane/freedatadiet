use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::model::FileEntry;
use crate::platform::get_allocated_size;

#[derive(Debug, Clone)]
pub struct ScanSummary {
    pub total_size: u64,
    pub total_allocated: u64,
    pub total_files: usize,
    pub duration: Duration,
}

pub enum ScanMessage {
    Progress { scanned_items: usize },
    InitialEntries(Vec<FileEntry>),
    EntryUpdated(FileEntry),
    Completed(ScanSummary),
    Failed(String),
}

pub struct ScannerHandle {
    pub receiver: Receiver<ScanMessage>,
    pub cancel_flag: Arc<AtomicBool>,
}

impl ScannerHandle {
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }
}

pub fn start_scan(root_path: PathBuf, excludes: Vec<String>) -> ScannerHandle {
    let (tx, rx) = channel();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel_flag);

    let excludes: Vec<String> = excludes.into_iter().map(|s| s.to_lowercase()).collect();
    let excludes = Arc::new(excludes);

    thread::spawn(move || {
        let start_time = Instant::now();
        let total_counter = Arc::new(AtomicUsize::new(0));

        let read_res = fs::read_dir(&root_path);
        let entries_dir = match read_res {
            Ok(entries) => entries,
            Err(e) => {
                let _ = tx.send(ScanMessage::Failed(format!(
                    "Error opening folder: {}",
                    e
                )));
                return;
            }
        };

        // Collect direct children and send immediate initial snapshot
        let mut child_paths = Vec::new();
        let mut initial_entries = Vec::new();
        let mut total_size = 0u64;
        let mut total_allocated = 0u64;
        let mut total_files = 0usize;

        for entry_res in entries_dir {
            if cancel_clone.load(Ordering::Relaxed) {
                return;
            }
            if let Ok(entry) = entry_res {
                let name = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();
                let meta_opt = fs::symlink_metadata(&path).ok();
                let is_dir = meta_opt.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let modified = meta_opt.as_ref().and_then(|m| m.modified().ok());

                let (size, allocated, is_scanning) = if !is_dir {
                    let s = meta_opt.as_ref().map(|m| m.len()).unwrap_or(0);
                    let a = get_allocated_size(&path, s);
                    total_size += s;
                    total_allocated += a;
                    total_files += 1;
                    (s, a, false)
                } else {
                    (0, 0, true)
                };

                let mut item = FileEntry::new(
                    name.clone(),
                    path.clone(),
                    is_dir,
                    size,
                    allocated,
                    modified,
                    if is_dir { 0 } else { 1 },
                    false,
                );
                item.is_scanning = is_scanning;
                initial_entries.push(item);

                if is_dir {
                    child_paths.push((path, name));
                }
            }
        }

        // Send all discovered entries immediately so UI shows folders without delay!
        let _ = tx.send(ScanMessage::InitialEntries(initial_entries));

        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        let child_paths = Arc::new(child_paths);
        let (worker_tx, worker_rx) = channel();

        let chunk_size = (child_paths.len() + num_cpus - 1).max(1) / num_cpus;
        let mut handles = Vec::new();

        for chunk_idx in 0..num_cpus {
            let child_paths = Arc::clone(&child_paths);
            let cancel = Arc::clone(&cancel_clone);
            let total_counter = Arc::clone(&total_counter);
            let excludes = Arc::clone(&excludes);
            let worker_tx = worker_tx.clone();

            let start = chunk_idx * chunk_size;
            let end = (start + chunk_size).min(child_paths.len());

            if start >= child_paths.len() {
                break;
            }

            let handle = thread::spawn(move || {
                for i in start..end {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let (path, name) = &child_paths[i];
                    let entry = process_single_entry(path, name, &cancel, &total_counter, &excludes);
                    let _ = worker_tx.send(entry);
                }
            });
            handles.push(handle);
        }
        drop(worker_tx);

        let mut last_progress_report = Instant::now();
        for entry in worker_rx {
            total_size += entry.size;
            total_allocated += entry.allocated_size;
            total_files += if entry.is_dir { entry.item_count } else { 1 };

            let _ = tx.send(ScanMessage::EntryUpdated(entry));

            if last_progress_report.elapsed() > Duration::from_millis(50) {
                let count = total_counter.load(Ordering::Relaxed);
                let _ = tx.send(ScanMessage::Progress { scanned_items: count });
                last_progress_report = Instant::now();
            }
        }

        for h in handles {
            let _ = h.join();
        }

        if !cancel_clone.load(Ordering::Relaxed) {
            let _ = tx.send(ScanMessage::Completed(ScanSummary {
                total_size,
                total_allocated,
                total_files,
                duration: start_time.elapsed(),
            }));
        }
    });

    ScannerHandle {
        receiver: rx,
        cancel_flag,
    }
}

fn matches_exclude(name: &str, excludes: &[String]) -> bool {
    let lower = name.to_lowercase();
    for ex in excludes {
        let pattern = ex.trim();
        if pattern.is_empty() {
            continue;
        }
        if pattern.starts_with('*') {
            let suffix = &pattern[1..];
            if lower.ends_with(suffix) {
                return true;
            }
        } else if lower == pattern || lower.contains(pattern) {
            return true;
        }
    }
    false
}

fn process_single_entry(
    path: &Path,
    name: &str,
    cancel: &Arc<AtomicBool>,
    total_counter: &Arc<AtomicUsize>,
    excludes: &[String],
) -> FileEntry {
    total_counter.fetch_add(1, Ordering::Relaxed);

    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => {
            return FileEntry::new(
                name.to_string(),
                path.to_path_buf(),
                path.is_dir(),
                0,
                0,
                None,
                0,
                true,
            );
        }
    };

    let modified = metadata.modified().ok();
    let is_dir = metadata.is_dir();

    if !is_dir {
        let size = metadata.len();
        let allocated = get_allocated_size(path, size);
        FileEntry::new(
            name.to_string(),
            path.to_path_buf(),
            false,
            size,
            allocated,
            modified,
            1,
            false,
        )
    } else {
        // If this directory is a junction or symlink, skip recursing
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if (metadata.file_attributes() & 0x0000_0400) != 0 || metadata.file_type().is_symlink() {
                return FileEntry::new(
                    name.to_string(),
                    path.to_path_buf(),
                    true,
                    0,
                    0,
                    modified,
                    0,
                    false,
                );
            }
        }

        // If this directory itself is excluded, skip walking children
        if matches_exclude(name, excludes) {
            let size = metadata.len();
            let allocated = get_allocated_size(path, size);
            return FileEntry::new(
                format!("{} (ignored)", name),
                path.to_path_buf(),
                true,
                size,
                allocated,
                modified,
                0,
                false,
            );
        }

        let mut dir_size = 0u64;
        let mut dir_allocated = 0u64;
        let mut count = 0usize;
        let mut access_denied = false;

        let mut stack = vec![path.to_path_buf()];

        while let Some(current_dir) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            match fs::read_dir(&current_dir) {
                Ok(entries) => {
                    for entry_res in entries {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }

                        if let Ok(e) = entry_res {
                            total_counter.fetch_add(1, Ordering::Relaxed);
                            let sub_path = e.path();
                            let sub_name = e.file_name().to_string_lossy().to_string();

                            // Check excludes on subfolders
                            if matches_exclude(&sub_name, excludes) {
                                continue;
                            }

                            if let Ok(meta) = fs::symlink_metadata(&sub_path) {
                                count += 1;
                                if meta.is_dir() {
                                    #[cfg(windows)]
                                    {
                                        use std::os::windows::fs::MetadataExt;
                                        if (meta.file_attributes() & 0x0000_0400) != 0 || meta.file_type().is_symlink() {
                                            continue;
                                        }
                                    }
                                    #[cfg(not(windows))]
                                    {
                                        if meta.file_type().is_symlink() {
                                            continue;
                                        }
                                    }
                                    stack.push(sub_path);
                                } else {
                                    let fsize = meta.len();
                                    dir_size += fsize;
                                    dir_allocated += get_allocated_size(&sub_path, fsize);
                                }
                            }
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                    if current_dir == path {
                        access_denied = true;
                    }
                }
                Err(_) => {}
            }
        }

        FileEntry::new(
            name.to_string(),
            path.to_path_buf(),
            true,
            dir_size,
            dir_allocated,
            modified,
            count,
            access_denied,
        )
    }
}
