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
    dorv: u32, // Number of seconds the door has been open this sample.
    dorc: u16, // Number of door opening events this sample.
    alrm: AlarmFlags, // Bitfield of active alarms
}


#[derive(Debug, Clone, PartialEq)]
pub struct Logger {
    rtcw: Timestamp, // Last known RTC value when awoken.
    agg: Aggregator,
    temps: Temperatures,
    door: door::Door,
    log_buffer: ArrayVec<DataEntry, LOG_BUFFER_SIZE>,
}

impl Logger {
    pub fn new(rtcw: Timestamp, door_open: bool, now: Timestamp) -> Self {
        Self {
            rtcw,
            agg: Aggregator::new(door_open, now),
            temps: Temperatures::new(),
            door: door::Door::new(door_open, now),
            log_buffer: ArrayVec::new(),
        }
    }

    // TODO: do we need to return Result<> here? Where is the best place to protect against out of order timestamps?
    pub fn process_event(&mut self, event: LoggerEvent, now: Timestamp) -> Result<AlarmTimerTrigger, TimestampError> {

        // Process the event and store 
        // 1. whether we have an 8h data aggregation ready to write to flash,
        // 2. whether we need to start or cancel an alarm timer.
        let (aggregate_ready, trigger) = match event {
            LoggerEvent::TemperatureSample(sample) => {

                // Update aggregation with new sample. Do this before calling cancel_temperature_alarms()
                // so that a record can be finalized if necessary.
                let ready = self.agg.new_temperatures(sample, now);

                // Update temperature state machine and determine if we need to start/cancel an alarm timer.
                let trigger = self.temps.new_temperatures(sample);

                if trigger == AlarmTimerTrigger::TemperatureCancel {
                    self.agg.cancel_temperature_alarms();
                }

                // Add sample to buffer

                // Determine active alarms.
                let mut alarm_flags = AlarmFlags::empty();
                if self.temps.is_high_alarm() {
                    alarm_flags |= AlarmFlags::HIGH_TEMP;
                }
                if self.temps.is_low_alarm() {
                    alarm_flags |= AlarmFlags::LOW_TEMP;
                }
                if self.door.is_alarm() {
                    alarm_flags |= AlarmFlags::DOOR_OPEN;
                }

                // Get sample values from accumulators.
                let (dorc, dorv, idrv) = self.door.get_values(now);
                
                let entry = DataEntry {
                    relt: now,
                    rtcw: self.rtcw,
                    tvc: sample.vaccine,
                    tamb: sample.ambient,
                    dorv,
                    dorc,
                    alrm: alarm_flags,
                };
                
                if self.log_buffer.is_full() {
                    // TODO: write buffer to storage before clearing
                    self.log_buffer.clear();
                }
                self.log_buffer.push(entry);

                // Clear sample alarms and counts to prepare for the next sample.
                self.temps.clear_new_alarms();
                self.door.reset_accumulators();

                (ready, trigger)
            }
            LoggerEvent::DoorEvent(door_event) => {
                // Update aggregation with new sample. Do this before calling cancel_door_alarm()
                // so that a record can be finalized if necessary.
                let ready = self.agg.door_event(door_event, now);
                let trigger = self.door.door_event(door_event, now);
                if trigger == AlarmTimerTrigger::DoorOpenCancel {
                    self.agg.cancel_door_alarm();
                }

                (ready, trigger)
            }
            // LoggerEvent::PowerEvent(power_event) => {
            //     self.agg.process_power_event(power_event, ts);
            // }
            // LoggerEvent::CompressorEvent(compressor_event) => {
            //     self.agg.process_compressor_event(compressor_event, ts);
            // }
            LoggerEvent::AlarmStateChange(expiring) => {
                
                let ready = self.agg.alarm_expired(expiring, now);
                match expiring {
                    AlarmTimerExpired::HighTemperature | AlarmTimerExpired::LowTemperature => {
                        self.temps.alarm_expired(expiring);
                    },
                    AlarmTimerExpired::Door => {
                        self.door.alarm_expired(expiring);
                    },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::door::DoorEvent;

    fn create_temp_sample(vaccine: Option<f32>, ambient: Option<f32>) -> TemperatureSample {
        TemperatureSample { vaccine, ambient }
    }

    #[test]
    fn test_logger_initialization() {
        let rtcw = Timestamp { seconds: 500 };
        let now = Timestamp { seconds: 1000 };
        let logger = Logger::new(rtcw, false, now);
        
        assert_eq!(logger.rtcw, rtcw);
        assert_eq!(logger.log_buffer.len(), 0);
        assert!(logger.log_buffer.is_empty());
    }

    #[test]
    fn test_temperature_sample_processing() {
        let mut logger = Logger::new(Timestamp { seconds: 500 }, false, Timestamp { seconds: 1000 });
        let sample = create_temp_sample(Some(5.0), Some(20.0));
        
        let trigger = logger.process_event(LoggerEvent::TemperatureSample(sample), Timestamp { seconds: 1000 });
        
        assert!(trigger.is_ok());
        assert_eq!(logger.log_buffer.len(), 1);
        
        let entry = &logger.log_buffer[0];
        assert_eq!(entry.tvc, Some(5.0));
        assert_eq!(entry.tamb, Some(20.0));
        assert_eq!(entry.relt, Timestamp { seconds: 1000 });
        assert_eq!(entry.rtcw, Timestamp { seconds: 500 });
        assert_eq!(entry.alrm, AlarmFlags::empty());
    }

    #[test]
    fn test_door_event_processing() {
        let mut logger = Logger::new(Timestamp { seconds: 500 }, false, Timestamp { seconds: 1000 });
        
        let trigger = logger.process_event(LoggerEvent::DoorEvent(DoorEvent::Opened), Timestamp { seconds: 1100 });
        
        assert!(trigger.is_ok());
        assert_eq!(trigger.unwrap(), AlarmTimerTrigger::DoorOpenStart);
    }

    #[test]
    fn test_door_values_in_log_entry() {
        let mut logger = Logger::new(Timestamp { seconds: 500 }, false, Timestamp { seconds: 1000 });
        
        // Open door
        logger.process_event(LoggerEvent::DoorEvent(DoorEvent::Opened), Timestamp { seconds: 1100 }).unwrap();
        
        // Close door after 200 seconds
        logger.process_event(LoggerEvent::DoorEvent(DoorEvent::Closed), Timestamp { seconds: 1300 }).unwrap();
        
        // Process temperature sample to create log entry
        let sample = create_temp_sample(Some(5.0), None);
        logger.process_event(LoggerEvent::TemperatureSample(sample), Timestamp { seconds: 1350 }).unwrap();
        
        let entry = &logger.log_buffer[0];
        assert_eq!(entry.dorc, 1); // One door open event
        assert_eq!(entry.dorv, 200); // 200 seconds accumulated open time
    }

    #[test]
    fn test_alarm_flags_population() {
        let mut logger = Logger::new(Timestamp { seconds: 500 }, false, Timestamp { seconds: 1000 });
        
        // Trigger high temperature alarm
        logger.process_event(LoggerEvent::AlarmStateChange(AlarmTimerExpired::HighTemperature), Timestamp { seconds: 1100 }).unwrap();
        
        // Process temperature sample to create log entry with alarm flag
        let sample = create_temp_sample(Some(9.0), Some(25.0));
        logger.process_event(LoggerEvent::TemperatureSample(sample), Timestamp { seconds: 1150 }).unwrap();
        
        assert_eq!(logger.log_buffer.len(), 1);
        let entry = &logger.log_buffer[0];
        assert!(entry.alrm.contains(AlarmFlags::HIGH_TEMP));
        assert!(!entry.alrm.contains(AlarmFlags::LOW_TEMP));
        assert!(!entry.alrm.contains(AlarmFlags::DOOR_OPEN));
    }

    #[test]
    fn test_multiple_alarm_flags() {
        let mut logger = Logger::new(Timestamp { seconds: 500 }, true, Timestamp { seconds: 1000 });
        
        // Trigger multiple alarms
        logger.process_event(LoggerEvent::AlarmStateChange(AlarmTimerExpired::LowTemperature), Timestamp { seconds: 1100 }).unwrap();
        logger.process_event(LoggerEvent::AlarmStateChange(AlarmTimerExpired::Door), Timestamp { seconds: 1150 }).unwrap();
        
        // Process temperature sample
        let sample = create_temp_sample(Some(-1.0), Some(15.0));
        logger.process_event(LoggerEvent::TemperatureSample(sample), Timestamp { seconds: 1200 }).unwrap();
        
        let entry = &logger.log_buffer[0];
        assert!(entry.alrm.contains(AlarmFlags::LOW_TEMP));
        assert!(entry.alrm.contains(AlarmFlags::DOOR_OPEN));
        assert!(!entry.alrm.contains(AlarmFlags::HIGH_TEMP));
    }

    #[test]
    fn test_door_accumulator_reset() {
        let mut logger = Logger::new(Timestamp { seconds: 500 }, false, Timestamp { seconds: 1000 });
        
        // Open and close door
        logger.process_event(LoggerEvent::DoorEvent(DoorEvent::Opened), Timestamp { seconds: 1100 }).unwrap();
        logger.process_event(LoggerEvent::DoorEvent(DoorEvent::Closed), Timestamp { seconds: 1200 }).unwrap();
        
        // Process temperature sample - should reset door accumulators
        let sample = create_temp_sample(Some(5.0), None);
        logger.process_event(LoggerEvent::TemperatureSample(sample), Timestamp { seconds: 1300 }).unwrap();
        
        let entry = &logger.log_buffer[0];
        assert_eq!(entry.dorc, 1);
        assert_eq!(entry.dorv, 100); // 100 seconds open
        
        // Process another temperature sample - accumulators should be reset
        logger.process_event(LoggerEvent::TemperatureSample(sample), Timestamp { seconds: 1400 }).unwrap();
        
        let entry2 = &logger.log_buffer[1];
        assert_eq!(entry2.dorc, 0); // Reset after previous sample
        assert_eq!(entry2.dorv, 0); // Reset after previous sample
    }

    #[test]
    fn test_log_buffer_overflow() {
        let mut logger = Logger::new(Timestamp { seconds: 500 }, false, Timestamp { seconds: 1000 });
        let sample = create_temp_sample(Some(5.0), Some(20.0));
        
        // Fill buffer to capacity
        for i in 0..crate::constants::LOG_BUFFER_SIZE {
            let timestamp = Timestamp { seconds: 1000 + (i as u32) * 60 };
            logger.process_event(LoggerEvent::TemperatureSample(sample), timestamp).unwrap();
        }
        
        assert_eq!(logger.log_buffer.len(), crate::constants::LOG_BUFFER_SIZE);
        
        // Add one more entry - should clear and restart
        logger.process_event(LoggerEvent::TemperatureSample(sample), Timestamp { seconds: 2500 }).unwrap();
        
        assert_eq!(logger.log_buffer.len(), 1); // Buffer was cleared and new entry added
    }

    #[test]
    fn test_door_cancel_integration() {
        let mut logger = Logger::new(Timestamp { seconds: 500 }, true, Timestamp { seconds: 1000 });
        
        // Close door - should trigger DoorOpenCancel
        let trigger = logger.process_event(LoggerEvent::DoorEvent(DoorEvent::Closed), Timestamp { seconds: 1200 });
        
        assert!(trigger.is_ok());
        assert_eq!(trigger.unwrap(), AlarmTimerTrigger::DoorOpenCancel);
    }

    #[test]
    fn test_comprehensive_workflow() {
        let mut logger = Logger::new(Timestamp { seconds: 500 }, false, Timestamp { seconds: 1000 });
        
        // Initial temperature sample
        let sample1 = create_temp_sample(Some(5.0), Some(20.0));
        logger.process_event(LoggerEvent::TemperatureSample(sample1), Timestamp { seconds: 1000 }).unwrap();
        
        // Open door
        logger.process_event(LoggerEvent::DoorEvent(DoorEvent::Opened), Timestamp { seconds: 1050 }).unwrap();
        
        // High temperature alarm
        logger.process_event(LoggerEvent::AlarmStateChange(AlarmTimerExpired::HighTemperature), Timestamp { seconds: 1100 }).unwrap();
        
        // Door alarm while door is open
        logger.process_event(LoggerEvent::AlarmStateChange(AlarmTimerExpired::Door), Timestamp { seconds: 1150 }).unwrap();
        
        // Temperature sample with both alarms active
        let sample2 = create_temp_sample(Some(9.0), Some(25.0));
        logger.process_event(LoggerEvent::TemperatureSample(sample2), Timestamp { seconds: 1200 }).unwrap();
        
        // Close door
        logger.process_event(LoggerEvent::DoorEvent(DoorEvent::Closed), Timestamp { seconds: 1250 }).unwrap();
        
        // Final temperature sample
        let sample3 = create_temp_sample(Some(6.0), Some(22.0));
        logger.process_event(LoggerEvent::TemperatureSample(sample3), Timestamp { seconds: 1300 }).unwrap();
        
        // Verify log entries
        assert_eq!(logger.log_buffer.len(), 3);
        
        let middle_entry = &logger.log_buffer[1];
        assert!(middle_entry.alrm.contains(AlarmFlags::HIGH_TEMP));
        assert!(middle_entry.alrm.contains(AlarmFlags::DOOR_OPEN));
        assert_eq!(middle_entry.dorc, 1); // Door opened since last sample
        
        let final_entry = &logger.log_buffer[2];
        assert_eq!(final_entry.dorc, 0); // No new door events since last sample
        assert_eq!(final_entry.dorv, 200); // Total accumulated open time from this sample period
    }
}
