//! Which process runs the updater.
//!
//! Every window of pdfxray is its own process, and an updater in each would check, download and
//! ask once per window. So only the process that holds [`UpdaterLock`] runs it: an advisory lock
//! (`flock`) on a file in the app data directory, held for the life of the process. The kernel
//! drops the lock when its process ends, however it ends (quit, crash, `kill -9`), so a lock is
//! never left behind, and [`UpdaterSlot`] lets a window that is still open take over by trying
//! again now and then. The file is opened close-on-exec (as Rust opens every file), so the window
//! processes a holder starts do not inherit the lock.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Name of the lock file in the app data directory.
pub const LOCK_FILE: &str = "updater.lock";

/// How long a process that does not run the updater waits before trying to take it over.
pub const RETRY_EVERY: Duration = Duration::from_secs(10);

/// The lock file of the data directory `dir`.
pub fn lock_path(dir: &Path) -> PathBuf {
    dir.join(LOCK_FILE)
}

/// The exclusive right to run the updater, held until it is dropped (or the process ends).
#[derive(Debug)]
pub struct UpdaterLock {
    /// Holds the lock; closing it releases it.
    _file: File,
}

impl UpdaterLock {
    /// Takes the lock at `path` (creating the file and its directory) unless another process, or
    /// another [`UpdaterLock`] of this one, holds it. The file then names the holder's process id,
    /// for people looking into it; the lock, not the content, is what counts.
    pub fn try_acquire(path: &Path) -> io::Result<Option<Self>> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Error(error)) => return Err(error),
        }
        file.set_len(0)?;
        writeln!(file, "{}", std::process::id())?;
        Ok(Some(Self { _file: file }))
    }
}

/// Whether this process runs the updater, and when it tries to again when it does not.
#[derive(Debug)]
pub struct UpdaterSlot<L> {
    held: Option<L>,
    next_try: Option<Instant>,
    gave_up: bool,
}

impl<L> Default for UpdaterSlot<L> {
    fn default() -> Self {
        Self {
            held: None,
            next_try: None,
            gave_up: false,
        }
    }
}

impl<L> UpdaterSlot<L> {
    /// Whether this process runs the updater.
    pub fn is_running(&self) -> bool {
        self.held.is_some()
    }

    /// Tries to become the process that runs the updater, at most once every [`RETRY_EVERY`]:
    /// `acquire` takes the lock, `start` starts the updater once it is held. When `start` fails
    /// the lock is let go and nothing is tried again (another window would fail the same way).
    /// Returns whether the updater runs in this process now.
    pub fn poll(
        &mut self,
        now: Instant,
        acquire: impl FnOnce() -> Option<L>,
        start: impl FnOnce() -> bool,
    ) -> bool {
        if self.held.is_some() {
            return true;
        }
        if self.gave_up || self.next_try.is_some_and(|next| now < next) {
            return false;
        }
        self.next_try = Some(now + RETRY_EVERY);
        let Some(lock) = acquire() else {
            return false;
        };
        if start() {
            self.held = Some(lock);
            true
        } else {
            self.gave_up = true;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdfxray-updater-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn only_one_holder_at_a_time_and_dropping_lets_the_next_one_in() {
        let dir = temp_dir("exclusive");
        let path = lock_path(&dir.join("nested"));

        let first = UpdaterLock::try_acquire(&path).expect("io").expect("free");
        assert!(UpdaterLock::try_acquire(&path).expect("io").is_none());
        assert_eq!(
            fs::read_to_string(&path).expect("file"),
            format!("{}\n", std::process::id())
        );
        drop(first);
        let second = UpdaterLock::try_acquire(&path).expect("io");
        // smoke: the lock file exists where it was asked for
        assert!(second.is_some());
        assert!(path.ends_with("nested/updater.lock") && path.is_file());
        drop(second);
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_child_process_does_not_inherit_the_lock() {
        let dir = temp_dir("cloexec");
        let path = lock_path(&dir);
        let lock = UpdaterLock::try_acquire(&path).expect("io").expect("free");
        // A window started by the holder, still running after the holder let go.
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("sleep");

        drop(lock);
        let taken = UpdaterLock::try_acquire(&path).expect("io");

        let _ = child.kill();
        let _ = child.wait();
        assert!(taken.is_some(), "the child kept the lock alive");
        drop(taken);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_slot_starts_once_when_the_lock_is_free() {
        let now = Instant::now();
        let mut slot = UpdaterSlot::default();
        let starts = Cell::new(0);

        assert!(slot.poll(
            now,
            || Some("lock"),
            || {
                starts.set(starts.get() + 1);
                true
            }
        ));
        // later polls neither lock nor start again
        assert!(slot.poll(
            now + RETRY_EVERY * 3,
            || panic!("no second lock"),
            || panic!("no second start")
        ));
        assert_eq!(starts.get(), 1);
        assert!(slot.is_running());
    }

    #[test]
    fn a_window_takes_over_after_the_holder_is_gone_but_not_more_often_than_the_interval() {
        let now = Instant::now();
        let mut slot: UpdaterSlot<&str> = UpdaterSlot::default();
        let tries = Cell::new(0);
        let held_elsewhere = || {
            tries.set(tries.get() + 1);
            None
        };

        assert!(!slot.poll(now, held_elsewhere, || true));
        assert!(!slot.poll(now + RETRY_EVERY / 2, held_elsewhere, || true));
        assert_eq!(tries.get(), 1, "retried before the interval");
        assert!(!slot.poll(now + RETRY_EVERY, held_elsewhere, || true));
        assert_eq!(tries.get(), 2);

        // the holder quit: the next try after the interval takes over
        assert!(slot.poll(now + RETRY_EVERY * 2, || Some("lock"), || true));
        assert!(slot.is_running());
    }

    #[test]
    fn a_failed_start_lets_the_lock_go_and_is_not_retried() {
        struct Lock<'a>(&'a Cell<bool>);
        impl Drop for Lock<'_> {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }
        let released = Cell::new(false);
        let now = Instant::now();
        let mut slot = UpdaterSlot::default();

        assert!(!slot.poll(now, || Some(Lock(&released)), || false));
        assert!(released.get(), "the lock is still held after a failed start");
        assert!(!slot.poll(now + RETRY_EVERY * 10, || panic!("no retry"), || true));
        // smoke: nothing runs
        assert!(!slot.is_running());
    }
}
