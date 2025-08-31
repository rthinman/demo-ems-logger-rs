//! Temperature alarms and logging.

use crate::constants::{ALARM_HIGH_TEMPERATURE, ALARM_TEMP_HYSTERESIS, ALARM_LOW_TEMPERATURE};
use crate::{logger::{AlarmTimerExpired, AlarmTimerTrigger, TemperatureSample}};

// For the temperature alarm state machine.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TemperatureState {
    Safe,       // Temperature is within the safe range (but still could be between -0.5 and 2, which is not optimal).
    HotNoAlarm, // Temperature is > +8°C, but not for long enough to alarm.
    HotAlarm,   // Temperature has been > +8°C long enoug to alarm.
    FreezeNoAlarm, // Temperature is < -0.5°C, but not long enough to alarm.
    FreezeAlarm,   // Temperature has been < -0.5°C for long enough to alarm.
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Temperatures {
    pub vaccine: Option<f32>, // TODO: do we need to store these temperatures?
    pub ambient: Option<f32>,
    status: TemperatureState,
    new_high_alarm: bool, // Tracks a new alarm that starts between temperature samples.
    new_low_alarm: bool,  // Tracks a new alarm that starts between temperature samples.
}

impl Temperatures {
    pub fn new() -> Self {
        Self {
            vaccine: None,
            ambient: None,
            status: TemperatureState::Safe,
            new_high_alarm: false,
            new_low_alarm: false,
        }
    }

    /// Process a new temperature sample and update the alarm state.
    /// Returns an AlarmTrigger to signal whether to start or cancel alarm timing.
    pub fn new_temperatures(&mut self, sample: TemperatureSample) -> AlarmTimerTrigger {
        // Update state based on new vaccine temperature
        let trigger = if let Some(current_vaccine) = sample.vaccine {
            match self.status {
                TemperatureState::HotAlarm | TemperatureState::HotNoAlarm => {
                    if current_vaccine <= ALARM_LOW_TEMPERATURE { 
                        self.status = TemperatureState::FreezeNoAlarm;
                        AlarmTimerTrigger::LowTemperatureStart
                    } else if current_vaccine < ALARM_HIGH_TEMPERATURE - ALARM_TEMP_HYSTERESIS {
                        self.status = TemperatureState::Safe;
                        AlarmTimerTrigger::TemperatureCancel
                    } else {
                        // Stay in hot state, alarm will continue, or be triggered by timer.
                        AlarmTimerTrigger::NoTrigger
                    }
                },
                TemperatureState::FreezeAlarm | TemperatureState::FreezeNoAlarm => {
                    if current_vaccine >= ALARM_HIGH_TEMPERATURE {
                        self.status = TemperatureState::HotNoAlarm;
                        AlarmTimerTrigger::HighTemperatureStart
                    } else if current_vaccine > ALARM_LOW_TEMPERATURE + ALARM_TEMP_HYSTERESIS {
                        self.status = TemperatureState::Safe;
                        AlarmTimerTrigger::TemperatureCancel
                    } else {
                        // Stay in freeze state, alarm will continue, or be triggered by timer.
                        AlarmTimerTrigger::NoTrigger
                    }
                },
                TemperatureState::Safe => {
                    if current_vaccine >= ALARM_HIGH_TEMPERATURE {
                        self.status = TemperatureState::HotNoAlarm;
                        AlarmTimerTrigger::HighTemperatureStart
                    } else if current_vaccine <= ALARM_LOW_TEMPERATURE {
                        self.status = TemperatureState::FreezeNoAlarm;
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

        trigger // TODO: if we don't add more code, move the save to top of function just return the value of the match.
    }

    pub fn alarm_expired(&mut self, state: AlarmTimerExpired) {
        match state {
            AlarmTimerExpired::HighTemperature => {
                self.status = TemperatureState::HotAlarm;
                self.new_high_alarm = true;
            },
            AlarmTimerExpired::LowTemperature => {
                self.status = TemperatureState::FreezeAlarm;
                self.new_low_alarm = true;
            },
            _ => {
                // Only HighTemperatureStart and LowTemperatureStart should trigger a change.
            }
        }
    }

    /// Returns true if a high temperature alarm is active or a new alarm was
    /// triggered but then resolved.
    pub fn is_high_alarm(&self) -> bool {
        self.status == TemperatureState::HotAlarm || self.new_high_alarm
    }

    /// Returns true if a low temperature alarm is active or a new alarm was
    /// triggered but then resolved.
    pub fn is_low_alarm(&self) -> bool {
        self.status == TemperatureState::FreezeAlarm || self.new_low_alarm
    }

    /// Clear the new alarm flags after they have been logged.
    pub fn clear_new_alarms(&mut self) {
        self.new_high_alarm = false;
        self.new_low_alarm = false;
    }
}
