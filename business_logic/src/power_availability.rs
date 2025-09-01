//! This module contains the business logic for handling power availability events, including
//! tracking power on/off events, accumulating available durations, and managing alarms.

use crate::constants::POWER_ALARM_THRESHOLD;
use crate::timestamp::{Timestamp, TimestampError};


// TODO: rework to have functions that get and reset accumulators all at once?

/// Represents a power event with a timestamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerEvent {
    On(Timestamp), // Timestamp when power turned on.
    Off(Timestamp), // Timestamp when power was removed.
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerAvailability {
    status: PowerEvent, // Including the Timestamp when power turned on or off.
    last_event_ts: Timestamp, // Timestamp of the last event or reset.
    prev_short_sample_ended: Timestamp, // Timestamp the previous short sampling period ended. (ideally, 0, 900, 1800...)
    prev_long_sample_ended: Timestamp, // Timestamp the previous long sampling period ended.
    short_power_accum: u32, // Accumulated time for short sampling.
    long_power_accum: u32, // Accumulated time for long sampling.
    short_alarmed: bool, // Flag indicating if the short sample alarm was triggered.
    long_alarmed: bool, // Flag indicating if the long sample alarm was triggered.
}

impl Default for PowerAvailability {
    fn default() -> Self {
        let now = Timestamp { seconds: 0 };
        Self {
            status: PowerEvent::Off(now),
            last_event_ts: now,
            prev_short_sample_ended: now,
            prev_long_sample_ended: now,
            short_power_accum: 0,
            long_power_accum: 0,
            short_alarmed: false,
            long_alarmed: false,
        }
    }
}

impl PowerAvailability {
    /// Create a new PowerAvailability instance with the current power state.
    pub fn new(on: bool, now: Timestamp) -> Self {
        // Initialize the "sample_ended" trackers with the last instance before now.
        let sample_ts = now.get_last_short_sample_end();
        let prev_long_sample_ended = now.get_last_long_sample_end();
        // Assume the power was turned on now.
        let status = if on {
            PowerEvent::On(now)
        } else {
            PowerEvent::Off(now)
        };

        Self {
            status,
            last_event_ts: now,
            prev_short_sample_ended: sample_ts,
            prev_long_sample_ended: prev_long_sample_ended,
            short_power_accum: 0,
            long_power_accum: 0,
            short_alarmed: false,
            long_alarmed: false,
        }
    }

    pub fn log_event(&mut self, event: PowerEvent) -> Result<(), TimestampError> {
        // Validate the event timestamp.
        let timestamp = match event {
            PowerEvent::On(ts) => ts,
            PowerEvent::Off(ts) => ts,
        };
        self.validate_timestamp_and_update(&timestamp)?;
        
        // TODO: Handle the case where the power is turned on while it is already on, and vice versa.
        match event {
            PowerEvent::On(timestamp) => {
                if let PowerEvent::Off(off) = self.status {
                    let duration = timestamp.seconds - off.seconds;
                    // Check if the alarms should be triggered (power has been off for just over 24 hours).
                    self.short_alarmed = duration >= POWER_ALARM_THRESHOLD;
                    self.long_alarmed = duration >= POWER_ALARM_THRESHOLD;
                }
            }
            PowerEvent::Off(timestamp) => {
                if let PowerEvent::On(on) = self.status {
                    let duration = timestamp.seconds - on.seconds;
                    // Durations are accumulated after the power turns off.
                    self.short_power_accum += duration;
                    self.long_power_accum += duration;
                }
            }
        }
        self.status = event;
        Ok(())
    }

    pub fn get_short_sample(&self, check_time: Timestamp) -> u32 {
        // TODO: Consider better error handling for check_time earlier than on.
        let presently_on_seconds = if let PowerEvent::On(on) = self.status {
            if check_time.seconds < on.seconds {
                0 // If check_time is before power turned on, return 0.
            } else {
                check_time.seconds - on.seconds
            }
        } else {
            0
        };

        self.short_power_accum + presently_on_seconds
    }

