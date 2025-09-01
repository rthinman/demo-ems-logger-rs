//! This module contains the business logic for aggregating temperature, 
//! door opening, and power data

use crate::{door::{DoorEvent}, logger::{AlarmTimerExpired, AlarmTimerTrigger, LoggerEvent, TemperatureSample}, timestamp::{Timestamp, TimestampError}};
use crate::constants::{MAX_GOOD_VACCINE_TEMP, MIN_GOOD_VACCINE_TEMP};

// Structs to hold data

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AggregationRecord {
    pub record_start: Timestamp, // The start time of this record.
    pub record_length_seconds: u16, // The length of this record in seconds.  Normally 8 hours (28800 seconds), but may be less for the last record of the day or if power was lost.
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
    active_record: AggregationRecord, // The aggregation record currently in process.
    prev_record: AggregationRecord,   // The previous aggregation record, saved for retrieval.
}

impl Aggregator {
    pub fn new(now: Timestamp) -> Self {
        let next_record_start = now.get_next_aggregation_start();
        let mut active_record = AggregationRecord::default();
        active_record.record_start = now;

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
            active_record,
            prev_record: AggregationRecord::default(),
        }
    }

    /// Process new temperature samples and update the aggregation record.
    /// Returns true if the current aggregation record is complete and should be saved.
    pub fn new_temperatures(&mut self, temps: TemperatureSample, now: Timestamp) -> bool {
        
        let record_ready = self.check_for_end_of_record(now);
        
        // Update the average ambient temperature
        if let Some(last_ambient_ts) = self.last_ambient_ts {
            if let Some(last_ambient_temp) = self.last_ambient_temp {
                let time_interval = now.seconds - last_ambient_ts.seconds;
                self.active_record.tamb_sum += time_interval as f32 * last_ambient_temp;
                self.active_record.tamb_seconds += time_interval;
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
                    self.active_record.tvc_high_seconds += vaccine_time;
                }
                if last_vaccine_temp < MIN_GOOD_VACCINE_TEMP {
                    self.active_record.tvc_low_seconds += vaccine_time;
                }
                self.active_record.tvc_sum += vaccine_time as f32 * last_vaccine_temp;
                self.active_record.tvc_seconds += vaccine_time;
            }
        }
        
        // Update vaccine temperature if present
        if let Some(vaccine) = temps.vaccine {
            self.last_vaccine_temp = Some(vaccine);
            self.last_vaccine_ts = Some(now);
            
            if self.active_record.tvc_seconds > 0 {
                if vaccine < self.active_record.tvc_min {
                    self.active_record.tvc_min = vaccine;
                }
                if vaccine > self.active_record.tvc_max {
                    self.active_record.tvc_max = vaccine;
                }
            } else {
                // TODO: make sure when we end record and flush, that we reset min and max to the last temperature.
                self.active_record.tvc_min = vaccine;
                self.active_record.tvc_max = vaccine;
            }
        } else {
            self.last_vaccine_ts = None;
        }

        // Update the alarm times
        if let Some(high_alarm_ts) = self.high_alarm_ts {
            self.active_record.high_alarm_seconds += now.seconds - high_alarm_ts.seconds;
            self.high_alarm_ts = Some(now);
        }
        if let Some(low_alarm_ts) = self.low_alarm_ts {
            self.active_record.low_alarm_seconds += now.seconds - low_alarm_ts.seconds;
            self.low_alarm_ts = Some(now);
        }
        if let Some(door_alarm_ts) = self.door_alarm_ts {
            self.active_record.door_alarm_seconds += now.seconds - door_alarm_ts.seconds;
            self.door_alarm_ts = Some(now);
        }

        record_ready
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
                    self.active_record.vaccine_door_seconds += open_duration;
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
        let record_ready = self.check_for_end_of_record(now);

        // Update the alarm aggregate times
        if let Some(high_alarm_ts) = self.high_alarm_ts {
            self.active_record.high_alarm_seconds += now.seconds - high_alarm_ts.seconds;
            self.high_alarm_ts = Some(now);
        }
        if let Some(low_alarm_ts) = self.low_alarm_ts {
            self.active_record.low_alarm_seconds += now.seconds - low_alarm_ts.seconds;
            self.low_alarm_ts = Some(now);
        }
        if let Some(door_alarm_ts) = self.door_alarm_ts {
            self.active_record.door_alarm_seconds += now.seconds - door_alarm_ts.seconds;
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

        record_ready
    }

    /// Cancel any ongoing temperature alarms.
    /// Only call this after calling new_temperatures() to update the aggregation
    /// record, so that we don't have to update the alarm times here.
    pub fn cancel_temperature_alarms(&mut self) {
        self.high_alarm_ts = None;
        self.low_alarm_ts = None;
    }

    // Private methods --------------------------

    /// Check if the current time indicates the end of the current aggregation record.
    fn check_for_end_of_record(&mut self, now: Timestamp) -> bool {
        // TODO: do we need to handle the reset clearing all data, as in the C code?

        // Not yet time to end the record; nothing to do.
        if now.seconds < self.next_record_start.seconds {
            return false;
        }

        // Finalize the current record.
        if let Some(last_ambient_ts) = self.last_ambient_ts {
            if let Some(last_ambient_temp) = self.last_ambient_temp {
                let time_interval = self.next_record_start.seconds - last_ambient_ts.seconds;
                self.active_record.tamb_sum += time_interval as f32 * last_ambient_temp;
                self.active_record.tamb_seconds += time_interval;
            }
        }
        if let Some(last_vaccine_ts) = self.last_vaccine_ts {
            if let Some(last_vaccine_temp) = self.last_vaccine_temp {
                let vaccine_time = self.next_record_start.seconds - last_vaccine_ts.seconds;
                
                if last_vaccine_temp >= MAX_GOOD_VACCINE_TEMP {
                    self.active_record.tvc_high_seconds += vaccine_time;
                }
                if last_vaccine_temp < MIN_GOOD_VACCINE_TEMP {
                    self.active_record.tvc_low_seconds += vaccine_time;
                }
                self.active_record.tvc_sum += vaccine_time as f32 * last_vaccine_temp;
                self.active_record.tvc_seconds += vaccine_time;
            }
        }
        if let Some(high_alarm_ts) = self.high_alarm_ts {
            self.active_record.high_alarm_seconds += self.next_record_start.seconds - high_alarm_ts.seconds;
            self.high_alarm_ts = Some(self.next_record_start);
        }
        if let Some(low_alarm_ts) = self.low_alarm_ts {
            self.active_record.low_alarm_seconds += self.next_record_start.seconds - low_alarm_ts.seconds;
            self.low_alarm_ts = Some(self.next_record_start);
        }
        if let Some(door_alarm_ts) = self.door_alarm_ts {
            self.active_record.door_alarm_seconds += self.next_record_start.seconds - door_alarm_ts.seconds;
            self.door_alarm_ts = Some(self.next_record_start);
        }
        if let Some(door_open_start) = self.door_open_start {
            let open_duration = self.next_record_start.seconds - door_open_start.seconds;
            self.active_record.vaccine_door_seconds += open_duration;
            self.door_open_start = Some(self.next_record_start);
        }
        // if let Some(power_on_start) = self.power_on_start {
        //     let power_duration = self.next_record_start.seconds - power_on_start.seconds;
        //     self.active_record.power_available_seconds += power_duration;
        //     self.power_on_start = Some(self.next_record_start);
        // }
        // if let Some(compressor_on_start) = self.compressor_on_start {
        //     let compressor_duration = self.next_record_start.seconds - compressor_on_start.seconds;
        //     self.active_record.compressor_run_seconds += compressor_duration;
        //     self.compressor_on_start = Some(self.next_record_start);
        // }

        // TODO: logger errors eventually.

        self.active_record.record_length_seconds = (self.next_record_start.seconds - self.active_record.record_start.seconds) as u16;
        self.prev_record = self.active_record; // Save the completed record.
        self.active_record = AggregationRecord::default(); // Start a new record.
        self.active_record.record_start = self.next_record_start;
        
        self.timestamp = self.next_record_start;
        self.next_record_start = self.next_record_start.get_next_aggregation_start();

        // Signal that the record is ready to be saved.
        true
    }
            

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::{AlarmTimerExpired, TemperatureSample};
    use crate::timestamp::Timestamp;

    fn create_temp_sample(vaccine: Option<f32>, ambient: Option<f32>) -> TemperatureSample {
        TemperatureSample { vaccine, ambient }
    }

    #[test]
    fn test_aggregator_initialization() {
        let now = Timestamp { seconds: 1000 };
        let agg = Aggregator::new(now);
        
        assert_eq!(agg.timestamp, now);
        assert_eq!(agg.last_ambient_temp, None);
        assert_eq!(agg.last_vaccine_temp, None);
        assert_eq!(agg.active_record.record_start, now);
        assert_eq!(agg.active_record.tvc_sum, 0.0);
        assert_eq!(agg.active_record.tamb_sum, 0.0);
    }

    #[test]
    fn test_single_temperature_sample() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        let sample = create_temp_sample(Some(5.0), Some(20.0));
        let now = Timestamp { seconds: 1000 };
        
        agg.new_temperatures(sample, now);
        
        assert_eq!(agg.last_vaccine_temp, Some(5.0));
        assert_eq!(agg.last_ambient_temp, Some(20.0));
        assert_eq!(agg.active_record.tvc_min, 5.0);
        assert_eq!(agg.active_record.tvc_max, 5.0);
        // No time accumulation yet since this is the first sample
        assert_eq!(agg.active_record.tvc_sum, 0.0);
        assert_eq!(agg.active_record.tamb_sum, 0.0);
    }

    #[test]
    fn test_temperature_aggregation_over_time() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // First sample
        let sample1 = create_temp_sample(Some(5.0), Some(20.0));
        agg.new_temperatures(sample1, Timestamp { seconds: 1000 });
        
        // Second sample 60 seconds later
        let sample2 = create_temp_sample(Some(6.0), Some(22.0));
        agg.new_temperatures(sample2, Timestamp { seconds: 1060 });
        
        // Check vaccine temperature aggregation
        assert_eq!(agg.active_record.tvc_sum, 60.0 * 5.0); // 60 seconds * 5.0°C
        assert_eq!(agg.active_record.tvc_seconds, 60);
        assert_eq!(agg.active_record.tvc_min, 5.0);
        assert_eq!(agg.active_record.tvc_max, 6.0);
        
        // Check ambient temperature aggregation
        assert_eq!(agg.active_record.tamb_sum, 60.0 * 20.0); // 60 seconds * 20.0°C
        assert_eq!(agg.active_record.tamb_seconds, 60);
    }

    #[test]
    fn test_high_temperature_tracking() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Start with a high temperature
        let sample1 = create_temp_sample(Some(9.0), None);
        agg.new_temperatures(sample1, Timestamp { seconds: 1000 });
        
        // Continue with high temperature for 120 seconds
        let sample2 = create_temp_sample(Some(8.5), None);
        agg.new_temperatures(sample2, Timestamp { seconds: 1120 });
        
        // High temperature time should be tracked
        assert_eq!(agg.active_record.tvc_high_seconds, 120);
        assert_eq!(agg.active_record.tvc_low_seconds, 0);
    }

    #[test]
    fn test_low_temperature_tracking() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Start with a low temperature
        let sample1 = create_temp_sample(Some(1.5), None);
        agg.new_temperatures(sample1, Timestamp { seconds: 1000 });
        
        // Continue with low temperature for 90 seconds
        let sample2 = create_temp_sample(Some(1.0), None);
        agg.new_temperatures(sample2, Timestamp { seconds: 1090 });
        
        // Low temperature time should be tracked
        assert_eq!(agg.active_record.tvc_low_seconds, 90);
        assert_eq!(agg.active_record.tvc_high_seconds, 0);
    }

    #[test]
    fn test_mixed_temperature_ranges() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Normal temperature
        let sample1 = create_temp_sample(Some(5.0), None);
        agg.new_temperatures(sample1, Timestamp { seconds: 1000 });
        
        // High temperature for 60 seconds
        let sample2 = create_temp_sample(Some(9.0), None);
        agg.new_temperatures(sample2, Timestamp { seconds: 1060 });
        
        // Low temperature for 30 seconds
        let sample3 = create_temp_sample(Some(1.0), None);
        agg.new_temperatures(sample3, Timestamp { seconds: 1090 });
        
        // Back to normal
        let sample4 = create_temp_sample(Some(4.0), None);
        agg.new_temperatures(sample4, Timestamp { seconds: 1120 });
        
        assert_eq!(agg.active_record.tvc_high_seconds, 30); // Only the 9.0°C period
        assert_eq!(agg.active_record.tvc_low_seconds, 30); // Only the 1.0°C period
        assert_eq!(agg.active_record.tvc_seconds, 120); // Total time tracked
    }

    #[test]
    fn test_missing_vaccine_temperature() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // First valid sample
        let sample1 = create_temp_sample(Some(5.0), Some(20.0));
        agg.new_temperatures(sample1, Timestamp { seconds: 1000 });
        
        // Missing vaccine temperature - should still accumulate previous vaccine temp
        let sample2 = create_temp_sample(None, Some(22.0));
        agg.new_temperatures(sample2, Timestamp { seconds: 1060 });
        
        // Previous vaccine temp should be accumulated, ambient should continue
        assert_eq!(agg.active_record.tvc_sum, 60.0 * 5.0); // Previous vaccine temp accumulated
        assert_eq!(agg.active_record.tvc_seconds, 60);
        assert_eq!(agg.active_record.tamb_sum, 60.0 * 20.0);
        assert_eq!(agg.active_record.tamb_seconds, 60);
    }

    #[test]
    fn test_alarm_time_tracking() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Start high temperature alarm
        agg.alarm_expired(AlarmTimerExpired::HighTemperature, Timestamp { seconds: 1100 });
        assert_eq!(agg.high_alarm_ts, Some(Timestamp { seconds: 1100 }));
        
        // Process temperature sample 30 seconds later
        let sample = create_temp_sample(Some(9.0), None);
        agg.new_temperatures(sample, Timestamp { seconds: 1130 });
        
        // Should accumulate 30 seconds of alarm time
        assert_eq!(agg.active_record.high_alarm_seconds, 30);
        assert_eq!(agg.high_alarm_ts, Some(Timestamp { seconds: 1130 }));
    }

    #[test]
    fn test_alarm_transition_from_high_to_low() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Start high temperature alarm
        agg.alarm_expired(AlarmTimerExpired::HighTemperature, Timestamp { seconds: 1100 });
        
        // Accumulate some high alarm time
        let sample1 = create_temp_sample(Some(9.0), None);
        agg.new_temperatures(sample1, Timestamp { seconds: 1150 });
        
        // Transition to low temperature alarm
        agg.alarm_expired(AlarmTimerExpired::LowTemperature, Timestamp { seconds: 1200 });
        
        // High alarm should be cleared, low alarm should start
        assert_eq!(agg.high_alarm_ts, None);
        assert_eq!(agg.low_alarm_ts, Some(Timestamp { seconds: 1200 }));
        assert_eq!(agg.active_record.high_alarm_seconds, 100); // 50 + 50 seconds
    }

    #[test]
    fn test_cancel_temperature_alarms() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Set up both alarms
        agg.high_alarm_ts = Some(Timestamp { seconds: 1100 });
        agg.low_alarm_ts = Some(Timestamp { seconds: 1150 });
        
        agg.cancel_temperature_alarms();
        
        assert_eq!(agg.high_alarm_ts, None);
        assert_eq!(agg.low_alarm_ts, None);
    }

    #[test]
    fn test_min_max_temperature_tracking() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // First temperature sets initial min/max
        let sample1 = create_temp_sample(Some(5.0), None);
        agg.new_temperatures(sample1, Timestamp { seconds: 1000 });
        assert_eq!(agg.active_record.tvc_min, 5.0);
        assert_eq!(agg.active_record.tvc_max, 5.0);
        
        // Higher temperature updates max
        let sample2 = create_temp_sample(Some(7.0), None);
        agg.new_temperatures(sample2, Timestamp { seconds: 1060 });
        assert_eq!(agg.active_record.tvc_min, 5.0);
        assert_eq!(agg.active_record.tvc_max, 7.0);
        
        // Lower temperature updates min
        let sample3 = create_temp_sample(Some(3.0), None);
        agg.new_temperatures(sample3, Timestamp { seconds: 1120 });
        assert_eq!(agg.active_record.tvc_min, 3.0);
        assert_eq!(agg.active_record.tvc_max, 7.0);
    }

    #[test]
    fn test_weighted_average_calculation() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // First temperature for 60 seconds
        let sample1 = create_temp_sample(Some(4.0), Some(18.0));
        agg.new_temperatures(sample1, Timestamp { seconds: 1000 });
        
        let sample2 = create_temp_sample(Some(6.0), Some(22.0));
        agg.new_temperatures(sample2, Timestamp { seconds: 1060 });
        
        // Another 120 seconds at different temperature
        let sample3 = create_temp_sample(Some(8.0), Some(24.0));
        agg.new_temperatures(sample3, Timestamp { seconds: 1180 });
        
        // Verify weighted sums
        let expected_vaccine_sum = 60.0 * 4.0 + 120.0 * 6.0; // 240 + 720 = 960
        let expected_ambient_sum = 60.0 * 18.0 + 120.0 * 22.0; // 1080 + 2640 = 3720
        
        assert_eq!(agg.active_record.tvc_sum, expected_vaccine_sum);
        assert_eq!(agg.active_record.tamb_sum, expected_ambient_sum);
        assert_eq!(agg.active_record.tvc_seconds, 180);
        assert_eq!(agg.active_record.tamb_seconds, 180);
        
        // Verify calculated averages would be correct
        let vaccine_avg = agg.active_record.tvc_sum / agg.active_record.tvc_seconds as f32;
        let ambient_avg = agg.active_record.tamb_sum / agg.active_record.tamb_seconds as f32;
        assert!((vaccine_avg - 5.33).abs() < 0.01); // (4*60 + 6*120) / 180 ≈ 5.33
        assert!((ambient_avg - 20.67).abs() < 0.01); // (18*60 + 22*120) / 180 ≈ 20.67
    }

    #[test]
    fn test_alarm_time_accumulation() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Start high alarm
        agg.alarm_expired(AlarmTimerExpired::HighTemperature, Timestamp { seconds: 1100 });
        
        // Process samples to accumulate alarm time
        agg.new_temperatures(create_temp_sample(Some(9.0), None), Timestamp { seconds: 1150 });
        agg.new_temperatures(create_temp_sample(Some(8.8), None), Timestamp { seconds: 1200 });
        
        // Total alarm time should be 100 seconds (50 + 50)
        assert_eq!(agg.active_record.high_alarm_seconds, 100);
        
        // Cancel alarms and verify no further accumulation
        agg.cancel_temperature_alarms();
        agg.new_temperatures(create_temp_sample(Some(5.0), None), Timestamp { seconds: 1300 });
        
        assert_eq!(agg.active_record.high_alarm_seconds, 100); // Should not change
    }

    #[test]
    fn test_low_alarm_accumulation() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Start low alarm
        agg.alarm_expired(AlarmTimerExpired::LowTemperature, Timestamp { seconds: 1200 });
        
        // Accumulate alarm time over multiple samples
        agg.new_temperatures(create_temp_sample(Some(-1.0), None), Timestamp { seconds: 1260 });
        agg.new_temperatures(create_temp_sample(Some(-0.8), None), Timestamp { seconds: 1320 });
        
        assert_eq!(agg.active_record.low_alarm_seconds, 120); // 60 + 60 seconds
    }

    #[test]
    fn test_temperature_range_boundaries() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Test exactly at boundaries
        let sample1 = create_temp_sample(Some(8.0), None); // Exactly at MAX_GOOD_VACCINE_TEMP
        agg.new_temperatures(sample1, Timestamp { seconds: 1000 });
        
        let sample2 = create_temp_sample(Some(2.0), None); // Exactly at MIN_GOOD_VACCINE_TEMP
        agg.new_temperatures(sample2, Timestamp { seconds: 1060 });
        
        // 8.0 should be counted as high (>=), 2.0 should not be counted as low (<)
        assert_eq!(agg.active_record.tvc_high_seconds, 60);
        assert_eq!(agg.active_record.tvc_low_seconds, 0);
    }

    #[test]
    fn test_interleaved_missing_samples() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Valid sample
        agg.new_temperatures(create_temp_sample(Some(5.0), Some(20.0)), Timestamp { seconds: 1000 });
        
        // Missing vaccine, valid ambient
        agg.new_temperatures(create_temp_sample(None, Some(21.0)), Timestamp { seconds: 1060 });
        
        // Valid vaccine, missing ambient
        agg.new_temperatures(create_temp_sample(Some(6.0), None), Timestamp { seconds: 1120 });
        
        // Previous vaccine temp accumulated for first interval, ambient for both intervals
        assert_eq!(agg.active_record.tamb_sum, 60.0 * 20.0 + 60.0 * 21.0); // Both intervals
        assert_eq!(agg.active_record.tamb_seconds, 120);
        assert_eq!(agg.active_record.tvc_sum, 60.0 * 5.0); // First interval only
        assert_eq!(agg.active_record.tvc_seconds, 60);
    }

    #[test]
    fn test_alarm_state_transitions() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Start with high alarm
        agg.alarm_expired(AlarmTimerExpired::HighTemperature, Timestamp { seconds: 1100 });
        agg.new_temperatures(create_temp_sample(Some(9.0), None), Timestamp { seconds: 1150 });
        
        // Transition to low alarm (should clear high alarm)
        agg.alarm_expired(AlarmTimerExpired::LowTemperature, Timestamp { seconds: 1200 });
        
        assert_eq!(agg.high_alarm_ts, None);
        assert_eq!(agg.low_alarm_ts, Some(Timestamp { seconds: 1200 }));
        assert_eq!(agg.active_record.high_alarm_seconds, 100); // Accumulated before transition
        
        // Continue with low alarm
        agg.new_temperatures(create_temp_sample(Some(-1.0), None), Timestamp { seconds: 1260 });
        
        assert_eq!(agg.active_record.low_alarm_seconds, 60);
    }

    #[test]
    fn test_temperature_aggregation_with_gaps() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // First valid sample
        agg.new_temperatures(create_temp_sample(Some(5.0), Some(20.0)), Timestamp { seconds: 1000 });
        
        // Gap with no vaccine temperature
        agg.new_temperatures(create_temp_sample(None, Some(21.0)), Timestamp { seconds: 1060 });
        
        // Resume vaccine temperature
        agg.new_temperatures(create_temp_sample(Some(6.0), Some(22.0)), Timestamp { seconds: 1120 });
        
        // Continue normally
        agg.new_temperatures(create_temp_sample(Some(7.0), Some(23.0)), Timestamp { seconds: 1180 });
        
        // Vaccine accumulates: 60s*5.0 + 60s*6.0 = 300 + 360 = 660
        assert_eq!(agg.active_record.tvc_sum, 60.0 * 5.0 + 60.0 * 6.0);
        assert_eq!(agg.active_record.tvc_seconds, 120);
        
        // Ambient should accumulate for all intervals
        assert_eq!(agg.active_record.tamb_sum, 60.0 * 20.0 + 60.0 * 21.0 + 60.0 * 22.0);
        assert_eq!(agg.active_record.tamb_seconds, 180);
    }

    // Tests for check_for_end_of_record() method
    
    #[test]
    fn test_check_for_end_of_record_not_ready() {
        let mut agg = Aggregator::new(Timestamp { seconds: 1000 });
        
        // Time before next_record_start should return false
        let before_end = Timestamp { seconds: agg.next_record_start.seconds - 100 };
        let result = agg.check_for_end_of_record(before_end);
        
        assert!(!result);
        // State should be unchanged
        assert_eq!(agg.timestamp, Timestamp { seconds: 1000 });
    }

    #[test]
    fn test_check_for_end_of_record_finalizes_to_record_end() {
        let start_time = Timestamp { seconds: 1000 };
        let mut agg = Aggregator::new(start_time);
        let record_end = agg.next_record_start;
        
        // Set up temperatures that started before record end
        agg.last_vaccine_temp = Some(6.0);
        agg.last_vaccine_ts = Some(Timestamp { seconds: record_end.seconds - 300 });
        agg.last_ambient_temp = Some(22.0);
        agg.last_ambient_ts = Some(Timestamp { seconds: record_end.seconds - 200 });
        
        // Trigger at exact record end time
        let result = agg.check_for_end_of_record(record_end);
        
        assert!(result);
        // Should accumulate temps to exact record end, not current timestamp
        assert_eq!(agg.prev_record.tvc_sum, 300.0 * 6.0); // 300 seconds to record end
        assert_eq!(agg.prev_record.tamb_sum, 200.0 * 22.0); // 200 seconds to record end
    }

    #[test]
    fn test_end_of_record_finalizes_alarm_times_to_record_end() {
        let start_time = Timestamp { seconds: 1000 };
        let mut agg = Aggregator::new(start_time);
        let record_end = agg.next_record_start;
        
        // Set up active alarms before record end
        agg.high_alarm_ts = Some(Timestamp { seconds: record_end.seconds - 500 });
        agg.low_alarm_ts = Some(Timestamp { seconds: record_end.seconds - 300 });
        
        agg.check_for_end_of_record(record_end);
        
        // Should accumulate alarm time to exact record end
        assert_eq!(agg.prev_record.high_alarm_seconds, 500);
        assert_eq!(agg.prev_record.low_alarm_seconds, 300);
        
        // Alarm timestamps should be updated to new record start
        assert_eq!(agg.high_alarm_ts, Some(record_end));
        assert_eq!(agg.low_alarm_ts, Some(record_end));
    }

    #[test]
    fn test_end_of_record_with_high_low_temperature_finalization() {
        let start_time = Timestamp { seconds: 1000 };
        let mut agg = Aggregator::new(start_time);
        let record_end = agg.next_record_start;
        
        // High temperature active until record end
        agg.last_vaccine_temp = Some(9.0);
        agg.last_vaccine_ts = Some(Timestamp { seconds: record_end.seconds - 600 });
        agg.active_record.tvc_high_seconds = 100; // Already accumulated time
        
        agg.check_for_end_of_record(record_end);
        
        // Should add final 600 seconds of high temp time
        assert_eq!(agg.prev_record.tvc_high_seconds, 700); // 100 + 600
        
        // Test low temperature finalization
        let mut agg2 = Aggregator::new(start_time);
        agg2.last_vaccine_temp = Some(1.0);
        agg2.last_vaccine_ts = Some(Timestamp { seconds: record_end.seconds - 400 });
        agg2.active_record.tvc_low_seconds = 200;
        
        agg2.check_for_end_of_record(record_end);
        
        assert_eq!(agg2.prev_record.tvc_low_seconds, 600); // 200 + 400
    }

    #[test]
    fn test_record_length_calculation() {
        let start_time = Timestamp { seconds: 1000 };
        let mut agg = Aggregator::new(start_time);
        let record_end = agg.next_record_start;
        
        agg.check_for_end_of_record(record_end);
        
        let expected_length = (record_end.seconds - start_time.seconds) as u16;
        assert_eq!(agg.prev_record.record_length_seconds, expected_length);
        assert_eq!(agg.prev_record.record_start, start_time);
    }

    #[test]
    fn test_new_record_initialization_after_rollover() {
        let start_time = Timestamp { seconds: 1000 };
        let mut agg = Aggregator::new(start_time);
        let first_end = agg.next_record_start;
        let second_end = first_end.get_next_aggregation_start();
        
        // Add some data to first record
        agg.active_record.tvc_sum = 500.0;
        agg.active_record.tamb_sum = 300.0;
        
        agg.check_for_end_of_record(first_end);
        
        // New record should be properly initialized
        assert_eq!(agg.active_record.record_start, first_end);
        assert_eq!(agg.next_record_start, second_end);
        assert_eq!(agg.timestamp, first_end);
        assert_eq!(agg.active_record.tvc_sum, 0.0);
        assert_eq!(agg.active_record.tamb_sum, 0.0);
        assert_eq!(agg.active_record.tvc_seconds, 0);
        assert_eq!(agg.active_record.tamb_seconds, 0);
    }

    #[test]
    fn test_end_of_record_triggered_by_new_temperatures() {
        let start_time = Timestamp { seconds: 1000 };
        let mut agg = Aggregator::new(start_time);
        let record_end = agg.next_record_start;
        
        // Add temperature before record end
        agg.new_temperatures(create_temp_sample(Some(5.0), Some(20.0)), Timestamp { seconds: 1500 });
        
        // Process sample at record end time - should trigger rollover
        let ready = agg.new_temperatures(create_temp_sample(Some(6.0), Some(21.0)), record_end);
        
        assert!(ready);
        // Previous record should contain the accumulated data
        assert!(agg.prev_record.tvc_sum > 0.0);
        assert!(agg.prev_record.tamb_sum > 0.0);
    }
}