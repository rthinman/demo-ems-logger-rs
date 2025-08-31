// //! This module contains the business logic for handling door events, including
// //! tracking door open/close events, accumulating open durations, and managing alarms.

// use crate::timestamp::{Timestamp, TimestampError};

// const DOOR_ALARM_THRESHOLD: u32 = 300;    // 5 minutes in seconds.

// // TODO: rework to have functions that get and reset accumulators all at once.  Need to decide how
// //       to handle the input timestamp: 
// //       If it is before prev_x_sample_ended + X_SAMPLE_PERIOD, do we advance to that one?
// //       At it exactly is the ideal case.
// //       If it is after prev_x_sample_ended + X_SAMPLE_PERIOD, do we just output for the one sample period?

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DoorEvent {
    Opened,
    Closed,
    // Opened(Timestamp), // Timestamp when the door was opened.
    // Closed(Timestamp), // Timestamp when the door was closed.
}

// #[derive(Debug, Clone, Copy, PartialEq)]
// pub struct Door {
//     opened: Option<Timestamp>, // Timestamp when the door was opened, None if closed.
//     last_event_ts: Timestamp, // Timestamp of the last event or reset.
//     prev_short_sample_ended: Timestamp, // Timestamp the previous short sampling period ended. (ideally, 0, 900, 1800...)
//     prev_long_sample_ended: Timestamp, // Timestamp the previous long sampling period ended.
//     short_open_count: u16, // Count of openings for short sampling.
//     short_open_accum: u32, // Accumulated time for short sampling.
//     long_open_count: u16, // Count of openings for long sampling.
//     long_open_accum: u32, // Accumulated time for long sampling.
//     short_alarmed: bool, // Flag indicating if the short sample alarm was triggered.
//     long_alarmed: bool, // Flag indicating if the long sample alarm was triggered.
// }

// impl Default for Door {
//     fn default() -> Self {
//         Self {
//             opened: None,
//             last_event_ts: Timestamp { seconds: 0 },
//             prev_short_sample_ended: Timestamp { seconds: 0 },
//             prev_long_sample_ended: Timestamp { seconds: 0 },
//             short_open_count: 0,
//             short_open_accum: 0,
//             long_open_count: 0,
//             long_open_accum: 0,
//             short_alarmed: false,
//             long_alarmed: false,
//         }
//     }
// }

// impl Door {
//     /// Create a new Door instance with the current door state.
//     pub fn new(open: bool, now: Timestamp) -> Self {
//         // Initialize the "sample_ended" trackers with the last instance before now.
//         let sample_ts = now.get_last_short_sample_end();
//         let prev_long_sample_ended = now.get_last_long_sample_end();
//         // Assume the door was opened now.
//         let opened = if open {
//             Some(now)
//         } else {
//             None
//         };
//         let open_count = if open { 1 } else { 0 };

//         Self {
//             opened: opened,
//             last_event_ts: now,
//             prev_short_sample_ended: sample_ts,
//             prev_long_sample_ended: prev_long_sample_ended,
//             short_open_count: open_count,
//             short_open_accum: 0,
//             long_open_count: open_count,
//             long_open_accum: 0,
//             short_alarmed: false,
//             long_alarmed: false,
//         }
//     }

//     pub fn log_event(&mut self, event: DoorEvent, timestamp: Timestamp) -> Result<(), TimestampError> {
//         // Validate the event timestamp.
//         // let timestamp = match event {
//         //     DoorEvent::Opened(ts) => ts,
//         //     DoorEvent::Closed(ts) => ts,
//         // };
//         self.validate_timestamp_and_update(&timestamp)?;
        
//         // TODO: Handle the case where the door is opened while it is already open, and vice versa.
//         match event {
//             DoorEvent::Opened => {
//                 self.opened = Some(timestamp);
//                 // Counts incremented when the door is opened.
//                 self.short_open_count += 1;
//                 self.long_open_count += 1;
//             }
//             DoorEvent::Closed => {
//                 if let Some(opened) = self.opened.take() {
//                     let duration = timestamp.seconds - opened.seconds;
//                     // Open durations are accumulated when the door is closed.
//                     self.short_open_accum += duration;
//                     self.long_open_accum += duration;
//                     // Check if the short sample alarm should be triggered.
//                     self.short_alarmed = duration >= DOOR_ALARM_THRESHOLD;
//                     self.long_alarmed = duration >= DOOR_ALARM_THRESHOLD;
//                 }
//             }
//         }
//         Ok(())
//     }

