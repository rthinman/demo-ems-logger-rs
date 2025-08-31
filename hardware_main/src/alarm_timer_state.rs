//! Alarm timer state management.

use embassy_time::{Duration, Instant};
use business_logic::logger::AlarmTimerTrigger;
use crate::fmt::info;


#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TempTimerActive {
    #[default]
    NoneActive,
    LowTemperature,
    HighTemperature,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlarmTimerState {
    pub door_active: bool,
    pub temperature_active: TempTimerActive,
    pub door_expires: Instant,
    pub temperature_expires: Instant,
}

impl Default for AlarmTimerState {
    fn default() -> Self {
        Self::new()
    }
}

impl AlarmTimerState {
    pub fn new() -> Self {
        Self {
            door_active: false,
            temperature_active: TempTimerActive::NoneActive,
            door_expires: Instant::now(),
            temperature_expires: Instant::now(),
        }
    }

    pub fn process_trigger(&mut self, trigger: AlarmTimerTrigger, now: Instant) {
        match trigger {
            AlarmTimerTrigger::NoTrigger => {}
            AlarmTimerTrigger::LowTemperatureStart => {
                if self.temperature_active != TempTimerActive::LowTemperature {
                    // Don't retrigger if already active.
                    info!("Low temperature alarm started");
                    self.temperature_active = TempTimerActive::LowTemperature;
                    self.temperature_expires = now + Duration::from_secs(60); // Example duration
                }
            }
            AlarmTimerTrigger::HighTemperatureStart => {
                if self.temperature_active != TempTimerActive::HighTemperature {
                    // Don't retrigger if already active.
                    info!("High temperature alarm started");
                    self.temperature_active = TempTimerActive::HighTemperature;
                    self.temperature_expires = now + Duration::from_secs(60); // Example duration
                }
            }
            AlarmTimerTrigger::TemperatureCancel => {
                info!("Temperature alarm canceled");
                self.temperature_active = TempTimerActive::NoneActive;
            }
            AlarmTimerTrigger::DoorOpenStart => {
                info!("Door open alarm started");
                self.door_active = true;
                self.door_expires = now + Duration::from_secs(30); // Example duration
            }
            AlarmTimerTrigger::DoorOpenCancel => {
                info!("Door open alarm canceled");
                self.door_active = false;
            }
        }
    }
}
