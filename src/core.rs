use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub random: bool,
    pub min_minutes: u32,
    pub max_minutes: u32,
    pub fixed_minutes: u32,
    pub serious: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            random: true,
            min_minutes: 20,
            max_minutes: 30,
            fixed_minutes: 25,
            serious: false,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(1..=1440).contains(&self.min_minutes)
            || !(1..=1440).contains(&self.max_minutes)
            || !(1..=1440).contains(&self.fixed_minutes)
        {
            return Err("Choose whole minutes between 1 and 1440.");
        }
        if self.min_minutes > self.max_minutes {
            return Err("The earliest reminder must not be later than the latest.");
        }
        Ok(())
    }

    pub fn interval_seconds(&self) -> i64 {
        let minutes = if self.random {
            rand::random_range(self.min_minutes..=self.max_minutes)
        } else {
            self.fixed_minutes
        };
        i64::from(minutes) * 60
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Prompt {
    #[default]
    Initial,
    Finished,
    Canceled,
}

impl Prompt {
    pub fn text(self) -> &'static str {
        match self {
            Self::Initial => "What are you doing now?",
            Self::Finished => "What are you going to do now?",
            Self::Canceled => "What are you going to do instead?",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Phase {
    Entry(Prompt),
    Waiting { due: i64 },
    Pending,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub version: u32,
    pub goal: Option<String>,
    pub draft: String,
    pub settings: Settings,
    pub phase: Phase,
    pub position: Option<(i32, i32)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            goal: None,
            draft: String::new(),
            settings: Settings::default(),
            phase: Phase::Entry(Prompt::Initial),
            position: None,
        }
    }
}

impl State {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1 {
            return Err("Unsupported state version.");
        }
        self.settings.validate()?;
        if self
            .goal
            .as_ref()
            .is_some_and(|g| g.trim().is_empty() || g.chars().count() > 500)
            || self.draft.chars().count() > 500
        {
            return Err("Invalid saved goal.");
        }
        if matches!(self.phase, Phase::Entry(_)) != self.goal.is_none() {
            return Err("Inconsistent saved reminder.");
        }
        Ok(())
    }

    pub fn start(&mut self, goal: &str, now: i64, interval: i64) -> Result<(), &'static str> {
        let goal = goal.trim();
        if goal.is_empty() {
            return Err("Write one thing you want to focus on.");
        }
        if goal.chars().count() > 500 {
            return Err("Keep your goal under 500 characters.");
        }
        self.goal = Some(goal.to_owned());
        self.draft.clear();
        self.reset(now, interval);
        Ok(())
    }

    pub fn reset(&mut self, now: i64, interval: i64) {
        if self.goal.is_some() {
            self.phase = Phase::Waiting {
                due: now.saturating_add(interval.max(1)),
            };
        }
    }

    pub fn complete(&mut self, canceled: bool) {
        self.goal = None;
        self.draft.clear();
        self.phase = Phase::Entry(if canceled {
            Prompt::Canceled
        } else {
            Prompt::Finished
        });
    }

    pub fn due(&mut self, now: i64) -> bool {
        if matches!(self.phase, Phase::Waiting { due } if now >= due) {
            self.phase = Phase::Pending;
            true
        } else {
            false
        }
    }

    pub fn restore(&mut self) {
        if self.goal.is_some() {
            self.phase = Phase::Pending;
        }
    }

    /// Use both clocks: a backwards wall-clock adjustment must not postpone a goal forever.
    /// Suspension/forward jumps are picked up by the persisted wall deadline instead.
    pub fn reconcile_clock(&mut self, previous_wall: i64, wall: i64, elapsed: i64) {
        if let Phase::Waiting { ref mut due } = self.phase {
            let expected = previous_wall.saturating_add(elapsed);
            if wall < expected.saturating_sub(2) {
                *due = due.saturating_sub(expected.saturating_sub(wall));
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Desktop {
    pub locked: bool,
    pub fullscreen: bool,
    pub reliable_fullscreen: bool,
    pub description: String,
}

pub fn may_remind(settings: &Settings, desktop: &Desktop) -> bool {
    !desktop.locked && (settings.serious || !desktop.fullscreen)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_and_distinct_next_prompts() {
        let mut s = State::default();
        assert!(s.start("  ", 0, 10).is_err());
        s.start("  Write chapter  ", 100, 10).unwrap();
        assert_eq!(s.goal.as_deref(), Some("Write chapter"));
        assert!(!s.due(109));
        assert!(s.due(110));
        assert!(!s.due(999));
        s.reset(999, 15);
        assert_eq!(s.phase, Phase::Waiting { due: 1014 });
        s.complete(false);
        assert_eq!(s.phase, Phase::Entry(Prompt::Finished));
        assert!(!s.due(i64::MAX));
        s.start("Next", 0, 1).unwrap();
        s.complete(true);
        assert_eq!(s.phase, Phase::Entry(Prompt::Canceled));
    }
    #[test]
    fn random_bounds_and_fixed_interval() {
        let mut settings = Settings::default();
        for _ in 0..1000 {
            assert!((1200..=1800).contains(&settings.interval_seconds()));
        }
        settings.random = false;
        settings.fixed_minutes = 7;
        assert_eq!(settings.interval_seconds(), 420);
        settings.fixed_minutes = 0;
        assert!(settings.validate().is_err());
        settings.fixed_minutes = 5;
        settings.min_minutes = 40;
        assert!(settings.validate().is_err());
    }
    #[test]
    fn modes_lock_and_pending_are_independent() {
        let mut s = State::default();
        s.start("Work", 0, 10).unwrap();
        s.due(10000);
        let d = Desktop {
            locked: false,
            fullscreen: true,
            ..Default::default()
        };
        assert!(!may_remind(&s.settings, &d));
        assert_eq!(s.phase, Phase::Pending);
        s.settings.serious = true;
        assert!(may_remind(&s.settings, &d));
        assert!(!may_remind(&s.settings, &Desktop { locked: true, ..d }));
    }
    #[test]
    fn restore_and_clock_changes_cannot_lose_reminder() {
        let mut s = State::default();
        s.start("Work", 1000, 60).unwrap();
        s.reconcile_clock(1000, 500, 10);
        assert_eq!(s.phase, Phase::Waiting { due: 550 });
        assert!(s.due(550));
        s.reset(0, 100);
        s.restore();
        assert_eq!(s.phase, Phase::Pending);
        s.complete(false);
        s.restore();
        assert_eq!(s.phase, Phase::Entry(Prompt::Finished));
    }
    #[test]
    fn twenty_four_hour_accelerated_soak() {
        let mut s = State::default();
        s.start("One goal", 0, 60).unwrap();
        let mut count = 0;
        for now in 0..86400 {
            if s.due(now) {
                count += 1;
                s.reset(now, 60);
            }
            s.validate().unwrap();
        }
        assert_eq!(count, 1439);
    }
}