    pub fn get_long_sample(&self, check_time: Timestamp) -> u32 {
        // TODO: Consider better error handling for check_time earlier than on.
        let presently_on_seconds = if let PowerEvent::On(on) = self.status {
            if check_time.seconds < on.seconds {
                0 // If check_time is before power turened on, return 0.
            } else {
                check_time.seconds - on.seconds
            }
        } else {
            0
        };

        self.long_power_accum + presently_on_seconds
    }       

    pub fn reset_short_sample(&mut self, timestamp: Timestamp) -> Result<(), TimestampError> {
        self.validate_timestamp_and_update(&timestamp)?;
        self.prev_short_sample_ended = timestamp;
        self.short_power_accum = 0;
        self.short_alarmed = false;
        Ok(())
    }

    pub fn reset_long_sample(&mut self, timestamp: Timestamp) -> Result<(), TimestampError> {
        self.validate_timestamp_and_update(&timestamp)?;
        self.prev_long_sample_ended = timestamp;
        self.long_power_accum = 0;
        self.long_alarmed = false;
        Ok(())
    }

    pub fn is_short_sample_alarmed(&self, now: Timestamp) -> bool {
        // If the power is currently on, check if it has been on longer than the threshold.
        let off_long = if let PowerEvent::Off(off) = self.status {
            now.seconds > off.seconds + POWER_ALARM_THRESHOLD
        } else {
            false
        };

        // True if there was an alarm that was canceled
        self.short_alarmed || off_long
    }

    pub fn is_long_sample_alarmed(&self, now: Timestamp) -> bool {
        // If the power is currently on, check if it has been on longer than the threshold.
        let off_long = if let PowerEvent::Off(off) = self.status {
            now.seconds > off.seconds + POWER_ALARM_THRESHOLD
        } else {
            false
        };

        // True if there was an alarm that was canceled
        self.long_alarmed || off_long
    }

    fn validate_timestamp_and_update(&mut self, timestamp: &Timestamp) -> Result<(), TimestampError> {
        if timestamp.seconds < self.last_event_ts.seconds {
            return Err(TimestampError::OutOfOrder);
        }
        self.last_event_ts = *timestamp;
        Ok(())
    }
    
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_event_logging() {
        let mut power = PowerAvailability::default();
        let on_time = Timestamp { seconds: 1000 };
        let off_time = Timestamp { seconds: 1200 };

        power.log_event(PowerEvent::On(on_time)).unwrap();
        assert_eq!(power.status, PowerEvent::On(on_time));

        power.log_event(PowerEvent::Off(off_time)).unwrap();
        assert_eq!(power.short_power_accum, 200);
        assert_eq!(power.long_power_accum, 200);

        // Out-of-order event: try to power on at a time before the last event
        let out_of_order_time = Timestamp { seconds: 100 };
        let result = power.log_event(PowerEvent::On(out_of_order_time));
        assert!(matches!(result, Err(TimestampError::OutOfOrder)));

        // On and off again.
        let on_time = Timestamp { seconds: 1500 };
        let off_time = Timestamp { seconds: 1800 };

        power.log_event(PowerEvent::On(on_time)).unwrap();
        assert_eq!(power.status, PowerEvent::On(on_time));

        power.log_event(PowerEvent::Off(off_time)).unwrap();
        assert_eq!(power.short_power_accum, 500);
        assert_eq!(power.long_power_accum, 500);

    }

