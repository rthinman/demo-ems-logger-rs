//! This module contains the business logic for aggregating temperature, 
//! door opening, and power data

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Logger {
    agg: Aggregator,
    temps: Temperatures, 
}

impl Logger {
    pub fn new(now: Timestamp) -> Self {
        Self {
            agg: Aggregator::new(now),
            temps: Temperatures::new(),
        }
    }

    // TODO: track alarm status so we don't retrigger if already alarming.
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

                // TODO: If samples buffer is full, write to a file. logger.c log_sample()
                // TODO: Populate an entry in the samples buffer. log_sample()
                // 

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
