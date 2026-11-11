//! Focus timer state. Pure logic: drawing and the end-of-session notice live in the notch module.

use std::time::{Duration, Instant};

const MIN: Duration = Duration::from_secs(1);
const MAX: Duration = Duration::from_secs(180 * 60);

const SHORT_BREAK: Duration = Duration::from_secs(5 * 60);
const LONG_BREAK: Duration = Duration::from_secs(15 * 60);
/// Focus sessions before the long break.
pub const ROUNDS: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Focus,
    Break,
    LongBreak,
}

pub struct Focus {
    /// Length of the current phase.
    total: Duration,
    /// Time left as of the last pause (or the full length when idle).
    left: Duration,
    since: Option<Instant>,
    /// With rounds on, focus and break follow each other by themselves: focus, break, ... and a long break after the
    /// fourth focus. `focus_len` is the length the user picked for the focus phase.
    rounds: bool,
    focus_len: Duration,
    phase: Phase,
    /// Focus sessions finished in this cycle.
    done: u8,
}

impl Focus {
    pub fn new(minutes: u64) -> Self {
        let total = Duration::from_secs(minutes * 60);
        Self { total, left: total, since: None, rounds: false, focus_len: total, phase: Phase::Focus, done: 0 }
    }

    pub fn rounds(&self) -> bool {
        self.rounds
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn done(&self) -> u8 {
        self.done
    }

    /// Turns rounds on or off. Either way the timer goes back to a stopped focus session.
    pub fn set_rounds(&mut self, on: bool) {
        self.rounds = on;
        (self.phase, self.done, self.since) = (Phase::Focus, 0, None);
        (self.total, self.left) = (self.focus_len, self.focus_len);
    }

    pub fn total(&self) -> Duration {
        self.total
    }

    pub fn running(&self) -> bool {
        self.since.is_some()
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        match self.since {
            Some(t) => self.left.saturating_sub(now.saturating_duration_since(t)),
            None => self.left,
        }
    }

    /// 0.0 at the start, 1.0 when time is up.
    pub fn progress(&self, now: Instant) -> f32 {
        1.0 - self.remaining(now).as_secs_f32() / self.total.as_secs_f32().max(1.0)
    }

    pub fn toggle(&mut self, now: Instant) {
        match self.since.take() {
            Some(t) => self.left = self.left.saturating_sub(now.saturating_duration_since(t)),
            None => self.since = Some(now),
        }
    }

    /// Picks a new length, 1 second to 3 hours. Ignored while running so a stray click cannot wipe a session, and during
    /// a break, whose length is fixed.
    pub fn set_duration(&mut self, d: Duration) {
        if !self.running() && self.phase == Phase::Focus {
            let d = d.clamp(MIN, MAX);
            (self.total, self.left, self.focus_len) = (d, d, d);
        }
    }

    /// Starts a fresh countdown of `d` right away, replacing whatever was running.
    pub fn start(&mut self, d: Duration, now: Instant) {
        let d = d.clamp(MIN, MAX);
        *self = Self { total: d, left: d, since: Some(now), rounds: self.rounds, focus_len: d, phase: Phase::Focus, done: 0 };
    }

    pub fn set_minutes(&mut self, minutes: u64) {
        self.set_duration(Duration::from_secs(minutes * 60));
    }

    /// Longer or shorter by `secs`, within the same limits.
    pub fn adjust(&mut self, secs: i64) {
        let t = (self.total.as_secs() as i64 + secs).max(1) as u64;
        self.set_duration(Duration::from_secs(t));
    }

    pub fn reset(&mut self) {
        self.left = self.total;
        self.since = None;
    }

    /// The phase that just ended, exactly once, when the countdown reaches zero. Without rounds the timer rewinds to
    /// the full length and stops. With rounds the next phase starts at once.
    pub fn tick(&mut self, now: Instant) -> Option<Phase> {
        if !(self.running() && self.remaining(now).is_zero()) {
            return None;
        }
        let ended = self.phase;
        if !self.rounds {
            self.reset();
            return Some(ended);
        }
        let next = match ended {
            Phase::Focus => {
                self.done += 1;
                if self.done >= ROUNDS { (Phase::LongBreak, LONG_BREAK) } else { (Phase::Break, SHORT_BREAK) }
            }
            Phase::Break => (Phase::Focus, self.focus_len),
            Phase::LongBreak => {
                self.done = 0;
                (Phase::Focus, self.focus_len)
            }
        };
        (self.phase, self.total) = (next.0, next.1);
        self.left = self.total;
        self.since = Some(now);
        Some(ended)
    }
}

/// What the user types into the timer: "25" is minutes, "25:30" is minutes and seconds. 1 second to 3 hours.
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    let secs = match s.split_once(':') {
        Some((m, sec)) => m.trim().parse::<u64>().ok()?.checked_mul(60)?.checked_add(sec.trim().parse::<u64>().ok().filter(|v| *v < 60)?)?,
        None => s.parse::<u64>().ok()?.checked_mul(60)?,
    };
    let d = Duration::from_secs(secs);
    (MIN..=MAX).contains(&d).then_some(d)
}

