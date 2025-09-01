//! This module contains the business logic for handling power availability events, including
//! tracking power on/off events, accumulating available durations, and managing alarms.

use crate::logger::{AlarmTimerExpired, AlarmTimerTrigger};
use crate::timestamp::Timestamp;


// TODO: rework to have functions that get and reset accumulators all at once?

/// Represents a power event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerEvent {
    Off,
    On,
}

// TODO: add a power state enum.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerAvailability {
    status: PowerEvent, // TODO: change to new power state.
    power_on_accum: u32, // Accumulated time power is on.
    new_alarm: bool, // Flag indicating if the short sample alarm was triggered.
}

impl PowerAvailability {
    /// Create a new PowerAvailability instance with the current power state.
    pub fn new(power_on: bool, now: Timestamp) -> Self {
        // Initialize the "sample_ended" trackers with the last instance before now.
        let sample_ts = now.get_last_short_sample_end();
        let prev_long_sample_ended = now.get_last_long_sample_end();
        // Assume the power was turned on now.
        let status = if power_on {
            PowerEvent::On
        } else {
            PowerEvent::Off
        };

        Self {
            status,
            power_on_accum: 0,
            new_alarm: false,
        }
    }

    pub fn power_event(&mut self, event: PowerEvent) -> AlarmTimerTrigger {
        // TODO: implement me.
        AlarmTimerTrigger::NoTrigger
    }

    pub fn alarm_expired(&mut self, event: AlarmTimerExpired) {
        // TODO: implement me.
    }

    pub fn is_alarm(&self) -> bool {
        // TODO: implement me.
        false
    }

    pub fn clear_new_alarms(&mut self) {
        // TODO: implement me.  Also, do we need this method since we have reset_accumulators?
    }

    pub fn get_power_on_accum(&self) -> u32 {
        // TODO: implement me.
        0
    }

    pub fn reset_accumulators(&mut self) {
        // TODO: implement me.
    }

    
}

#[cfg(test)]
mod tests {
    use super::*;

}
