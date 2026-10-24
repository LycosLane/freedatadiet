use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::model::FileEntry;
use crate::platform::{get_allocated_size, get_file_id_and_links, FileId};

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
        let seen_hardlinks: Arc<Mutex<HashSet<FileId>>> = Arc::new(Mutex::new(HashSet::new()));
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
                let is_dir = meta_opt.as_ref().map(|m| is_dir_entry(&path, m)).unwrap_or(false);
                let modified = meta_opt.as_ref().and_then(|m| m.modified().ok());

                let (size, allocated, is_scanning) = if !is_dir {
                    let s = meta_opt.as_ref().map(|m| m.len()).unwrap_or(0);
                    let a = get_allocated_size(&path, s);
                    let is_dup = if let Some(m) = &meta_opt {
                        let (fid_opt, links) = get_file_id_and_links(&path, m);
                        if links > 1 {
                            if let Some(fid) = fid_opt {
                                !seen_hardlinks.lock().unwrap().insert(fid)
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if !is_dup {
                        total_size += s;
                        total_allocated += a;
                    }
                    total_files += 1;
                    (if is_dup { 0 } else { s }, if is_dup { 0 } else { a }, false)
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

        let chunk_size = child_paths.len().div_ceil(num_cpus).max(1);
        let mut handles = Vec::new();

        for chunk_idx in 0..num_cpus {
            let child_paths = Arc::clone(&child_paths);
            let cancel = Arc::clone(&cancel_clone);
            let total_counter = Arc::clone(&total_counter);
            let excludes = Arc::clone(&excludes);
            let seen_hardlinks = Arc::clone(&seen_hardlinks);
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
                    let entry = process_single_entry(path, name, &cancel, &total_counter, &excludes, &seen_hardlinks);
                    let _ = worker_tx.send(entry);
                }
            });
            handles.push(handle);
        }
        drop(worker_tx);

        let mut last_progress_report = Instant::now();
        for entry in worker_rx {
            if cancel_clone.load(Ordering::Relaxed) {
                break;
            }
            total_size += entry.size;
            total_allocated += entry.allocated_size;
            total_files += if entry.is_dir { entry.item_count } else { 1 };

            // The UI dropped its receiver (navigated away): stop and let the workers wind down
            if tx.send(ScanMessage::EntryUpdated(entry)).is_err() {
                cancel_clone.store(true, Ordering::Relaxed);
                break;
            }

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
        if let Some(suffix) = pattern.strip_prefix('*') {
            if lower.ends_with(suffix) {
                return true;
            }
        } else if lower == pattern {
            return true;
        }
    }
    false
}

/// `symlink_metadata().is_dir()` is false for links, so follow the link only to learn whether it points at a folder.
fn is_dir_entry(path: &Path, meta: &fs::Metadata) -> bool {
    meta.is_dir() || (meta.file_type().is_symlink() && fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false))
}

fn process_single_entry(
    path: &Path,
    name: &str,
    cancel: &Arc<AtomicBool>,
    total_counter: &Arc<AtomicUsize>,
    excludes: &[String],
    seen_hardlinks: &Arc<Mutex<HashSet<FileId>>>,
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
                false,
            );
        }
    };

    let modified = metadata.modified().ok();
    let is_dir = is_dir_entry(path, &metadata);

    if !is_dir {
        let size = metadata.len();
        let (fid_opt, links) = get_file_id_and_links(path, &metadata);
        let is_dup = if links > 1 {
            if let Some(fid) = fid_opt {
                !seen_hardlinks.lock().unwrap().insert(fid)
            } else {
                false
            }
        } else {
            false
        };
        let counted_size = if is_dup { 0 } else { size };
        let allocated = if is_dup { 0 } else { get_allocated_size(path, size) };
        FileEntry::new(
            name.to_string(),
            path.to_path_buf(),
            false,
            counted_size,
            allocated,
            modified,
            1,
            false,
            false,
        )
    } else {
        // Junctions and symlinks are listed but never followed, so loops and double counting cannot happen
        if metadata.file_type().is_symlink() {
            return FileEntry::new(
                name.to_string(),
                path.to_path_buf(),
                true,
                0,
                0,
                modified,
                0,
                false,
                false,
            );
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
                false,
            );
        }

        let mut dir_size = 0u64;
        let mut dir_allocated = 0u64;
        let mut count = 0usize;
        let mut access_denied = false;
        let mut partially_locked = false;

        let mut visited_dirs = HashSet::new();
        if let (Some(root_fid), _) = get_file_id_and_links(path, &metadata) {
            visited_dirs.insert(root_fid);
        }

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
                                    // Never follow links (symlinks, junctions) while walking
                                    if meta.file_type().is_symlink() {
                                        continue;
                                    }
                                    // Track directory inode to prevent infinite loops (Unix bind mounts or recursive mounts)
                                    let (dir_fid, _) = get_file_id_and_links(&sub_path, &meta);
                                    if let Some(dfid) = dir_fid {
                                        if !visited_dirs.insert(dfid) {
                                            continue;
                                        }
                                    }
                                    stack.push(sub_path);
                                } else {
                                    let fsize = meta.len();
                                    let (file_fid, links) = get_file_id_and_links(&sub_path, &meta);
                                    let is_dup = if links > 1 {
                                        if let Some(ffid) = file_fid {
                                            !seen_hardlinks.lock().unwrap().insert(ffid)
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    };

                                    if !is_dup {
                                        dir_size += fsize;
                                        dir_allocated += get_allocated_size(&sub_path, fsize);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                    if current_dir == path {
                        access_denied = true;
                    } else {
                        partially_locked = true;
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
            partially_locked,
        )
    }
}
