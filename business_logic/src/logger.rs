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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AlarmTrigger {
    #[default]
    NoTrigger,
    LowTemperatureStart,
    HighTemperatureStart,
    TemperatureCancel,
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

    // TODO: track alarm status so we don't retrigger if already alarming.
    pub fn process_event(&mut self, event: LoggerEvent, ts: Timestamp) -> Result<AlarmTrigger, TimestampError> {
        match event {
            LoggerEvent::TemperatureSample(sample) => {
                self.agg.new_temperatures(sample, ts);
                if let Some(vax) = sample.vaccine {
                    if vax < 2.0 {
                        Ok(AlarmTrigger::LowTemperatureStart)
                    } else if vax > 8.0 {
                        Ok(AlarmTrigger::HighTemperatureStart)
                    } else {
                        Ok(AlarmTrigger::TemperatureCancel)
                    }
                } else {
                    Ok(AlarmTrigger::NoTrigger)
                }
            }
            LoggerEvent::DoorEvent(door_event) => {
                self.agg.process_door_event(door_event, ts);
                if door_event == door::DoorEvent::Opened {
                    Ok(AlarmTrigger::DoorOpenStart)
                } else {
                    Ok(AlarmTrigger::DoorOpenCancel)
                }
            }
            // LoggerEvent::PowerEvent(power_event) => {
            //     self.agg.process_power_event(power_event, ts);
            // }
            // LoggerEvent::CompressorEvent(compressor_event) => {
            //     self.agg.process_compressor_event(compressor_event, ts);
            // }
            LoggerEvent::AlarmStateChange(state) => {
                self.agg.set_alarm_state(state, ts);
                Ok(AlarmTrigger::NoTrigger)
            }
        }
        
    }
}