    #[test]
    fn test_power_alarm_threshold() {
        let mut power = PowerAvailability::default();
        let on_time = Timestamp { seconds: 1000 };
        let off_time = Timestamp { seconds: 1200 };

        power.log_event(PowerEvent::On(on_time)).unwrap();
        assert!(!power.is_short_sample_alarmed(off_time));
        assert!(!power.is_long_sample_alarmed(off_time));

        // Turn off, no alarm yet..
        power.log_event(PowerEvent::Off(off_time)).unwrap();
        assert!(!power.is_short_sample_alarmed(off_time));
        assert!(!power.is_long_sample_alarmed(off_time));

        // Now, let's exceed the threshold. Keep off for over 24 hours
        let check_time = Timestamp { seconds: 1200  + 86400};
        let check_time2 = Timestamp { seconds: 1202  + 86400};

        // Exactly at the threshold, should not alarm.
        assert!(!power.is_short_sample_alarmed(check_time));
        assert!(!power.is_long_sample_alarmed(check_time));

        // Should alarm now.
        assert!(power.is_short_sample_alarmed(check_time2));
        assert!(power.is_long_sample_alarmed(check_time2));
    }

    #[test]
    fn test_on_off_reset() {
        let mut power = PowerAvailability::default();
        let on_time = Timestamp { seconds: 450 };
        let off_time = Timestamp { seconds: 600 };
        let check_time = Timestamp { seconds: 900 };

        // Power on and off
        power.log_event(PowerEvent::On(on_time)).unwrap();
        power.log_event(PowerEvent::Off(off_time)).unwrap();

        // Get short sample values at 900 seconds
        let short_accum = power.get_short_sample(check_time);
        assert_eq!(short_accum, 150);
        assert!(!power.is_short_sample_alarmed(check_time));

        // Get long sample values at 900 seconds
        let long_accum = power.get_long_sample(check_time);
        assert_eq!(long_accum, 150);
        assert!(!power.is_long_sample_alarmed(check_time));

        // Reset short sample at 900 seconds, should not affect long sample.
        power.reset_short_sample(check_time).unwrap();
        assert_eq!(power.short_power_accum, 0);
        assert_eq!(power.prev_short_sample_ended, check_time);
        assert_eq!(power.long_power_accum, 150);
        assert_eq!(power.prev_long_sample_ended, Timestamp { seconds: 0 });

        // Reset long sample at 900 seconds
        power.reset_long_sample(check_time).unwrap();
        assert_eq!(power.long_power_accum, 0);
        assert_eq!(power.prev_long_sample_ended, check_time);
    }

    #[test]
    fn test_init_on_off_reset() {
        let init_time = Timestamp { seconds: 901 };
        let mut power = PowerAvailability::new(false, init_time);
        let on_time = Timestamp { seconds: 1300 };
        let check_while_on = Timestamp { seconds: 1400 };
        let off_time = Timestamp { seconds: 1700 };
        let check_time = Timestamp { seconds: 1800 };

        // Ensure the initial state is correct
        assert_eq!(power.prev_short_sample_ended.seconds, 900);
        assert_eq!(power.prev_long_sample_ended.seconds, 0);

        // Power on
        power.log_event(PowerEvent::On(on_time)).unwrap();
        let short_accum = power.get_short_sample(check_while_on);
        assert_eq!(short_accum, 100); // 1400 - 1300 = 100 seconds while on
        assert!(!power.is_short_sample_alarmed(check_while_on));

        // Long sample checks while on
        let long_accum = power.get_long_sample(check_while_on);
        assert_eq!(long_accum, 100);
        assert!(!power.is_long_sample_alarmed(check_while_on));

        // Power off
        power.log_event(PowerEvent::Off(off_time)).unwrap();

        // Get short sample values at 1800 seconds
        let short_accum = power.get_short_sample(check_time);
        assert_eq!(short_accum, 400);
        assert!(!power.is_short_sample_alarmed(check_time));

        // Long sample checks after power off
        let long_accum = power.get_long_sample(check_time);
        assert_eq!(long_accum, 400);
        assert!(!power.is_long_sample_alarmed(check_time));

        // Reset short sample at 1800 seconds, which should not affect long sample.
        power.reset_short_sample(check_time).unwrap();
        assert_eq!(power.short_power_accum, 0);
        assert_eq!(power.prev_short_sample_ended, check_time);
        assert_eq!(long_accum, 400);

        // Reset long sample at 1800 seconds
        power.reset_long_sample(check_time).unwrap();
        assert_eq!(power.long_power_accum, 0);
        assert_eq!(power.prev_long_sample_ended, check_time);
    }

