//! This module contains the business logic for aggregating temperature, 
//! door opening, and power data

use crate::{door::{DoorEvent}, logger::{AlarmTrigger, LoggerEvent, TemperatureSample}, timestamp::{Timestamp, TimestampError}};

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
    low_alarm_start: Option<Timestamp>, // If alarming, when the alarm started.
    high_alarm_start: Option<Timestamp>, // If alarming, when the alarm started.
    door_alarm_start: Option<Timestamp>, // If alarming, when the door alarm started.
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
            low_alarm_start: None,
            high_alarm_start: None,
            door_alarm_start: None,
            door_open_start: None,
            power_on_start: None,
            compressor_on_start: None,
            long_record: AggregationRecord::default(),
        }
    }

    pub fn new_temperatures(&mut self, temps: TemperatureSample, now: Timestamp) {

    }

    pub fn process_door_event(&mut self, door: DoorEvent, now: Timestamp) -> AlarmTrigger {
        match door {
            DoorEvent::Opened => {
                if self.door_open_start.is_none() {
                    self.door_open_start = Some(now);
                    return AlarmTrigger::DoorOpenStart;
                }
            }
            DoorEvent::Closed => {
                if let Some(open_time) = self.door_open_start {
                    let open_duration = now.seconds - open_time.seconds; // TODO: check logic.
                    self.long_record.vaccine_door_seconds += open_duration;
                    self.door_open_start = None;
                    return AlarmTrigger::DoorOpenCancel;
                }
            }
        }
        AlarmTrigger::NoTrigger

    }

    // TODO: this is a placeholder.
    pub fn set_alarm_state(&mut self, state: AlarmTrigger, now: Timestamp) {
        match state {
            AlarmTrigger::LowTemperatureStart => {
                self.low_alarm_start = Some(now);
            }
            AlarmTrigger::LowTemperatureCancel => {
                self.low_alarm_start = None;
            }
            AlarmTrigger::HighTemperatureStart => {
                self.high_alarm_start = Some(now);
            }
            AlarmTrigger::HighTemperatureCancel => {
                self.high_alarm_start = None;
            }
            AlarmTrigger::DoorOpenStart => {
                self.door_open_start = Some(now);
            }
            AlarmTrigger::DoorOpenCancel => {
                self.door_open_start = None;
            }
            _ => {}
        }
    }



}