//     pub fn get_short_sample(&self, check_time: Timestamp) -> (u16, u32) {
//         // TODO: Consider better error handling for check_time earlier than opened
//         let presently_open_seconds = if let Some(opened) = self.opened {
//             if check_time.seconds < opened.seconds {
//                 0 // If check_time is before the door was opened, return 0.
//             } else {
//                 check_time.seconds - opened.seconds
//             }
//         } else {
//             0
//         };

//         (self.short_open_count, self.short_open_accum + presently_open_seconds)
//     }

//     pub fn get_long_sample(&self, check_time: Timestamp) -> (u16, u32) {
//         // TODO: Consider better error handling for check_time earlier than opened
//         let presently_open_seconds = if let Some(opened) = self.opened {
//             if check_time.seconds < opened.seconds {
//                 0 // If check_time is before the door was opened, return 0.
//             } else {
//                 check_time.seconds - opened.seconds
//             }
//         } else {
//             0
//         };

//         (self.long_open_count, self.long_open_accum + presently_open_seconds)
//     }       

//     pub fn reset_short_sample(&mut self, timestamp: Timestamp) -> Result<(), TimestampError> {
//         self.validate_timestamp_and_update(&timestamp)?;
//         self.prev_short_sample_ended = timestamp;
//         self.short_open_count = 0;
//         self.short_open_accum = 0;
//         self.short_alarmed = false;
//         Ok(())
//     }

//     pub fn reset_long_sample(&mut self, timestamp: Timestamp) -> Result<(), TimestampError> {
//         self.validate_timestamp_and_update(&timestamp)?;
//         self.prev_long_sample_ended = timestamp;
//         self.long_open_count = 0;
//         self.long_open_accum = 0;
//         self.long_alarmed = false;
//         Ok(())
//     }

//     pub fn is_short_sample_alarmed(&self, now: Timestamp) -> bool {
//         // If the door is currently open, check if it has been open longer than the threshold.
//         let open_long = if let Some(opened) = self.opened {
//             now.seconds > opened.seconds + DOOR_ALARM_THRESHOLD
//         } else {
//             false
//         };

//         // True if there was an alarm that was canceled
//         self.short_alarmed || open_long
//     }

//     pub fn is_long_sample_alarmed(&self, now: Timestamp) -> bool {
//         // If the door is currently open, check if it has been open longer than the threshold.
//         let open_long = if let Some(opened) = self.opened {
//             now.seconds > opened.seconds + DOOR_ALARM_THRESHOLD
//         } else {
//             false
//         };

//         // True if there was an alarm that was canceled
//         self.long_alarmed || open_long
//     }

//     pub fn get_idrv(&self, now: Timestamp) -> u32 {
//         // Return the duration the door has been open at this instant.
//         if let Some(opened) = self.opened {
//             if now.seconds >= opened.seconds {
//                 now.seconds - opened.seconds
//             } else {
//                 0 // If the current time is before the door was opened, return 0.
//             }
//         } else {
//             0 // If the door is not open, return 0.
//         }
//     }

