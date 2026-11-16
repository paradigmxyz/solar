//! Kills compiler processes that run far longer than any fixture should.
//!
//! `ui_test` waits for each child process with no deadline, so one hung compiler stalls the whole
//! run until `cargo-nextest` times it out and kills it, with no hint of which fixture hung. A
//! thread polls `ps` for children of the test runner and kills any that has run for [`LIMIT`].
//! The runner then sees an abnormal exit and usually reports that fixture as a failed test, with
//! its command, and the other fixtures still run. A fixture that expects no exit status could pass
//! on the output printed before the kill, so [`killed`] also fails the run. Each kill is logged to
//! stderr.
//!
//! This covers only the compiler and `FileCheck` children. It does not cover a hang in the runner
//! itself, such as in the interpreters of `run-call` fixtures, and it needs the Unix `ps` and
//! `kill`; elsewhere the watchdog does nothing.

use std::{
    collections::HashSet,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

/// The time a child may run before it is killed. Normal fixtures take seconds at most.
const LIMIT: Duration = Duration::from_secs(120);

/// The time between polls.
const POLL: Duration = Duration::from_secs(10);

/// Whether the watchdog has killed a child.
static KILLED: AtomicBool = AtomicBool::new(false);

/// Starts the watchdog thread, which lasts until the process exits.
pub(crate) fn spawn() {
    if cfg!(unix) {
        let _ = thread::Builder::new().name("watchdog".into()).spawn(|| {
            // `ps` reads a just-forked process in a racy state and can report a bogus run time, so
            // a child is killed only if the previous poll also found it over the limit.
            let mut over = HashSet::new();
            loop {
                thread::sleep(POLL);
                let mut now_over = HashSet::new();
                for child in children().into_iter().filter(|child| child.elapsed >= LIMIT) {
                    now_over.insert(child.pid);
                    if over.contains(&child.pid) {
                        eprintln!(
                            "\nsolar-tester: killing `{}` after {}s",
                            child.command,
                            child.elapsed.as_secs()
                        );
                        KILLED.store(true, Ordering::Relaxed);
                        let _ =
                            Command::new("kill").args(["-KILL", &child.pid.to_string()]).status();
                    }
                }
                over = now_over;
            }
        });
    }
}

/// Returns whether the watchdog has killed a child.
pub(crate) fn killed() -> bool {
    KILLED.load(Ordering::Relaxed)
}

/// A child process of the test runner.
#[derive(Debug, PartialEq)]
struct Child {
    pid: u32,
    elapsed: Duration,
    command: String,
}

/// Returns the running children of this process.
fn children() -> Vec<Child> {
    let output = Command::new("ps").args(["-A", "-o", "pid=,ppid=,etime=,args="]).output();
    let Ok(output) = output else { return Vec::new() };
    let me = std::process::id();
    String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| parse(line, me)).collect()
}

/// Parses a `pid ppid etime args` line of `ps`, if the process has parent `ppid`.
fn parse(line: &str, ppid: u32) -> Option<Child> {
    let (pid, rest) = line.trim_start().split_once(' ')?;
    let (parent, rest) = rest.trim_start().split_once(' ')?;
    if parent.parse::<u32>().ok()? != ppid {
        return None;
    }
    let (etime, command) = rest.trim_start().split_once(' ')?;
    Some(Child {
        pid: pid.parse().ok()?,
        elapsed: elapsed(etime)?,
        command: command.trim_start().to_string(),
    })
}

/// Parses a `ps` elapsed time, `[[DD-]HH:]MM:SS`.
fn elapsed(etime: &str) -> Option<Duration> {
    let (days, clock) = etime.split_once('-').unwrap_or(("0", etime));
    let mut seconds = 0;
    for part in clock.split(':') {
        seconds = seconds * 60 + part.parse::<u64>().ok()?;
    }
    Some(Duration::from_secs(days.parse::<u64>().ok()? * 86400 + seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_elapsed_times() {
        assert_eq!(elapsed("00:07"), Some(Duration::from_secs(7)));
        assert_eq!(elapsed("12:34"), Some(Duration::from_secs(12 * 60 + 34)));
        assert_eq!(elapsed("1:02:03"), Some(Duration::from_secs(3723)));
        assert_eq!(
            elapsed("2-03:04:05"),
            Some(Duration::from_secs(((2 * 24 + 3) * 60 + 4) * 60 + 5))
        );
        assert_eq!(elapsed("soon"), None);
    }

    #[test]
    fn parses_children_of_a_process() {
        let line = "  4242  100    02:00 /target/solar -j1 --emit=abi tests/ui/a b.sol";
        assert_eq!(
            parse(line, 100),
            Some(Child {
                pid: 4242,
                elapsed: Duration::from_secs(120),
                command: "/target/solar -j1 --emit=abi tests/ui/a b.sol".into(),
            })
        );
        assert_eq!(parse(line, 101), None);
    }
}
