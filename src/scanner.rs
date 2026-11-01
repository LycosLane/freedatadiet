use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::model::FileEntry;
use crate::platform::{get_allocated_size_from_meta, get_file_id_and_links, FileId};

pub use crate::scanner_excludes::CompiledExcludes;

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

    let compiled_excludes = Arc::new(CompiledExcludes::compile(&excludes));

    thread::spawn(move || {
        let start_time = Instant::now();
        let total_counter = Arc::new(AtomicUsize::new(0));
        let seen_hardlinks: Arc<Mutex<HashSet<FileId>>> = Arc::new(Mutex::new(HashSet::new()));

        if flatten || root_paths.len() > 1 {
            scan_flattened(
                root_paths,
                compiled_excludes,
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
                    let a = get_allocated_size_from_meta(&path, meta_opt.as_ref(), s);
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

                let mut item = FileEntry { size, allocated_size: allocated, modified, ..FileEntry::new(name.clone(), path.clone(), is_dir) };
                item.is_scanning = is_scanning;
                initial_entries.push(item);

                if is_dir {
                    child_paths.push((path, name));
                }
            }
        }

        // Send all discovered entries immediately so UI shows folders without delay!
        let _ = tx.send(ScanMessage::InitialEntries(initial_entries));

        let child_paths = Arc::new(child_paths);
        let (worker_tx, worker_rx) = channel();

        let cancel_w = Arc::clone(&cancel_clone);
        let total_counter_w = Arc::clone(&total_counter);
        let excludes_w = Arc::clone(&compiled_excludes);
        let seen_hardlinks_w = Arc::clone(&seen_hardlinks);
        let child_paths_w = Arc::clone(&child_paths);
        let worker_tx_w = worker_tx.clone();

        let worker_handle = thread::spawn(move || {
            (0..child_paths_w.len()).into_par_iter().for_each(|i| {
                if cancel_w.load(Ordering::Relaxed) {
                    return;
                }
                let (path, name) = &child_paths_w[i];
                let entry = process_single_entry(
                    path,
                    name,
                    &cancel_w,
                    &total_counter_w,
                    &excludes_w,
                    &seen_hardlinks_w,
                );
                let _ = worker_tx_w.send(entry);
            });
        });
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

        let _ = worker_handle.join();

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

/// `symlink_metadata().is_dir()` is false for links, so follow the link only to learn whether it points at a folder.
fn is_dir_entry(path: &Path, meta: &fs::Metadata) -> bool {
    meta.is_dir() || (meta.file_type().is_symlink() && fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false))
}

fn process_single_entry(
    path: &Path,
    name: &str,
    cancel: &Arc<AtomicBool>,
    total_counter: &Arc<AtomicUsize>,
    excludes: &CompiledExcludes,
    seen_hardlinks: &Arc<Mutex<HashSet<FileId>>>,
) -> FileEntry {
    total_counter.fetch_add(1, Ordering::Relaxed);

    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => {
            return FileEntry { is_access_denied: true, ..FileEntry::new(name.to_string(), path.to_path_buf(), path.is_dir()) };
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
        let allocated = if is_dup { 0 } else { get_allocated_size_from_meta(path, Some(&metadata), size) };
        FileEntry { size: counted_size, allocated_size: allocated, modified, ..FileEntry::new(name.to_string(), path.to_path_buf(), false) }
    } else {
        // Junctions and symlinks are listed but never followed, so loops and double counting cannot happen
        if metadata.file_type().is_symlink() {
            return FileEntry { modified, ..FileEntry::new(name.to_string(), path.to_path_buf(), true) };
        }

        // If this directory itself is excluded, skip walking children
        if excludes.matches(name, path) {
            let size = metadata.len();
            let allocated = get_allocated_size_from_meta(path, Some(&metadata), size);
            return FileEntry {
                size,
                allocated_size: allocated,
                modified,
                ..FileEntry::new(format!("{} (ignored)", name), path.to_path_buf(), true)
            };
        }

        let visited_dirs = Arc::new(Mutex::new(HashSet::new()));
        if let (Some(root_fid), _) = get_file_id_and_links(path, &metadata) {
            visited_dirs.lock().unwrap_or_else(|p| p.into_inner()).insert(root_fid);
        }

        let stats = walk_dir_parallel(
            path,
            cancel,
            total_counter,
            excludes,
            seen_hardlinks,
            &visited_dirs,
            0,
        );

        FileEntry { size: stats.size, allocated_size: stats.allocated, modified, item_count: stats.count, is_access_denied: stats.access_denied, is_partially_locked: stats.partially_locked, ..FileEntry::new(name.to_string(), path.to_path_buf(), true) }
    }
}

#[derive(Default)]
struct DirScanStats {
    size: u64,
    allocated: u64,
    count: usize,
    access_denied: bool,
    partially_locked: bool,
}

fn walk_dir_parallel(
    dir_path: &Path,
    cancel: &Arc<AtomicBool>,
    total_counter: &Arc<AtomicUsize>,
    excludes: &CompiledExcludes,
    seen_hardlinks: &Arc<Mutex<HashSet<FileId>>>,
    visited_dirs: &Arc<Mutex<HashSet<FileId>>>,
    depth: usize,
) -> DirScanStats {
    if cancel.load(Ordering::Relaxed) {
        return DirScanStats::default();
    }

    let entries = match fs::read_dir(dir_path) {
        Ok(e) => e,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            return DirScanStats {
                size: 0,
                allocated: 0,
                count: 0,
                access_denied: depth == 0,
                partially_locked: depth > 0,
            };
        }
        Err(_) => return DirScanStats::default(),
    };

    let mut local_stats = DirScanStats::default();
    let mut subdirs: Vec<PathBuf> = Vec::new();

    for entry_res in entries {
        if cancel.load(Ordering::Relaxed) {
            return local_stats;
        }

        if let Ok(e) = entry_res {
            total_counter.fetch_add(1, Ordering::Relaxed);
            let sub_path = e.path();
            let sub_name = e.file_name().to_string_lossy().to_string();

            if excludes.matches(&sub_name, &sub_path) {
                continue;
            }

            if let Ok(meta) = fs::symlink_metadata(&sub_path) {
                local_stats.count += 1;
                if is_dir_entry(&sub_path, &meta) {
                    if meta.file_type().is_symlink() {
                        continue;
                    }
                    let (dir_fid, _) = get_file_id_and_links(&sub_path, &meta);
                    if let Some(dfid) = dir_fid {
                        let mut v = visited_dirs.lock().unwrap_or_else(|p| p.into_inner());
                        if !v.insert(dfid) {
                            continue;
                        }
                    }
                    subdirs.push(sub_path);
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
                        local_stats.size += fsize;
                        local_stats.allocated += get_allocated_size_from_meta(&sub_path, Some(&meta), fsize);
                    }
                }
            }
        }
    }

    if subdirs.is_empty() {
        return local_stats;
    }

    // Parallelize with Rayon if at top levels and multiple subdirectories exist
    if depth < 3 && subdirs.len() > 1 {
        let child_stats: DirScanStats = subdirs
            .into_par_iter()
            .map(|sub| {
                walk_dir_parallel(
                    &sub,
                    cancel,
                    total_counter,
                    excludes,
                    seen_hardlinks,
                    visited_dirs,
                    depth + 1,
                )
            })
            .reduce(DirScanStats::default, |mut a, b| {
                a.size += b.size;
                a.allocated += b.allocated;
                a.count += b.count;
                a.partially_locked |= b.partially_locked || b.access_denied;
                a
            });

        local_stats.size += child_stats.size;
        local_stats.allocated += child_stats.allocated;
        local_stats.count += child_stats.count;
        local_stats.partially_locked |= child_stats.partially_locked || child_stats.access_denied;
    } else {
        for sub in subdirs {
            let s = walk_dir_sequential(&sub, cancel, total_counter, excludes, seen_hardlinks, visited_dirs);
            local_stats.size += s.size;
            local_stats.allocated += s.allocated;
            local_stats.count += s.count;
            local_stats.partially_locked |= s.partially_locked || s.access_denied;
        }
    }

    local_stats
}

