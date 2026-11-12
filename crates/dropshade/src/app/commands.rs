//! Noticing when a long command finishes, by watching which process is in a tab's foreground.

use std::time::{Duration, Instant};

use dropshade_term::ForegroundProcess;

/// The command in a tab's foreground, and since when it is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pid: i32,
    name: String,
    since: Instant,
}

/// A command that ran long enough to be worth a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    pub name: String,
    pub elapsed: Duration,
}

/// Advances a tab from the command seen last time to the one in the foreground `now`. A command
/// finished when it left the foreground, either to the shell or to another command; it counts
/// when it ran for at least `min`.
pub fn step(
    previous: Option<Running>,
    current: Option<ForegroundProcess>,
    now: Instant,
    min: Duration,
) -> (Option<Running>, Option<Finished>) {
    let same = matches!((&previous, &current), (Some(running), Some(process)) if running.pid == process.pid);
    if same {
        return (previous, None);
    }
    let finished = previous
        .map(|running| Finished {
            elapsed: now.saturating_duration_since(running.since),
            name: running.name,
        })
        .filter(|finished| finished.elapsed >= min);
    let running = current.map(|process| Running {
        pid: process.pid,
        name: process.name,
        since: now,
    });
    (running, finished)
}

/// A duration as people read it: "45 s", "3 min 7 s", "2 h 5 min".
pub fn describe(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match (hours, minutes) {
        (0, 0) => format!("{seconds} s"),
        (0, _) if seconds == 0 => format!("{minutes} min"),
        (0, _) => format!("{minutes} min {seconds} s"),
        (_, 0) => format!("{hours} h"),
        _ => format!("{hours} h {minutes} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(10);

    fn process(pid: i32, name: &str) -> ForegroundProcess {
        ForegroundProcess {
            pid,
            name: name.into(),
        }
    }

    fn started(pid: i32, name: &str, at: Instant) -> Option<Running> {
        step(None, Some(process(pid, name)), at, MIN).0
    }

    #[test]
    fn an_idle_shell_finishes_nothing() {
        assert_eq!(step(None, None, Instant::now(), MIN), (None, None));
    }

    #[test]
    fn a_command_still_running_keeps_its_start() {
        let start = Instant::now();
        let running = started(7, "make", start);
        let (still, finished) = step(
            running.clone(),
            Some(process(7, "make")),
            start + MIN * 2,
            MIN,
        );
        assert_eq!(still, running);
        assert_eq!(finished, None);
    }

    #[test]
    fn a_long_command_that_returns_to_the_shell_finished() {
        let start = Instant::now();
        let running = started(7, "make", start);
        let (now_running, finished) = step(running, None, start + MIN, MIN);
        assert_eq!(now_running, None);
        assert_eq!(
            finished,
            Some(Finished {
                name: "make".into(),
                elapsed: MIN
            })
        );
    }

    #[test]
    fn a_short_command_does_not_count() {
        let start = Instant::now();
        let running = started(7, "ls", start);
        let (_, finished) = step(running, None, start + MIN / 2, MIN);
        assert_eq!(finished, None);
    }

    #[test]
    fn a_command_replaced_by_another_finished_and_the_next_one_starts() {
        let start = Instant::now();
        let running = started(7, "make", start);
        let later = start + MIN * 3;
        let (next, finished) = step(running, Some(process(9, "cargo")), later, MIN);
        assert_eq!(
            finished.map(|finished| finished.name).as_deref(),
            Some("make")
        );
        assert_eq!(next, started(9, "cargo", later));
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(describe(Duration::from_secs(45)), "45 s");
        assert_eq!(describe(Duration::from_secs(180)), "3 min");
        assert_eq!(describe(Duration::from_secs(187)), "3 min 7 s");
        assert_eq!(describe(Duration::from_secs(7200)), "2 h");
        assert_eq!(describe(Duration::from_secs(7500)), "2 h 5 min");
    }
}
