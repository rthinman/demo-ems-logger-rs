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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::TemperatureSample;

    #[test]
    fn test_initial_state() {
        let temps = Temperatures::new();
        assert_eq!(temps.status, TemperatureState::Safe);
        assert!(!temps.is_high_alarm());
        assert!(!temps.is_low_alarm());
    }

    #[test]
    fn test_safe_to_hot_transition() {
        let mut temps = Temperatures::new();
        let sample = TemperatureSample { vaccine: Some(8.5), ambient: None };
        
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::HighTemperatureStart);
        assert_eq!(temps.status, TemperatureState::HotNoAlarm);
    }

    #[test]
    fn test_safe_to_freeze_transition() {
        let mut temps = Temperatures::new();
        let sample = TemperatureSample { vaccine: Some(-1.0), ambient: None };
        
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::LowTemperatureStart);
        assert_eq!(temps.status, TemperatureState::FreezeNoAlarm);
    }

    #[test]
    fn test_hot_to_safe_with_hysteresis() {
        let mut temps = Temperatures::new();
        temps.status = TemperatureState::HotNoAlarm;
        
        // Temperature drops below high threshold minus hysteresis
        let sample = TemperatureSample { vaccine: Some(7.8), ambient: None };
        
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::TemperatureCancel);
        assert_eq!(temps.status, TemperatureState::Safe);
    }

    #[test]
    fn test_freeze_to_safe_with_hysteresis() {
        let mut temps = Temperatures::new();
        temps.status = TemperatureState::FreezeNoAlarm;
        
        // Temperature rises above low threshold plus hysteresis
        let sample = TemperatureSample { vaccine: Some(-0.3), ambient: None };
        
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::TemperatureCancel);
        assert_eq!(temps.status, TemperatureState::Safe);
    }

    #[test]
    fn test_hot_to_freeze_direct_transition() {
        let mut temps = Temperatures::new();
        temps.status = TemperatureState::HotNoAlarm;
        
        // Temperature drops directly to freeze level
        let sample = TemperatureSample { vaccine: Some(-1.0), ambient: None };
        
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::LowTemperatureStart);
        assert_eq!(temps.status, TemperatureState::FreezeNoAlarm);
    }

    #[test]
    fn test_freeze_to_hot_direct_transition() {
        let mut temps = Temperatures::new();
        temps.status = TemperatureState::FreezeNoAlarm;
        
        // Temperature rises directly to hot level
        let sample = TemperatureSample { vaccine: Some(8.5), ambient: None };
        
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::HighTemperatureStart);
        assert_eq!(temps.status, TemperatureState::HotNoAlarm);
    }

    #[test]
    fn test_alarm_expiration_high_temp() {
        let mut temps = Temperatures::new();
        temps.status = TemperatureState::HotNoAlarm;
        
        temps.alarm_expired(AlarmTimerExpired::HighTemperature);
        
        assert_eq!(temps.status, TemperatureState::HotAlarm);
        assert!(temps.is_high_alarm());
        assert!(!temps.is_low_alarm());
    }

    #[test]
    fn test_alarm_expiration_low_temp() {
        let mut temps = Temperatures::new();
        temps.status = TemperatureState::FreezeNoAlarm;
        
        temps.alarm_expired(AlarmTimerExpired::LowTemperature);
        
        assert_eq!(temps.status, TemperatureState::FreezeAlarm);
        assert!(temps.is_low_alarm());
        assert!(!temps.is_high_alarm());
    }

    #[test]
    fn test_new_alarm_flags() {
        let mut temps = Temperatures::new();
        
        // Trigger and resolve a high alarm quickly
        temps.alarm_expired(AlarmTimerExpired::HighTemperature);
        assert!(temps.is_high_alarm());
        
        // Return to safe, but new_high_alarm flag should still be set
        temps.status = TemperatureState::Safe;
        assert!(temps.is_high_alarm()); // Still true due to new_high_alarm flag
        
        // Clear flags
        temps.clear_new_alarms();
        assert!(!temps.is_high_alarm());
    }

    #[test]
    fn test_no_vaccine_temperature() {
        let mut temps = Temperatures::new();
        let sample = TemperatureSample { vaccine: None, ambient: Some(25.0) };
        
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::NoTrigger);
        assert_eq!(temps.status, TemperatureState::Safe);
    }

    #[test]
    fn test_stay_in_hot_state() {
        let mut temps = Temperatures::new();
        temps.status = TemperatureState::HotNoAlarm;
        
        // Temperature stays hot but not extreme
        let sample = TemperatureSample { vaccine: Some(8.2), ambient: None };
        
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::NoTrigger);
        assert_eq!(temps.status, TemperatureState::HotNoAlarm);
    }

    #[test]
    fn test_stay_in_freeze_state() {
        let mut temps = Temperatures::new();
        temps.status = TemperatureState::FreezeNoAlarm;
        
        // Temperature stays freezing
        let sample = TemperatureSample { vaccine: Some(-0.7), ambient: None };
        
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::NoTrigger);
        assert_eq!(temps.status, TemperatureState::FreezeNoAlarm);
    }

    #[test]
    fn test_hysteresis_boundary_conditions() {
        let mut temps = Temperatures::new();
        
        // Test exact hysteresis boundary for hot to safe
        // Exactly at the boundary should not cancel.
        temps.status = TemperatureState::HotAlarm;
        let sample = TemperatureSample { vaccine: Some(ALARM_HIGH_TEMPERATURE - ALARM_TEMP_HYSTERESIS), ambient: None };
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::NoTrigger);
        assert_eq!(temps.status, TemperatureState::HotAlarm);
        
        // Test exact hysteresis boundary for hot to safe
        // Just below the boundary should cancel.
        temps.status = TemperatureState::HotAlarm;
        let sample = TemperatureSample { vaccine: Some(ALARM_HIGH_TEMPERATURE - ALARM_TEMP_HYSTERESIS - 0.05), ambient: None };
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::TemperatureCancel);
        assert_eq!(temps.status, TemperatureState::Safe);
        
        // Test exact hysteresis boundary for freeze to safe
        // Just at the boundary should not cancel.
        temps.status = TemperatureState::FreezeAlarm;
        let sample = TemperatureSample { vaccine: Some(ALARM_LOW_TEMPERATURE + ALARM_TEMP_HYSTERESIS), ambient: None };
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::NoTrigger);
        assert_eq!(temps.status, TemperatureState::FreezeAlarm);

        // Test exact hysteresis boundary for freeze to safe
        // Just above the boundary should cancel.
        temps.status = TemperatureState::FreezeAlarm;
        let sample = TemperatureSample { vaccine: Some(ALARM_LOW_TEMPERATURE + ALARM_TEMP_HYSTERESIS + 0.05), ambient: None };
        let trigger = temps.new_temperatures(sample);
        assert_eq!(trigger, AlarmTimerTrigger::TemperatureCancel);
        assert_eq!(temps.status, TemperatureState::Safe);

    }
}