fn walk_dir_sequential(
    root: &Path,
    cancel: &Arc<AtomicBool>,
    total_counter: &Arc<AtomicUsize>,
    excludes: &CompiledExcludes,
    seen_hardlinks: &Arc<Mutex<HashSet<FileId>>>,
    visited_dirs: &Arc<Mutex<HashSet<FileId>>>,
) -> DirScanStats {
    let mut stats = DirScanStats::default();
    let mut stack = vec![root.to_path_buf()];

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

                        if excludes.matches(&sub_name, &sub_path) {
                            continue;
                        }

                        if let Ok(meta) = fs::symlink_metadata(&sub_path) {
                            stats.count += 1;
                            if is_dir_entry(&sub_path, &meta) {
                                if meta.file_type().is_symlink() {
                                    continue;
                                }
                                let (dir_fid, _) = get_file_id_and_links(&sub_path, &meta);
                                if let Some(dfid) = dir_fid {
                                    let mut v = visited_dirs.lock().unwrap_or_else(|p| p.into_inner());
                                    if !v.insert(dfid) {
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
                                    stats.size += fsize;
                                    stats.allocated += get_allocated_size_from_meta(&sub_path, Some(&meta), fsize);
                                }
                            }
                        }
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                if current_dir == root {
                    stats.access_denied = true;
                } else {
                    stats.partially_locked = true;
                }
            }
            Err(_) => {}
        }
    }

    stats
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
    excludes: Arc<CompiledExcludes>,
    cancel: Arc<AtomicBool>,
    total_counter: Arc<AtomicUsize>,
    seen_hardlinks: Arc<Mutex<HashSet<FileId>>>,
    tx: std::sync::mpsc::Sender<ScanMessage>,
    start_time: Instant,
) {
    let mut total_size = 0u64;
    let mut total_allocated = 0u64;
    let mut total_files = 0usize;
    let mut batch = Vec::with_capacity(2048);
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
            let alloc = get_allocated_size_from_meta(&root_path, Some(&root_meta), size);
            total_size += size;
            total_allocated += alloc;
            total_files += 1;
            let name = root_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| root_path.to_string_lossy().to_string());
            let file_entry = FileEntry { size, allocated_size: alloc, modified: root_meta.modified().ok(), ..FileEntry::new(name, root_path, false) };
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
                    let path = entry.path();
                    if excludes.matches(&name, &path) {
                        continue;
                    }
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
                            let alloc = get_allocated_size_from_meta(&path, Some(&meta), size);
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

                            let file_entry = FileEntry { size: if is_dup { 0 } else { size }, allocated_size: if is_dup { 0 } else { alloc }, modified: meta.modified().ok(), ..FileEntry::new(name, path, false) };
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
                    if batch.len() >= 2048
                        && tx
                            .send(ScanMessage::InitialEntries(std::mem::take(&mut batch)))
                            .is_err()
                        {
                            cancel.store(true, Ordering::Relaxed);
                            return;
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
                    let folder_entry = FileEntry { size: finished.size, allocated_size: finished.allocated, modified: finished.modified, item_count: finished.items, ..FileEntry::new(finished.name.clone(), finished.path.clone(), true) };
                    batch.push(folder_entry);

                    if batch.len() >= 2048
                        && tx
                            .send(ScanMessage::InitialEntries(std::mem::take(&mut batch)))
                            .is_err()
                        {
                            cancel.store(true, Ordering::Relaxed);
                            return;
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
