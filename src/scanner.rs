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

pub fn start_scan(root_paths: Vec<PathBuf>, excludes: Vec<String>, flatten: bool) -> ScannerHandle {
    let (tx, rx) = channel();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel_flag);

    let excludes: Vec<String> = excludes.into_iter().map(|s| s.to_lowercase()).collect();
    let excludes = Arc::new(excludes);

    thread::spawn(move || {
        let start_time = Instant::now();
        let total_counter = Arc::new(AtomicUsize::new(0));
        let seen_hardlinks: Arc<Mutex<HashSet<FileId>>> = Arc::new(Mutex::new(HashSet::new()));

        if flatten || root_paths.len() > 1 {
            scan_flattened(
                root_paths,
                excludes,
                cancel_clone,
                total_counter,
                seen_hardlinks,
                tx,
                start_time,
            );
            return;
        }

        let root_path = match root_paths.first() {
            Some(p) => p.clone(),
            None => {
                let _ = tx.send(ScanMessage::Failed("No scan location specified.".to_string()));
                return;
            }
        };

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
                                !seen_hardlinks.lock().unwrap_or_else(|p| p.into_inner()).insert(fid)
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
        let pattern = ex.trim().to_lowercase();
        if pattern.is_empty() {
            continue;
        }

        // Wildcard support: *pattern*, *suffix, prefix*, or exact match
        if pattern.starts_with('*') && pattern.ends_with('*') && pattern.len() > 2 {
            let substr = &pattern[1..pattern.len() - 1];
            if lower.contains(substr) {
                return true;
            }
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            if lower.ends_with(suffix) {
                return true;
            }
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            if lower.starts_with(prefix) {
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
                !seen_hardlinks.lock().unwrap_or_else(|p| p.into_inner()).insert(fid)
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
                                            !seen_hardlinks.lock().unwrap_or_else(|p| p.into_inner()).insert(ffid)
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

struct DirFrame {
    path: PathBuf,
    name: String,
    modified: Option<std::time::SystemTime>,
    size: u64,
    allocated: u64,
    items: usize,
    subdirs: Vec<PathBuf>,
    is_root: bool,
}

fn scan_flattened(
    roots: Vec<PathBuf>,
    excludes: Arc<Vec<String>>,
    cancel: Arc<AtomicBool>,
    total_counter: Arc<AtomicUsize>,
    seen_hardlinks: Arc<Mutex<HashSet<FileId>>>,
    tx: std::sync::mpsc::Sender<ScanMessage>,
    start_time: Instant,
) {
    let mut total_size = 0u64;
    let mut total_allocated = 0u64;
    let mut total_files = 0usize;
    let mut batch = Vec::with_capacity(128);
    let mut last_progress = Instant::now();

    for root_path in roots {
        if cancel.load(Ordering::Relaxed) {
            return;
        }

        let root_meta = match fs::symlink_metadata(&root_path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if !is_dir_entry(&root_path, &root_meta) {
            let size = root_meta.len();
            let alloc = get_allocated_size(&root_path, size);
            total_size += size;
            total_allocated += alloc;
            total_files += 1;
            let name = root_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| root_path.to_string_lossy().to_string());
            let file_entry = FileEntry::new(
                name,
                root_path,
                false,
                size,
                alloc,
                root_meta.modified().ok(),
                1,
                false,
                false,
            );
            batch.push(file_entry);
            continue;
        }

        let inspect_dir = |dir_path: &Path, is_root: bool| -> Option<(DirFrame, Vec<FileEntry>)> {
            let rd = match fs::read_dir(dir_path) {
                Ok(r) => r,
                Err(_) => return None,
            };

            let mut subdirs = Vec::new();
            let mut file_entries = Vec::new();
            let mut local_size = 0u64;
            let mut local_alloc = 0u64;
            let mut local_items = 0usize;

            for entry_res in rd {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                if let Ok(entry) = entry_res {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if matches_exclude(&name, &excludes) {
                        continue;
                    }
                    let path = entry.path();
                    let meta_opt = fs::symlink_metadata(&path).ok();
                    if let Some(meta) = meta_opt {
                        let is_symlink = meta.file_type().is_symlink();
                        let is_dir = is_dir_entry(&path, &meta);

                        if is_dir {
                            if !is_symlink {
                                subdirs.push(path);
                            }
                        } else {
                            let size = meta.len();
                            let alloc = get_allocated_size(&path, size);
                            let (fid_opt, links) = get_file_id_and_links(&path, &meta);
                            let is_dup = if links > 1 {
                                if let Some(fid) = fid_opt {
                                    !seen_hardlinks
                                        .lock()
                                        .unwrap_or_else(|p| p.into_inner())
                                        .insert(fid)
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            let file_entry = FileEntry::new(
                                name,
                                path,
                                false,
                                if is_dup { 0 } else { size },
                                if is_dup { 0 } else { alloc },
                                meta.modified().ok(),
                                1,
                                false,
                                false,
                            );
                            file_entries.push(file_entry);

                            if !is_dup {
                                local_size += size;
                                local_alloc += alloc;
                            }
                            local_items += 1;
                        }
                    }
                }
            }

            let name = dir_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| dir_path.to_string_lossy().to_string());
            let modified = fs::symlink_metadata(dir_path).ok().and_then(|m| m.modified().ok());

            Some((
                DirFrame {
                    path: dir_path.to_path_buf(),
                    name,
                    modified,
                    size: local_size,
                    allocated: local_alloc,
                    items: local_items,
                    subdirs,
                    is_root,
                },
                file_entries,
            ))
        };

        let mut stack: Vec<DirFrame> = Vec::new();

        if let Some((root_frame, files)) = inspect_dir(&root_path, true) {
            for f in files {
                total_counter.fetch_add(1, Ordering::Relaxed);
                batch.push(f);
            }
            stack.push(root_frame);
        }

        while let Some(top) = stack.last_mut() {
            if cancel.load(Ordering::Relaxed) {
                return;
            }

            if let Some(next_subdir) = top.subdirs.pop() {
                if let Some((subdir_frame, files)) = inspect_dir(&next_subdir, false) {
                    for f in files {
                        total_counter.fetch_add(1, Ordering::Relaxed);
                        batch.push(f);
                    }
                    if batch.len() >= 128 {
                        if tx
                            .send(ScanMessage::InitialEntries(std::mem::take(&mut batch)))
                            .is_err()
                        {
                            cancel.store(true, Ordering::Relaxed);
                            return;
                        }
                    }
                    if last_progress.elapsed() > Duration::from_millis(50) {
                        let count = total_counter.load(Ordering::Relaxed);
                        let _ = tx.send(ScanMessage::Progress { scanned_items: count });
                        last_progress = Instant::now();
                    }
                    stack.push(subdir_frame);
                }
            } else {
                let finished = stack.pop().unwrap();
                total_counter.fetch_add(1, Ordering::Relaxed);

                if !finished.is_root {
                    let folder_entry = FileEntry::new(
                        finished.name.clone(),
                        finished.path.clone(),
                        true,
                        finished.size,
                        finished.allocated,
                        finished.modified,
                        finished.items,
                        false,
                        false,
                    );
                    batch.push(folder_entry);

                    if batch.len() >= 128 {
                        if tx
                            .send(ScanMessage::InitialEntries(std::mem::take(&mut batch)))
                            .is_err()
                        {
                            cancel.store(true, Ordering::Relaxed);
                            return;
                        }
                    }
                }

                if let Some(parent) = stack.last_mut() {
                    parent.size += finished.size;
                    parent.allocated += finished.allocated;
                    parent.items += finished.items;
                } else {
                    total_size += finished.size;
                    total_allocated += finished.allocated;
                    total_files += finished.items;
                }

                if last_progress.elapsed() > Duration::from_millis(50) {
                    let count = total_counter.load(Ordering::Relaxed);
                    let _ = tx.send(ScanMessage::Progress { scanned_items: count });
                    last_progress = Instant::now();
                }
            }
        }
    }

    if !batch.is_empty() {
        let _ = tx.send(ScanMessage::InitialEntries(batch));
    }

    if !cancel.load(Ordering::Relaxed) {
        let _ = tx.send(ScanMessage::Completed(ScanSummary {
            total_size,
            total_allocated,
            total_files,
            duration: start_time.elapsed(),
        }));
    }
}
