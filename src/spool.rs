//! Spool mode's directory watcher — the queue-management half of the
//! printer in the corner of the lab (`pscat --spool DIR`). This module
//! is pure bookkeeping, no window: it decides *which* files count as
//! new print jobs, so the policy is testable headlessly. The window
//! integration (when to poll, running jobs) lives in `window.rs`.
//!
//! Policy, pinned by the tests below:
//! - only `.ps`/`.eps` files (case-insensitive) are jobs;
//! - files already in the directory at startup are not printed — the
//!   printer coming online doesn't reprint the tray, it prints what
//!   *lands* from then on;
//! - a candidate must hold the same size+mtime across two consecutive
//!   polls before it's queued, so a file still being copied in isn't
//!   printed half-written;
//! - rewriting an already-printed file makes it a new job again.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A file's identity for change detection. mtime alone has coarse
/// granularity on some filesystems; size catches most same-second
/// rewrites too.
type Stamp = (u64, SystemTime);

pub struct Watcher {
    dir: PathBuf,
    /// Printed (or present at startup): path → stamp at queue time.
    seen: HashMap<PathBuf, Stamp>,
    /// Noticed but not yet stable across two polls.
    settling: HashMap<PathBuf, Stamp>,
    queue: VecDeque<PathBuf>,
}

impl Watcher {
    /// Watch `dir`, treating its current spoolable contents as already
    /// printed.
    pub fn new(dir: &Path) -> io::Result<Self> {
        let mut w = Watcher {
            dir: dir.to_path_buf(),
            seen: HashMap::new(),
            settling: HashMap::new(),
            queue: VecDeque::new(),
        };
        for (path, stamp) in w.scan()? {
            w.seen.insert(path, stamp);
        }
        Ok(w)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Look at the directory once, promoting stable new arrivals into
    /// the job queue. Call `next_job` afterwards to drain it. Errors
    /// reading the directory are returned; the watcher state is
    /// unharmed and a later poll can succeed.
    pub fn poll(&mut self) -> io::Result<()> {
        let listing = self.scan()?;
        // Forget settling candidates that vanished before stabilizing.
        self.settling
            .retain(|path, _| listing.iter().any(|(p, _)| p == path));
        for (path, stamp) in listing {
            if self.seen.get(&path) == Some(&stamp) {
                continue;
            }
            match self.settling.get(&path) {
                Some(&pending) if pending == stamp => {
                    // Unchanged since last poll: safe to print.
                    self.settling.remove(&path);
                    self.seen.insert(path.clone(), stamp);
                    self.queue.push_back(path);
                }
                _ => {
                    // New, or still growing: (re)start the clock.
                    self.settling.insert(path, stamp);
                }
            }
        }
        Ok(())
    }

    /// The next queued job, oldest first.
    pub fn next_job(&mut self) -> Option<PathBuf> {
        self.queue.pop_front()
    }

    /// Spoolable files currently in the directory, name-sorted so
    /// same-poll arrivals print in a deterministic order.
    fn scan(&self) -> io::Result<Vec<(PathBuf, Stamp)>> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if !is_spoolable(&path) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            files.push((path, (meta.len(), mtime)));
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(files)
    }
}

fn is_spoolable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ps") || e.eq_ignore_ascii_case("eps"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch directory per test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pscat-spool-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// poll() twice — a stable file needs two sightings to queue.
    fn poll2(w: &mut Watcher) {
        w.poll().expect("poll");
        w.poll().expect("poll");
    }

    #[test]
    fn startup_contents_are_not_reprinted() {
        let dir = scratch("startup");
        std::fs::write(dir.join("old.ps"), b"1 2 add").expect("write");
        let mut w = Watcher::new(&dir).expect("watcher");
        poll2(&mut w);
        assert_eq!(w.next_job(), None, "pre-existing file must not queue");
    }

    #[test]
    fn a_new_file_queues_after_two_stable_polls() {
        let dir = scratch("arrival");
        let mut w = Watcher::new(&dir).expect("watcher");
        std::fs::write(dir.join("job.ps"), b"1 2 add").expect("write");
        w.poll().expect("poll");
        assert_eq!(w.next_job(), None, "one sighting is not stable yet");
        w.poll().expect("poll");
        assert_eq!(w.next_job(), Some(dir.join("job.ps")));
        poll2(&mut w);
        assert_eq!(w.next_job(), None, "a printed job doesn't re-queue");
    }

    #[test]
    fn rewriting_a_printed_file_requeues_it() {
        let dir = scratch("rewrite");
        let mut w = Watcher::new(&dir).expect("watcher");
        std::fs::write(dir.join("job.ps"), b"1").expect("write");
        poll2(&mut w);
        assert_eq!(w.next_job(), Some(dir.join("job.ps")));
        // Different length, so the stamp differs even within mtime
        // granularity.
        std::fs::write(dir.join("job.ps"), b"1 2 add").expect("rewrite");
        poll2(&mut w);
        assert_eq!(w.next_job(), Some(dir.join("job.ps")));
    }

    #[test]
    fn a_growing_file_waits_until_it_settles() {
        let dir = scratch("growing");
        let mut w = Watcher::new(&dir).expect("watcher");
        std::fs::write(dir.join("big.ps"), b"part").expect("write");
        w.poll().expect("poll");
        std::fs::write(dir.join("big.ps"), b"part one and part two").expect("grow");
        w.poll().expect("poll");
        assert_eq!(w.next_job(), None, "still growing between polls");
        poll2(&mut w);
        assert_eq!(w.next_job(), Some(dir.join("big.ps")));
    }

    #[test]
    fn only_ps_and_eps_files_are_jobs() {
        let dir = scratch("filter");
        let mut w = Watcher::new(&dir).expect("watcher");
        std::fs::write(dir.join("notes.txt"), b"not a job").expect("write");
        std::fs::write(dir.join(".hidden.ps.swp"), b"nor this").expect("write");
        std::fs::write(dir.join("job.EPS"), b"0 0 moveto").expect("write");
        poll2(&mut w);
        assert_eq!(w.next_job(), Some(dir.join("job.EPS")));
        assert_eq!(w.next_job(), None);
    }

    #[test]
    fn a_file_that_vanishes_while_settling_is_forgotten() {
        let dir = scratch("vanish");
        let mut w = Watcher::new(&dir).expect("watcher");
        std::fs::write(dir.join("gone.ps"), b"1").expect("write");
        w.poll().expect("poll");
        std::fs::remove_file(dir.join("gone.ps")).expect("remove");
        poll2(&mut w);
        assert_eq!(w.next_job(), None);
        // And it can come back later as a brand-new job.
        std::fs::write(dir.join("gone.ps"), b"2").expect("rewrite");
        poll2(&mut w);
        assert_eq!(w.next_job(), Some(dir.join("gone.ps")));
    }
}
