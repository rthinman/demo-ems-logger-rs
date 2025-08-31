//! Temperature alarms and logging.

use crate::constants::{ALARM_HIGH_SECONDS, ALARM_HIGH_TEMPERATURE, ALARM_LOW_SECONDS, ALARM_LOW_TEMPERATURE, MAX_GOOD_VACCINE_TEMP, MIN_GOOD_VACCINE_TEMP};
use crate::{logger::{AlarmTimerExpired, AlarmTimerTrigger, LoggerEvent, TemperatureSample}, timestamp::{Timestamp, TimestampError}};

// For the temperature alarm state machine.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TemperatureState {
    Safe,
    HotNoAlarm(Timestamp), // Timestamp when temperature went > +8°C.
    HotAlarm(Timestamp),   // Timestamp when temperature went > +8°C.
    FreezeNoAlarm(Timestamp), // Timestamp when temperature went < -0.5°C.
    FreezeAlarm(Timestamp),   // Timestamp when temperature went < -0.5°C.
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Temperatures {
    pub vaccine: Option<f32>,
    pub ambient: Option<f32>,
    status: TemperatureState,
    //  maybe alarms in progress, though should be tracked by state.
    // new alarms this sample
}

impl Temperatures {
    pub fn new() -> Self {
        Self {
            vaccine: None,
            ambient: None,
            status: TemperatureState::Safe,
        }
    }

    /// Process a new temperature sample and update the alarm state.
    /// Returns an AlarmTrigger to signal whether to start or cancel alarm timing.
    // TODO: do we need the timestamp?
    // TODO: check that comparisons handle equals case correctly, and add hysteresis.
    pub fn new_temperatures(&mut self, sample: TemperatureSample, now: Timestamp) -> Result<AlarmTimerTrigger, TimestampError> {
        // Update state based on new vaccine temperature
        let trigger = if let Some(current_vaccine) = sample.vaccine {
            match self.status {
                TemperatureState::HotAlarm(_) | TemperatureState::HotNoAlarm(_) => {
                    if current_vaccine < ALARM_LOW_TEMPERATURE { 
                        self.status = TemperatureState::FreezeNoAlarm(now);
                        AlarmTimerTrigger::LowTemperatureStart
                    } else if current_vaccine < MAX_GOOD_VACCINE_TEMP {
                        self.status = TemperatureState::Safe;
                        AlarmTimerTrigger::TemperatureCancel
                    } else {
                        // Stay in hot state, alarm will continue, or be triggered by timer.
                        AlarmTimerTrigger::NoTrigger
                    }
                },
                TemperatureState::FreezeAlarm(_) | TemperatureState::FreezeNoAlarm(_) => {
                    if current_vaccine > MAX_GOOD_VACCINE_TEMP {
                        self.status = TemperatureState::HotNoAlarm(now);
                        AlarmTimerTrigger::HighTemperatureStart
                    } else if current_vaccine > ALARM_LOW_TEMPERATURE {
                        self.status = TemperatureState::Safe;
                        AlarmTimerTrigger::TemperatureCancel
                    } else {
                        // Stay in freeze state, alarm will continue, or be triggered by timer.
                        AlarmTimerTrigger::NoTrigger
                    }
                },
                TemperatureState::Safe => {
                    if current_vaccine > MAX_GOOD_VACCINE_TEMP {
                        self.status = TemperatureState::HotNoAlarm(now);
                        AlarmTimerTrigger::HighTemperatureStart
                    } else if current_vaccine < ALARM_LOW_TEMPERATURE {
                        self.status = TemperatureState::FreezeNoAlarm(now);
                        AlarmTimerTrigger::LowTemperatureStart
                    } else {
                            self.status = TemperatureState::Safe;
                            AlarmTimerTrigger::NoTrigger
                    }
                }
            }
        } else {
                // No vaccine temperature available, cannot change state.
                AlarmTimerTrigger::NoTrigger
        };

        // Save the latest temperatures.
        self.vaccine = sample.vaccine;
        self.ambient = sample.ambient;

        Ok(trigger)
    }

    pub fn alarm_expired(&mut self, state: AlarmTimerExpired) {
        match state {
            AlarmTimerExpired::HighTemperature => {
                if let TemperatureState::HotNoAlarm(hot_ts) = self.status {
                    self.status = TemperatureState::HotAlarm(hot_ts);
                }
            },
            AlarmTimerExpired::LowTemperature => {
                if let TemperatureState::FreezeNoAlarm(cold_ts) = self.status {
                    self.status = TemperatureState::FreezeAlarm(cold_ts);
                }
            },
            _ => {
                // Only HighTemperatureStart and LowTemperatureStart should trigger this.
            }
        }

    }
}
