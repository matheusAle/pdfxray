//! Subprocess helper: run a command with captured, size-capped output, a deadline and a
//! cancellation flag. Children are killed (and reaped) on timeout or cancellation.

use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How often the supervising loop checks the child, the deadline and the cancel flag.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Output of a finished child process.
#[derive(Debug)]
pub(crate) struct Captured {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    /// Truncated to the configured cap; the rest of the stream was drained and dropped.
    pub(crate) stderr: Vec<u8>,
}

impl Captured {
    pub(crate) fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).trim().to_owned()
    }
}

/// Why a child did not run to completion.
#[derive(Debug)]
pub(crate) enum ProcError {
    Spawn(std::io::Error),
    Wait(std::io::Error),
    Timeout,
    Cancelled,
}

/// Runs `cmd` to completion. `stdin` is closed, `stdout`/`stderr` are captured.
pub(crate) fn run_captured(
    cmd: &mut Command,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    stdout_cap: usize,
    stderr_cap: usize,
) -> Result<Captured, ProcError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(ProcError::Spawn)?;
    let stdout_reader = child.stdout.take().map(|s| spawn_reader(s, stdout_cap));
    let stderr_reader = child.stderr.take().map(|s| spawn_reader(s, stderr_cap));

    let deadline = Instant::now().checked_add(timeout);
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(e) => break Err(ProcError::Wait(e)),
        }
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            break Err(ProcError::Cancelled);
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            break Err(ProcError::Timeout);
        }
        std::thread::sleep(POLL_INTERVAL);
    };

    if outcome.is_err() {
        // Best effort: the child may have exited between the last poll and the kill.
        let _ = child.kill();
        let _ = child.wait();
    }
    let stdout = join_reader(stdout_reader);
    let stderr = join_reader(stderr_reader);
    outcome.map(|status| Captured {
        status,
        stdout,
        stderr,
    })
}

fn spawn_reader<R: Read + Send + 'static>(mut source: R, cap: usize) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut chunk = [0u8; 16 * 1024];
        loop {
            match source.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = cap.saturating_sub(kept.len());
                    kept.extend_from_slice(&chunk[..n.min(room)]);
                }
            }
        }
        kept
    })
}

fn join_reader(handle: Option<JoinHandle<Vec<u8>>>) -> Vec<u8> {
    handle.and_then(|h| h.join().ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::sync::Arc;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(script);
        c
    }

    #[test]
    fn captures_both_streams_and_status() {
        let out = run_captured(
            &mut sh("printf out; printf err >&2; exit 3"),
            Duration::from_secs(10),
            None,
            1024,
            1024,
        )
        .expect("runs");
        assert_eq!(
            (out.status.code(), out.stdout.clone(), out.stderr_text()),
            (Some(3), b"out".to_vec(), "err".to_owned())
        );
    }

    #[test]
    fn stdout_cap_truncates_but_drains() {
        let out = run_captured(
            &mut sh("yes x | head -c 100000"),
            Duration::from_secs(10),
            None,
            10,
            10,
        )
        .expect("runs");
        assert_eq!((out.status.success(), out.stdout.len()), (true, 10));
    }

    #[test]
    fn timeout_kills_the_child() {
        let started = Instant::now();
        let err = run_captured(&mut sh("sleep 30"), Duration::from_millis(100), None, 16, 16)
            .expect_err("times out");
        assert!(matches!(err, ProcError::Timeout), "got {err:?}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "child was not killed promptly"
        );
    }

    #[test]
    fn cancel_flag_kills_the_child() {
        let flag = Arc::new(AtomicBool::new(false));
        let setter = {
            let flag = Arc::clone(&flag);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                flag.store(true, Ordering::Relaxed);
            })
        };
        let err = run_captured(&mut sh("sleep 30"), Duration::from_secs(60), Some(&flag), 16, 16)
            .expect_err("cancelled");
        let _ = setter.join();
        assert!(matches!(err, ProcError::Cancelled), "got {err:?}");
    }

    #[test]
    fn missing_program_is_a_spawn_error() {
        let err = run_captured(
            &mut Command::new("/definitely/not/here"),
            Duration::from_secs(1),
            None,
            1,
            1,
        )
        .expect_err("no such program");
        assert!(matches!(err, ProcError::Spawn(_)), "got {err:?}");
    }
}
