//! Parallel, cancellable hashing passes used by the duplicate engine.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;

use rayon::prelude::*;
use sha2::{Digest, Sha256};

use crate::duplicate::DuplicateMessage;
use crate::model::FileEntry;

pub const PREFIX_HASH_LEN: usize = 64 * 1024;
pub const SUFFIX_HASH_LEN: usize = 64 * 1024;
const STREAM_BUFFER_SIZE: usize = 1024 * 1024;

pub type Hash = [u8; 32];
pub type Group<'a> = Vec<&'a FileEntry>;

/// Cancellation flag plus optional progress channel shared by all passes.
pub struct ScanContext<'a> {
    pub cancel: &'a AtomicBool,
    pub tx: Option<&'a Sender<DuplicateMessage>>,
}

impl ScanContext<'_> {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn progress(&self, stage: &str, done: usize, total: usize) {
        if let Some(tx) = self.tx {
            let _ = tx.send(DuplicateMessage::Progress { stage: stage.into(), done, total });
        }
    }
}

/// Hashes `len` bytes starting at `offset` (fewer if the file is shorter).
pub fn hash_range(path: &Path, offset: u64, len: usize) -> std::io::Result<Hash> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::with_capacity(len);
    file.take(len as u64).read_to_end(&mut buf)?;
    Ok(Sha256::digest(&buf).into())
}

pub fn hash_prefix(file: &FileEntry) -> std::io::Result<Hash> {
    hash_range(&file.path, 0, PREFIX_HASH_LEN)
}

pub fn hash_suffix(file: &FileEntry) -> std::io::Result<Hash> {
    hash_range(&file.path, file.size.saturating_sub(SUFFIX_HASH_LEN as u64), SUFFIX_HASH_LEN)
}

/// Full streaming SHA-256. Returns `Ok(None)` when cancelled or when the
/// file changed size while it was being read.
pub fn hash_full(file: &FileEntry, cancel: &AtomicBool) -> std::io::Result<Option<Hash>> {
    let mut reader = File::open(&file.path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; STREAM_BUFFER_SIZE];
    let mut total = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    Ok((total == file.size).then(|| hasher.finalize().into()))
}

/// Splits every group by `hash_fn`, in parallel, dropping files that cannot be
/// read and groups that fall below two members. Returns `None` if cancelled.
pub fn refine<'a, F>(
    groups: &[Group<'a>],
    stage: &str,
    report_every: usize,
    ctx: &ScanContext,
    hash_fn: F,
) -> Option<Vec<(Group<'a>, Hash)>>
where
    F: Fn(&FileEntry) -> std::io::Result<Option<Hash>> + Sync,
{
    let candidates: Vec<(usize, &FileEntry)> = groups
        .iter()
        .enumerate()
        .flat_map(|(i, g)| g.iter().map(move |f| (i, *f)))
        .collect();
    let total = candidates.len();
    if total == 0 {
        return Some(Vec::new());
    }
    ctx.progress(stage, 0, total);

    let counter = AtomicUsize::new(0);
    let hashed: Vec<Option<(usize, Hash, &FileEntry)>> = candidates
        .par_iter()
        .map(|&(i, file)| {
            if ctx.cancelled() {
                return None;
            }
            let result = hash_fn(file).ok().flatten().map(|h| (i, h, file));
            let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
            if done.is_multiple_of(report_every) || done == total {
                ctx.progress(stage, done, total);
            }
            result
        })
        .collect();
    if ctx.cancelled() {
        return None;
    }

    let mut buckets: HashMap<(usize, Hash), Group<'a>> = HashMap::new();
    for (i, hash, file) in hashed.into_iter().flatten() {
        buckets.entry((i, hash)).or_default().push(file);
    }
    Some(
        buckets
            .into_iter()
            .filter(|(_, g)| g.len() >= 2)
            .map(|((_, hash), g)| (g, hash))
            .collect(),
    )
}
