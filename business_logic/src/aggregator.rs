//! This module contains the business logic for aggregating temperature, 
//! door opening, and power data

use crate::{door::{DoorEvent}, logger::{AlarmTimerExpired, AlarmTimerTrigger, LoggerEvent, TemperatureSample}, timestamp::{Timestamp, TimestampError}};
use crate::constants::{MAX_GOOD_VACCINE_TEMP, MIN_GOOD_VACCINE_TEMP};

// Structs to hold data

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AggregationRecord {
    pub tvc_sum: f32, // Sum of all vaccine temperatures * their time intervals.   Divided by vaccine_temp_seconds, this gives a time-weighted average vaccine temperature.
    pub tvc_seconds: u32, // Sum of all vaccine temperature measurement time intervals in this record in seconds.
    pub tvc_min: f32, // Minimum vaccine temperature for this record.
    pub tvc_max: f32, // Maximum vaccine temperature for this record.
    pub tamb_sum: f32, // Sum of all ambient temperatures * their time intervals.  Divided by ambient_temp_secons, this gives a time-weighted average ambient temperature.
    pub tamb_seconds: u32, // Sum of all ambient temperature measurement time intervals in this record in seconds.
    pub tvc_low_seconds: u32, // Number of seconds that the vaccine temperature has been less than 2.0 degrees C for this record.
    pub tvc_high_seconds: u32, // Number of seconds thata the vaccine temperature has been greater than 8.0 degrees C for this record.
    pub low_alarm_seconds: u32, // Number of seconds that a low temperature alarm has been in effect for this record.
    pub high_alarm_seconds: u32, // Number of seconds that a high temperature alarm hwas been in effect for this record.
    pub vaccine_door_count: u16, // Number of times the vaccine door has been opened for this record. 
    pub vaccine_door_seconds: u32, // Number of seconds that the vaccine door has been open for this record.
    pub power_available_seconds: u32, // Number of seconds that power has been available for this record.
    pub compressor_run_seconds: u32, // Number of seconds that the compressor has been running for this record.
    pub door_alarm_seconds: u32, // Number of seconds that the door alarm has been in effect for this record.
    // pub logger_errors: u32, // Aggregation of up to 4 8-bit packed error codes that have been recorded during this record.  Zero is no error.
    pub records_read: u8, // Only used when combining records into a single day.  The number of records of combined data.
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Aggregator {
    // status: AlarmState,
    timestamp: Timestamp, // TODO: use?  I think it is The timestamp of the last sample received.
    next_record_start: Timestamp, // Timestamp when the next record should start.
//    prev_long_sample_ended: Timestamp, // Timestamp the previous long sampling period ended (just before the start of the current record).
    last_ambient_temp: Option<f32>, // The last good ambient temperature received.
    last_vaccine_temp: Option<f32>, // The last good vaccine temperature received.
    last_ambient_ts: Option<Timestamp>, // The timestamp of the last good ambient temperature.
    last_vaccine_ts: Option<Timestamp>, // The timestamp of the last good vaccine temperature.
    low_alarm_ts: Option<Timestamp>, // If alarming, the last timestamp where alarm was active.
    high_alarm_ts: Option<Timestamp>, // If alarming, the last timestamp where alarm was active.
    door_alarm_ts: Option<Timestamp>, // If alarming, the last timestamp where alarm was active.
    door_open_start: Option<Timestamp>, // If the door is open, when it was opened.
    power_on_start: Option<Timestamp>, // If power is available, when it was last available.
    compressor_on_start: Option<Timestamp>, // If the compressor is running, when it was last started.
    // logger_errors TODO: add later.
    long_record: AggregationRecord, // The long aggregation record currently in process.

}

impl Aggregator {
    pub fn new(now: Timestamp) -> Self {
        let next_record_start = now.get_next_aggregation_start();

        Self {
            // status: AlarmState::Normal,
            timestamp: now,
            next_record_start,
            last_ambient_temp: None,
            last_vaccine_temp: None,
            last_ambient_ts: None,
            last_vaccine_ts: None,
            low_alarm_ts: None,
            high_alarm_ts: None,
            door_alarm_ts: None,
            door_open_start: None,
            power_on_start: None,
            compressor_on_start: None,
            long_record: AggregationRecord::default(),
        }
    }

    /// Process new temperature samples and update the aggregation record.
    /// Returns true if the current aggregation record is complete and should be saved.
    pub fn new_temperatures(&mut self, temps: TemperatureSample, now: Timestamp) -> bool {
        
        // TODO: let record_ready = check_for_end_of_record(now);
        
        // Update the average ambient temperature
        if let Some(last_ambient_ts) = self.last_ambient_ts {
            if let Some(last_ambient_temp) = self.last_ambient_temp {
                let time_interval = now.seconds - last_ambient_ts.seconds;
                self.long_record.tamb_sum += time_interval as f32 * last_ambient_temp;
                self.long_record.tamb_seconds += time_interval;
            }
        }
        
        // Update ambient temperature if present
        if let Some(ambient) = temps.ambient {
            self.last_ambient_temp = Some(ambient);
            self.last_ambient_ts = Some(now);
        } else {
            self.last_ambient_ts = None;
        }

        // Update the vaccine temperature and related values
        if let Some(last_vaccine_ts) = self.last_vaccine_ts {
            if let Some(last_vaccine_temp) = self.last_vaccine_temp {
                let vaccine_time = now.seconds - last_vaccine_ts.seconds;
                
                if last_vaccine_temp >= MAX_GOOD_VACCINE_TEMP {
                    self.long_record.tvc_high_seconds += vaccine_time;
                }
                if last_vaccine_temp < MIN_GOOD_VACCINE_TEMP {
                    self.long_record.tvc_low_seconds += vaccine_time;
                }
                self.long_record.tvc_sum += vaccine_time as f32 * last_vaccine_temp;
                self.long_record.tvc_seconds += vaccine_time;
            }
        }
        
        // Update vaccine temperature if present
        if let Some(vaccine) = temps.vaccine {
            self.last_vaccine_temp = Some(vaccine);
            self.last_vaccine_ts = Some(now);
            
            if self.long_record.tvc_seconds > 0 {
                if vaccine < self.long_record.tvc_min {
                    self.long_record.tvc_min = vaccine;
                }
                if vaccine > self.long_record.tvc_max {
                    self.long_record.tvc_max = vaccine;
                }
            } else {
                // TODO: make sure when we end record and flush, that we reset min and max to the last temperature.
                self.long_record.tvc_min = vaccine;
                self.long_record.tvc_max = vaccine;
            }
        } else {
            self.last_vaccine_ts = None;
        }

        // Update the alarm times
        if let Some(high_alarm_ts) = self.high_alarm_ts {
            self.long_record.high_alarm_seconds += now.seconds - high_alarm_ts.seconds;
            self.high_alarm_ts = Some(now);
        }
        if let Some(low_alarm_ts) = self.low_alarm_ts {
            self.long_record.low_alarm_seconds += now.seconds - low_alarm_ts.seconds;
            self.low_alarm_ts = Some(now);
        }
        if let Some(door_alarm_ts) = self.door_alarm_ts {
            self.long_record.door_alarm_seconds += now.seconds - door_alarm_ts.seconds;
            self.door_alarm_ts = Some(now);
        }

        false // TODO: determine proper return value: record_ready
    }

    pub fn process_door_event(&mut self, door: DoorEvent, now: Timestamp) -> AlarmTimerTrigger {
        // TODO: let record_ready = check_for_end_of_record(now); and return instead of the trigger (trigger handled by door state machine).
        match door {
            DoorEvent::Opened => {
                if self.door_open_start.is_none() {
                    self.door_open_start = Some(now);
                    return AlarmTimerTrigger::DoorOpenStart;
                }
            }
            DoorEvent::Closed => {
                if let Some(open_time) = self.door_open_start {
                    let open_duration = now.seconds - open_time.seconds; // TODO: check logic.
                    self.long_record.vaccine_door_seconds += open_duration;
                    self.door_open_start = None;
                    return AlarmTimerTrigger::DoorOpenCancel;
                }
            }
        }
        AlarmTimerTrigger::NoTrigger

    }

    /// Update the aggragation state if an alarm timer has expired, signaling that an alarm is now active.
    /// Returns true if the current aggregation record is complete and should be saved.
    pub fn alarm_expired(&mut self, state: AlarmTimerExpired, now: Timestamp) -> bool {
        // This is a combination of BINARY_temp_alarm_state_change() and BINARY_door_alarm_state_change().
        // TODO: let record_ready = check_for_end_of_record(now);

        // Update the alarm aggregate times
        if let Some(high_alarm_ts) = self.high_alarm_ts {
            self.long_record.high_alarm_seconds += now.seconds - high_alarm_ts.seconds;
            self.high_alarm_ts = Some(now);
        }
        if let Some(low_alarm_ts) = self.low_alarm_ts {
            self.long_record.low_alarm_seconds += now.seconds - low_alarm_ts.seconds;
            self.low_alarm_ts = Some(now);
        }
        if let Some(door_alarm_ts) = self.door_alarm_ts {
            self.long_record.door_alarm_seconds += now.seconds - door_alarm_ts.seconds;
            self.door_alarm_ts = Some(now);
        }

        match state {
            AlarmTimerExpired::LowTemperature => {
                self.low_alarm_ts = Some(now);
                self.high_alarm_ts = None; // Clear high alarm if low alarm starts.
            }
            AlarmTimerExpired::HighTemperature => {
                self.high_alarm_ts = Some(now);
                self.low_alarm_ts = None; // Clear low alarm if high alarm starts.
            }
            AlarmTimerExpired::Door => {
                self.door_open_start = Some(now);
            }
            // AlarmTimerExpired::Door => {
            //     self.door_open_start = None;
            // }
            // _ => {}
        }

        false // TODO: determine proper return value: record_ready
    }

    /// Cancel any ongoing temperature alarms.
    /// Only call this after calling new_temperatures() to update the aggregation
    /// record, so that we don't have to update the alarm times here.
    pub fn cancel_temperature_alarms(&mut self) {
        self.high_alarm_ts = None;
        self.low_alarm_ts = None;
    }

}