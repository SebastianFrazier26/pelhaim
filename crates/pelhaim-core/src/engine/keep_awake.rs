use super::{Effect, Engine, KeepAwakeAction};
use crate::event::Ts;
use crate::model::{KeepAwakeStatus, TrainState};

/// §8.6: the delay avoids flapping between steps.
const RELEASE_AFTER_SECS: i64 = 60;

#[derive(Default)]
pub(super) struct KeepAwake {
    held: bool,
    zero_since: Option<Ts>,
    paused_until: Option<Ts>,
    mechanism: Option<String>,
    on_battery: Option<bool>,
}

impl KeepAwake {
    pub(super) fn set_env(&mut self, mechanism: Option<String>, on_battery: Option<bool>) {
        self.mechanism = mechanism;
        self.on_battery = on_battery;
    }
}

impl Engine {
    fn awake_count(&self) -> u32 {
        let holds = |s: &TrainState| match s {
            TrainState::Working => self.cfg.include_observed,
            TrainState::Waiting(_) | TrainState::ProbablyWaiting => self.cfg.remote_enabled,
            _ => false,
        };
        self.trains.values().filter(|t| holds(&t.state)).count() as u32
    }

    /// Runs after every input. Holds as soon as the count rises; releases and
    /// resumes after a pause only on a Tick.
    pub(super) fn keep_awake_step(&mut self, clock: Ts, tick: bool) -> bool {
        let count = self.awake_count();
        let enabled = self.cfg.keep_awake_enabled;
        let k = &mut self.keep;
        let mut changed = false;
        if tick && k.paused_until.is_some_and(|u| clock >= u) {
            k.paused_until = None;
            changed = true;
        }
        if count > 0 {
            k.zero_since = None;
            if enabled && k.paused_until.is_none() && !k.held {
                k.held = true;
                self.effects.push(Effect::KeepAwake(KeepAwakeAction::Hold));
                changed = true;
            }
        } else if k.held {
            let since = *k.zero_since.get_or_insert(clock);
            if tick && clock - since >= chrono::TimeDelta::seconds(RELEASE_AFTER_SECS) {
                k.held = false;
                k.zero_since = None;
                self.effects
                    .push(Effect::KeepAwake(KeepAwakeAction::Release));
                changed = true;
            }
        }
        changed
    }

    pub(super) fn pause_keep_awake(&mut self, until: Ts) {
        let k = &mut self.keep;
        k.paused_until = Some(until);
        k.zero_since = None;
        if k.held {
            k.held = false;
            self.effects
                .push(Effect::KeepAwake(KeepAwakeAction::Release));
        }
    }

    pub(super) fn keep_awake_status(&self) -> KeepAwakeStatus {
        KeepAwakeStatus {
            enabled: self.cfg.keep_awake_enabled,
            held: self.keep.held,
            count: self.awake_count(),
            paused_until: self.keep.paused_until,
            mechanism: self.keep.mechanism.clone(),
            on_battery: self.keep.on_battery,
        }
    }
}