    #[test]
    fn test_init_while_on() {
        // Test the case where the power is initialized while on
        let init_time = Timestamp { seconds: 901 };
        let mut power = PowerAvailability::new(true, init_time);
        let check_while_on = Timestamp { seconds: 1400 };
        let off_time = Timestamp { seconds: 1700 };
        let check_time = Timestamp { seconds: 1800 };

        // Ensure the initial state is correct
        assert_eq!(power.prev_short_sample_ended.seconds, 900);
        assert_eq!(power.prev_long_sample_ended.seconds, 0);

        // Get short sample values at 1400 seconds
        let short_accum = power.get_short_sample(check_while_on);
        assert_eq!(short_accum, 499);
        assert!(!power.is_short_sample_alarmed(check_time));

        // Long sample checks while on
        let long_accum = power.get_long_sample(check_while_on);
        assert_eq!(long_accum, 499);
        assert!(!power.is_long_sample_alarmed(check_time));

        // Turn off, then check at 1800 seconds when the power has been on for 799 sec.
        power.log_event(PowerEvent::Off(off_time)).unwrap();
        let short_accum = power.get_short_sample(check_time);
        assert_eq!(short_accum, 799);
        assert!(!power.is_short_sample_alarmed(check_while_on));

        // Long sample checks after off
        let long_accum = power.get_long_sample(check_time);
        assert_eq!(long_accum, 799);
        assert!(!power.is_long_sample_alarmed(check_while_on));
    }

    #[test]
    fn test_out_of_order_reset() {
        let mut power = PowerAvailability::default();
        let on_time = Timestamp { seconds: 700 };
        let off_time = Timestamp { seconds: 800 };
        let reset_time = Timestamp { seconds: 900 };

        // Power on
        power.log_event(PowerEvent::On(on_time)).unwrap();

        // Try to reset with an out-of-order timestamp
        let out_of_order_reset = Timestamp { seconds: 600 };
        assert!(matches!(power.reset_short_sample(out_of_order_reset), Err(TimestampError::OutOfOrder)));

        // Power off
        power.log_event(PowerEvent::Off(off_time)).unwrap();

        // Reset short sample with a valid timestamp
        assert!(power.reset_short_sample(reset_time).is_ok());
        assert_eq!(power.prev_short_sample_ended, reset_time);
        
        // Try to reset with an out-of-order timestamp
        assert!(matches!(power.reset_short_sample(out_of_order_reset), Err(TimestampError::OutOfOrder)));
    }

    #[test]
    fn test_multiple_brief_events() {
        let mut power = PowerAvailability::default();
        let on_time1 = Timestamp { seconds: 1000 };
        let off_time1 = Timestamp { seconds: 1150 };
        let on_time2 = Timestamp { seconds: 1200 };
        let check_time2 = Timestamp { seconds: 1300 };
        let off_time2 = Timestamp { seconds: 1450 };

        // Turn on power for the first time.
        power.log_event(PowerEvent::On(on_time1)).unwrap();
        power.log_event(PowerEvent::Off(off_time1)).unwrap();
        assert!(!power.is_short_sample_alarmed(off_time1));
        assert!(!power.is_long_sample_alarmed(off_time1)); // 100 sec, no problem.

        // Turn on power for the second time.
        power.log_event(PowerEvent::On(on_time2)).unwrap();
        assert!(!power.is_short_sample_alarmed(check_time2));
        assert!(!power.is_long_sample_alarmed(check_time2)); // Also 100 sec.
        power.log_event(PowerEvent::Off(off_time2)).unwrap(); // Off after 250 seconds, should not alarm
        assert!(!power.is_short_sample_alarmed(off_time2));
        assert!(!power.is_long_sample_alarmed(off_time2));
    }       
}
