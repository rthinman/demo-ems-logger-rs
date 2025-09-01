//! This module contains the business logic for aggregating temperature, 
//! door opening, and power data

use arrayvec::ArrayVec;
use bitflags::bitflags;
use crate::constants::LOG_BUFFER_SIZE;
use crate::{aggregator::Aggregator, door, temperatures::Temperatures, timestamp::{Timestamp, TimestampError}};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TemperatureSample {
    pub ambient: Option<f32>,
    pub vaccine: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoggerEvent {
    TemperatureSample(TemperatureSample),
    DoorEvent(door::DoorEvent),
    // PowerEvent(aggregator::PowerEvent),
    // CompressorEvent(aggregator::CompressorEvent),
    AlarmStateChange(AlarmTimerExpired),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AlarmTimerTrigger {
    #[default]
    NoTrigger,
    LowTemperatureStart,
    HighTemperatureStart,
    TemperatureCancel,
    DoorOpenStart,
    DoorOpenCancel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlarmTimerExpired {
    LowTemperature,
    HighTemperature,
    Door,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Default)]
    pub struct AlarmFlags: u8 {
        const HIGH_TEMP = 0b00000001;
        const LOW_TEMP  = 0b00000010;
        const DOOR_OPEN = 0b00000100;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct DataEntry {
    relt: Timestamp,  // Relative time in seconds since logging started.
    rtcw: Timestamp,  // RTC value when awoken.
    tvc: Option<f32>, // Vaccine temperature in degC
    tamb: Option<f32>, // Ambient temperature in degC
    // blog: u16, // Logger battery lifetime in days.
    // cmpr: u16, // Compressor run time this sample in seconds.
    // cmps: u16, // Compressor maximum speed this sample in RPM.
    // sva: u16, // Supply voltage availability in seconds this sample.
    dorv: u16, // Door open time this sample in seconds.
    dorc: u16, // Door open count this sample.
    alrm: AlarmFlags, // Bitfield of active alarms
}


#[derive(Debug, Clone, PartialEq)]
pub struct Logger {
    agg: Aggregator,
    temps: Temperatures, 
    log_buffer: ArrayVec<DataEntry, LOG_BUFFER_SIZE>,
}

impl Logger {
    pub fn new(now: Timestamp) -> Self {
        Self {
            agg: Aggregator::new(now),
            temps: Temperatures::new(),
            log_buffer: ArrayVec::new(),
        }
    }

    // TODO: do we need to return Result<> here? Where is the best place to protect against out of order timestamps?
    pub fn process_event(&mut self, event: LoggerEvent, ts: Timestamp) -> Result<AlarmTimerTrigger, TimestampError> {

        // Process the event and store 
        // 1. whether we have an 8h data aggregation ready to write to flash,
        // 2. whether we need to start or cancel an alarm timer.
        let (aggregate_ready, trigger) = match event {
            LoggerEvent::TemperatureSample(sample) => {

                // Update aggregation with new sample. Do this before calling cancel_temperature_alarms()
                // so that a record can be finalized if necessary.
                let ready = self.agg.new_temperatures(sample, ts);

                // Update temperature state machine and determine if we need to start/cancel an alarm timer.
                let trigger = self.temps.new_temperatures(sample);

                if trigger == AlarmTimerTrigger::TemperatureCancel {
                    self.agg.cancel_temperature_alarms();
                }

                // Add sample to buffer
                let mut alarm_flags = AlarmFlags::empty();
                if self.temps.is_high_alarm() {
                    alarm_flags |= AlarmFlags::HIGH_TEMP;
                }
                if self.temps.is_low_alarm() {
                    alarm_flags |= AlarmFlags::LOW_TEMP;
                }


                // TODO: add door alarm check, including new alarm flag clearing below.
                
                // Clear new alarm flags.
                self.temps.clear_new_alarms();

                let entry = DataEntry {
                    relt: ts, // TODO: convert to relative time
                    rtcw: ts, // TODO: get actual RTCW
                    tvc: sample.vaccine,
                    tamb: sample.ambient,
                    dorv: 0, // TODO: get from aggregator
                    dorc: 0, // TODO: get from aggregator
                    alrm: alarm_flags,
                };
                
                if self.log_buffer.is_full() {
                    // TODO: write buffer to storage before clearing
                    self.log_buffer.clear();
                }
                self.log_buffer.push(entry);

                (ready, trigger)
            }
            LoggerEvent::DoorEvent(door_event) => {
                self.agg.process_door_event(door_event, ts);

                // Placeholder.
                let t = if door_event == door::DoorEvent::Opened {
                    AlarmTimerTrigger::DoorOpenStart
                } else {
                    AlarmTimerTrigger::DoorOpenCancel
                };
                (false, t)
            }
            // LoggerEvent::PowerEvent(power_event) => {
            //     self.agg.process_power_event(power_event, ts);
            // }
            // LoggerEvent::CompressorEvent(compressor_event) => {
            //     self.agg.process_compressor_event(compressor_event, ts);
            // }
            LoggerEvent::AlarmStateChange(state) => {
                // Placeholder.
                let ready = self.agg.alarm_expired(state, ts);
                match state {
                    AlarmTimerExpired::HighTemperature | AlarmTimerExpired::LowTemperature => {
                        self.temps.alarm_expired(state);
                    },
                    _ => {}, // TODO: add door alarm handling here.
                }
                (ready, AlarmTimerTrigger::NoTrigger)
            }
        };

        // Handle aggregation period rollover.
        // if aggregate_ready {
        //     self.agg.rollover_aggregation(ts);
        // }

        Ok(trigger)
    }
}
