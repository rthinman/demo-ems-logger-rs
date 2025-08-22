//! This module contains the business logic for aggregating temperature, 
//! door opening, and power data

use crate::{aggregator, door, timestamp::{Timestamp, TimestampError}};

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
    AlarmStateChange(AlarmTrigger),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlarmTrigger {
    NoTrigger,
    LowTemperatureStart,
    LowTemperatureCancel,
    HighTemperatureStart,
    HighTemperatureCancel,
    DoorOpenStart,
    DoorOpenCancel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Logger {
    pub agg: aggregator::Aggregator,
}

impl Logger {
    pub fn new(now: Timestamp) -> Self {
        Self {
            agg: aggregator::Aggregator::new(now),
        }
    }

    pub fn process_event(&mut self, event: LoggerEvent, ts: Timestamp) -> Result<AlarmTrigger, TimestampError> {
        match event {
            LoggerEvent::TemperatureSample(sample) => {
                self.agg.new_temperatures(sample, ts);
            }
            LoggerEvent::DoorEvent(door_event) => {
                self.agg.process_door_event(door_event, ts);
            }
            // LoggerEvent::PowerEvent(power_event) => {
            //     self.agg.process_power_event(power_event, ts);
            // }
            // LoggerEvent::CompressorEvent(compressor_event) => {
            //     self.agg.process_compressor_event(compressor_event, ts);
            // }
            // LoggerEvent::AlarmStateChange(state) => {
            //     self.agg.set_alarm_state(state, ts);
            // }
        }
        Ok(AlarmTrigger::NoTrigger) // Placeholder, actual logic to determine trigger should be implemented
    }
}