//     fn validate_timestamp_and_update(&mut self, timestamp: &Timestamp) -> Result<(), TimestampError> {
//         if timestamp.seconds < self.last_event_ts.seconds {
//             return Err(TimestampError::OutOfOrder);
//         }
//         self.last_event_ts = *timestamp;
//         Ok(())
//     }
    
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_door_event_logging() {
//         let mut door = Door::default();
//         let open_time = Timestamp { seconds: 1000 };
//         let close_time = Timestamp { seconds: 1200 };

//         door.log_event(DoorEvent::Opened, open_time).unwrap();
//         assert_eq!(door.opened, Some(open_time));
//         assert_eq!(door.short_open_count, 1);
//         assert_eq!(door.long_open_count, 1);

//         door.log_event(DoorEvent::Closed, close_time).unwrap();
//         assert_eq!(door.short_open_accum, 200);
//         assert_eq!(door.long_open_accum, 200);

//         // Out-of-order event: try to open at a time before the last event
//         let out_of_order_time = Timestamp { seconds: 100 };
//         let result = door.log_event(DoorEvent::Opened(out_of_order_time));
//         assert!(matches!(result, Err(TimestampError::OutOfOrder)));

//         // Open and close again.
//         let open_time = Timestamp { seconds: 1500 };
//         let close_time = Timestamp { seconds: 1800 };

//         door.log_event(DoorEvent::Closed, close_time).unwrap();
//         assert_eq!(door.opened, Some(open_time));
//         assert_eq!(door.short_open_count, 2);
//         assert_eq!(door.long_open_count, 2);

//         door.log_event(DoorEvent::Closed, close_time).unwrap();
//         assert_eq!(door.short_open_accum, 500);
//         assert_eq!(door.long_open_accum, 500);

//     }

//     #[test]
//     fn test_door_alarm_threshold() {
//         let mut door = Door::default();
//         let open_time = Timestamp { seconds: 1000 };
//         let close_time = Timestamp { seconds: 1200 };

//         door.log_event(DoorEvent::Closed, close_time).unwrap();
//         assert!(!door.is_short_sample_alarmed(close_time));
//         assert!(!door.is_long_sample_alarmed(close_time));

//         // Shorter than the alarm duration, should not alarm.
//         door.log_event(DoorEvent::Closed, close_time).unwrap();
//         assert!(!door.is_short_sample_alarmed(close_time));
//         assert!(!door.is_long_sample_alarmed(close_time));

//         // Now, let's exceed the threshold.
//         let open_time = Timestamp { seconds: 1500 };
//         let check_time = Timestamp { seconds: 1800 };
//         let close_time = Timestamp { seconds: 1801 };
//         let check_time2 = Timestamp { seconds: 1802 };


//         // Exactly at the threshold, should not alarm.
//         door.log_event(DoorEvent::Closed, close_time).unwrap();
//         assert!(!door.is_short_sample_alarmed(check_time));
//         assert!(!door.is_long_sample_alarmed(check_time));

//         // Simulate a long open duration.
//         //Check before closing; should alarm.
//         assert!(door.is_short_sample_alarmed(close_time));
//         assert!(door.is_long_sample_alarmed(close_time));

//         // Close the door after exceeding the threshold; should still alarm.
//         door.log_event(DoorEvent::Closed, close_time).unwrap();
//         assert!(door.is_short_sample_alarmed(check_time2));
//         assert!(door.is_long_sample_alarmed(check_time2));
//     }

//     #[test]
//     fn test_open_close_reset() {
//         let mut door = Door::default();
//         let open_time = Timestamp { seconds: 450 };
//         let close_time = Timestamp { seconds: 600 };
//         let check_time = Timestamp { seconds: 900 };

//         // Open and close the door
//         door.log_event(DoorEvent::Closed, close_time).unwrap();
//         door.log_event(DoorEvent::Closed, close_time).unwrap();

//         // Get short sample values at 900 seconds
//         let (short_count, short_accum) = door.get_short_sample(check_time);
//         assert_eq!(short_count, 1);
//         assert_eq!(short_accum, 150);
//         assert!(!door.is_short_sample_alarmed(check_time));

//         // Get long sample values at 900 seconds
//         let (long_count, long_accum) = door.get_long_sample(check_time);
//         assert_eq!(long_count, 1);
//         assert_eq!(long_accum, 150);
//         assert!(!door.is_long_sample_alarmed(check_time));

//         // Reset short sample at 900 seconds, should not affect long sample.
//         door.reset_short_sample(check_time);
//         assert_eq!(door.short_open_count, 0);
//         assert_eq!(door.short_open_accum, 0);
//         assert_eq!(door.prev_short_sample_ended, check_time);
//         assert_eq!(door.long_open_count, 1);
//         assert_eq!(door.long_open_accum, 150);
//         assert_eq!(door.prev_long_sample_ended, Timestamp { seconds: 0 });

//         // Reset long sample at 900 seconds
//         door.reset_long_sample(check_time);
//         assert_eq!(door.long_open_count, 0);
//         assert_eq!(door.long_open_accum, 0);
//         assert_eq!(door.prev_long_sample_ended, check_time);
//     }
    
//     #[test]
//     fn test_init_open_close_reset() {
//         let init_time = Timestamp { seconds: 901 };
//         let mut door = Door::new(false, init_time);
//         let open_time = Timestamp { seconds: 1300 };
//         let check_while_open = Timestamp { seconds: 1400 };
//         let close_time = Timestamp { seconds: 1700 };
//         let check_time = Timestamp { seconds: 1800 };

//         // Ensure the initial state is correct
//         assert_eq!(door.prev_short_sample_ended.seconds, 900);
//         assert_eq!(door.prev_long_sample_ended.seconds, 0);

//         // Open and close the door
//         door.log_event(DoorEvent::Closed, close_time).unwrap();
//         let (short_count, short_accum) = door.get_short_sample(check_while_open);
//         assert_eq!(short_count, 1);
//         assert_eq!(short_accum, 100); // 1400 - 1300 = 100 seconds while open
//         assert!(!door.is_short_sample_alarmed(check_while_open));

//         // Long sample checks while open
//         let (long_count, long_accum) = door.get_long_sample(check_while_open);
//         assert_eq!(long_count, 1);
//         assert_eq!(long_accum, 100);
//         assert!(!door.is_long_sample_alarmed(check_while_open));

//         door.log_event(DoorEvent::Closed, close_time).unwrap();

//         // Get short sample values at 1800 seconds
//         let (short_count, short_accum) = door.get_short_sample(check_time);
//         assert_eq!(short_count, 1);
//         assert_eq!(short_accum, 400);
//         assert!(door.is_short_sample_alarmed(check_time));

//         // Long sample checks after close
//         let (long_count, long_accum) = door.get_long_sample(check_time);
//         assert_eq!(long_count, 1);
//         assert_eq!(long_accum, 400);
//         assert!(door.is_long_sample_alarmed(check_time));

//         // Reset short sample at 1800 seconds, which should not affect long sample.
//         door.reset_short_sample(check_time).unwrap();
//         assert_eq!(door.short_open_count, 0);
//         assert_eq!(door.short_open_accum, 0);
//         assert_eq!(door.prev_short_sample_ended, check_time);
//         assert_eq!(long_count, 1);
//         assert_eq!(long_accum, 400);

//         // Reset long sample at 1800 seconds
//         door.reset_long_sample(check_time).unwrap();
//         assert_eq!(door.long_open_count, 0);
//         assert_eq!(door.long_open_accum, 0);
//         assert_eq!(door.prev_long_sample_ended, check_time);
//     }

//     #[test]
//     fn test_init_while_open() {
//         // Test the case where the door is initialized while open
//         let init_time = Timestamp { seconds: 901 };
//         let mut door = Door::new(true, init_time);
//         let check_while_open = Timestamp { seconds: 1400 };
//         let close_time = Timestamp { seconds: 1700 };
//         let check_time = Timestamp { seconds: 1800 };

//         // Ensure the initial state is correct
//         assert_eq!(door.prev_short_sample_ended.seconds, 900);
//         assert_eq!(door.prev_long_sample_ended.seconds, 0);

//         // Get short sample values at 1400 seconds
//         let (short_count, short_accum) = door.get_short_sample(check_while_open);
//         assert_eq!(short_count, 1);
//         assert_eq!(short_accum, 499);
//         assert!(door.is_short_sample_alarmed(check_time));

//         // Long sample checks while open
//         let (long_count, long_accum) = door.get_long_sample(check_while_open);
//         assert_eq!(long_count, 1);
//         assert_eq!(long_accum, 499);
//         assert!(door.is_long_sample_alarmed(check_time));

//         // Close the door, then check at 1800 seconds when the door has been open for 799 sec.
//         door.log_event(DoorEvent::Closed, close_time).unwrap();
//         let (short_count, short_accum) = door.get_short_sample(check_time);
//         assert_eq!(short_count, 1);
//         assert_eq!(short_accum, 799);
//         assert!(door.is_short_sample_alarmed(check_while_open));

//         // Long sample checks after close
//         let (long_count, long_accum) = door.get_long_sample(check_time);
//         assert_eq!(long_count, 1);
//         assert_eq!(long_accum, 799);
//         assert!(door.is_long_sample_alarmed(check_while_open));
//     }

//     #[test]
//     fn test_out_of_order_reset() {
//         let mut door = Door::default();
//         let open_time = Timestamp { seconds: 700 };
//         let close_time = Timestamp { seconds: 800 };
//         let reset_time = Timestamp { seconds: 900 };

//         // Open the door
//         door.log_event(DoorEvent::Closed, close_time).unwrap();


//         // Try to reset with an out-of-order timestamp
//         let out_of_order_reset = Timestamp { seconds: 600 };
//         assert!(matches!(door.reset_short_sample(out_of_order_reset), Err(TimestampError::OutOfOrder)));

//         // Close the door
//         door.log_event(DoorEvent::Closed, close_time).unwrap();

//         // Reset short sample with a valid timestamp
//         assert!(door.reset_short_sample(reset_time).is_ok());
//         assert_eq!(door.prev_short_sample_ended, reset_time);
        
//         // Try to reset with an out-of-order timestamp
//         assert!(matches!(door.reset_short_sample(out_of_order_reset), Err(TimestampError::OutOfOrder)));
//     }

//     #[test]
//     fn test_get_idrv() {
//         let mut door = Door::default();
//         let open_time = Timestamp { seconds: 100 };
//         let check_time1 = Timestamp { seconds: 300 };
//         let reset_time = Timestamp { seconds: 900 };
//         let check_time2 = Timestamp { seconds: 1200 };
//         let close_time = Timestamp { seconds: 1300 };
//         let check_time3 = Timestamp { seconds: 1400 };

//         // Open the door
//         door.log_event(DoorEvent::Closed, close_time).unwrap();

//         // Get IDRV at 1200 seconds, should return 200 (1200 - 1000)
//         assert_eq!(door.get_idrv(check_time1), 200);

//         // Reset the short sample
//         door.reset_short_sample(reset_time).unwrap();

//         // Get IDRV after reset, should return 1100.
//         assert_eq!(door.get_idrv(check_time2), 1100);

//         // Close the door
//         door.log_event(DoorEvent::Closed, close_time).unwrap();

//         // Get IDRV after closing, should return 0
//         assert_eq!(door.get_idrv(check_time3), 0);
//     }

//     #[test]
//     fn test_multiple_brief_openings() {
//         let mut door = Door::default();
//         let open_time1 = Timestamp { seconds: 1000 };
//         let close_time1 = Timestamp { seconds: 1150 };
//         let open_time2 = Timestamp { seconds: 1200 };
//         let check_time2 = Timestamp { seconds: 1300 };
//         let close_time2 = Timestamp { seconds: 1450 };
//         // let reset_time = Timestamp { seconds: 1500 };

//         // Open the door for the first time
//         door.log_event(DoorEvent::Opened(open_time1)).unwrap();
//         door.log_event(DoorEvent::Closed(close_time1)).unwrap();
//         assert!(!door.is_short_sample_alarmed(close_time1));
//         assert!(!door.is_long_sample_alarmed(close_time1)); // 100 sec, no problem.

//         // Open the door for the second time
//         door.log_event(DoorEvent::Opened(open_time2)).unwrap();
//         assert!(!door.is_short_sample_alarmed(check_time2));
//         assert!(!door.is_long_sample_alarmed(check_time2)); // Also 100 sec.
//         door.log_event(DoorEvent::Closed(close_time2)).unwrap(); // Close after 250 seconds, should not alarm
//         assert!(!door.is_short_sample_alarmed(close_time2));
//         assert!(!door.is_long_sample_alarmed(close_time2));


//     }        
// }