/// What is said to a command box: "25", "25:30", "10m", "90s", "1h30m", "10 min", "1 hour 30 minutes".
pub fn parse_spoken(s: &str) -> Option<Duration> {
    let s = s.trim().to_lowercase();
    if let Some(d) = parse_duration(&s) {
        return Some(d);
    }
    let s = s
        .replace("minutes", "m")
        .replace("minute", "m")
        .replace("mins", "m")
        .replace("min", "m")
        .replace("hours", "h")
        .replace("hour", "h")
        .replace("hrs", "h")
        .replace("hr", "h")
        .replace("seconds", "s")
        .replace("second", "s")
        .replace("secs", "s")
        .replace("sec", "s");
    let (mut total, mut num, mut seen) = (0u64, String::new(), false);
    for ch in s.chars() {
        match ch {
            '0'..='9' => num.push(ch),
            'h' | 'm' | 's' if !num.is_empty() => {
                let unit = match ch {
                    'h' => 3600,
                    'm' => 60,
                    _ => 1,
                };
                total = total.checked_add(num.parse::<u64>().ok()?.checked_mul(unit)?)?;
                num.clear();
                seen = true;
            }
            ' ' => {}
            _ => return None,
        }
    }
    let d = Duration::from_secs(total);
    (num.is_empty() && seen && (MIN..=MAX).contains(&d)).then_some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_down_pauses_and_finishes() {
        let t0 = Instant::now();
        let mut f = Focus::new(25);
        assert!(!f.running());
        f.toggle(t0);
        assert_eq!(f.remaining(t0 + Duration::from_secs(60)), Duration::from_secs(24 * 60));

        f.toggle(t0 + Duration::from_secs(10));
        assert!(!f.running());
        assert_eq!(f.remaining(t0 + Duration::from_secs(999)), Duration::from_secs(25 * 60 - 10));

        f.toggle(t0 + Duration::from_secs(20));
        assert_eq!(f.tick(t0 + Duration::from_secs(30)), None);
        assert_eq!(f.tick(t0 + Duration::from_secs(20 + 25 * 60)), Some(Phase::Focus));
        assert!(!f.running());
        assert_eq!(f.remaining(t0), Duration::from_secs(25 * 60));
        assert_eq!(f.tick(t0 + Duration::from_secs(99999)), None);
    }

    #[test]
    fn rounds_alternate_focus_and_breaks_with_a_long_break_after_four() {
        let mut t = Instant::now();
        let mut f = Focus::new(1);
        f.set_rounds(true);
        f.toggle(t);
        let mut seen = Vec::new();
        for _ in 0..9 {
            let left = f.remaining(t);
            t += left;
            let ended = f.tick(t).expect("the phase ends when its time is up");
            seen.push((ended, f.phase(), f.running(), f.total().as_secs()));
        }
        let (f_, b_, l_) = (Phase::Focus, Phase::Break, Phase::LongBreak);
        assert_eq!(
            seen,
            [
                (f_, b_, true, 300),
                (b_, f_, true, 60),
                (f_, b_, true, 300),
                (b_, f_, true, 60),
                (f_, b_, true, 300),
                (b_, f_, true, 60),
                (f_, l_, true, 900),
                (l_, f_, true, 60),
                (f_, b_, true, 300),
            ]
        );
        assert_eq!(f.done(), 1, "the count started again after the long break");
    }

    #[test]
    fn rounds_leave_the_break_length_alone_and_turning_them_off_goes_back_to_focus() {
        let t = Instant::now();
        let mut f = Focus::new(25);
        f.set_rounds(true);
        f.toggle(t);
        f.tick(t + Duration::from_secs(25 * 60));
        assert_eq!(f.phase(), Phase::Break);
        f.reset();
        f.set_minutes(45);
        assert_eq!(f.total(), Duration::from_secs(300), "a break cannot be resized");
        f.set_rounds(false);
        assert_eq!((f.phase(), f.running(), f.total().as_secs(), f.done()), (Phase::Focus, false, 25 * 60, 0));
        f.start(Duration::from_secs(600), t);
        assert!(!f.rounds());
    }

    #[test]
    fn parses_typed_durations() {
        let m = |s: u64| Some(Duration::from_secs(s));
        assert_eq!(parse_duration("25"), m(25 * 60));
        assert_eq!(parse_duration(" 5 "), m(300));
        assert_eq!(parse_duration("25:30"), m(25 * 60 + 30));
        assert_eq!(parse_duration("0:45"), m(45));
        assert_eq!(parse_duration("180"), m(180 * 60));
        for bad in ["", "abc", "0", "0:00", "181", "5:60", "-5", "1:2:3", "99999999999999999999", "18446744073709551615"] {
            assert_eq!(parse_duration(bad), None, "{bad}");
        }
    }

    #[test]
    fn parses_spoken_durations() {
        let m = |s: u64| Some(Duration::from_secs(s));
        assert_eq!(parse_spoken("10m"), m(600));
        assert_eq!(parse_spoken("1h30m"), m(5400));
        assert_eq!(parse_spoken("90s"), m(90));
        assert_eq!(parse_spoken("25"), m(25 * 60));
        assert_eq!(parse_spoken("25:30"), m(25 * 60 + 30));
        assert_eq!(parse_spoken("10 min"), m(600));
        assert_eq!(parse_spoken("1 hour 30 minutes"), m(5400));
        for bad in ["", "m10", "5x", "200m", "10m5", "soon", "h"] {
            assert_eq!(parse_spoken(bad), None, "{bad}");
        }
    }

    #[test]
    fn start_replaces_a_running_countdown() {
        let t0 = Instant::now();
        let mut f = Focus::new(25);
        f.toggle(t0);
        f.start(Duration::from_secs(600), t0);
        assert!(f.running());
        assert_eq!(f.total(), Duration::from_secs(600));
        assert_eq!(f.remaining(t0), Duration::from_secs(600));
    }

    #[test]
    fn adjust_stays_within_limits_and_locks_while_running() {
        let mut f = Focus::new(25);
        f.adjust(300);
        assert_eq!(f.total(), Duration::from_secs(30 * 60));
        f.adjust(-100_000);
        assert_eq!(f.total(), Duration::from_secs(1));
        f.adjust(1_000_000);
        assert_eq!(f.total(), Duration::from_secs(180 * 60));
        f.set_duration(Duration::from_secs(600));
        f.toggle(Instant::now());
        f.adjust(60);
        assert_eq!(f.total(), Duration::from_secs(600));
    }

    #[test]
    fn length_cannot_change_while_running() {
        let mut f = Focus::new(25);
        f.toggle(Instant::now());
        f.set_minutes(45);
        assert_eq!(f.total(), Duration::from_secs(25 * 60));
        f.reset();
        f.set_minutes(45);
        assert_eq!(f.total(), Duration::from_secs(45 * 60));
    }
}
