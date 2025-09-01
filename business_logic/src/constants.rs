//! Constants for the business logic.

// Vaccine temperatures and times.
pub const MAX_GOOD_VACCINE_TEMP: f32 = 8.0;       // in °C
pub const MIN_GOOD_VACCINE_TEMP: f32 = 2.0;       // in °C
pub const ALARM_HIGH_TEMPERATURE: f32 = 8.0;      // in °C
pub const ALARM_LOW_TEMPERATURE: f32 = -0.5;      // in °C
pub const ALARM_TEMP_HYSTERESIS: f32 = 0.1;            // in °C
pub const ALARM_HIGH_SECONDS: u32 = 10 * 60 * 60; // 10 hours to trigger high temp alarm
pub const ALARM_LOW_SECONDS: u32 = 60 * 60;       // 1 hour to trigger low temp alarm

// Door open times.
pub const DOOR_ALARM_THRESHOLD: u32 = 300;    // 5 minutes in seconds.

// Aggregation/sampling.
pub const SAMPLE_PERIOD: u32 = 15 * 60; // 15 minutes in seconds.
pub const AGGREGATION_PERIOD: u32 = 8 * 60 * 60; // 8 hours in seconds.

pub const LOG_BUFFER_SIZE: usize = 24; // Number of entries in log buffer (should be enough for 6h at 15min intervals).
