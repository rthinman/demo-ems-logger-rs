// //! This module contains the business logic for handling door events, including
// //! tracking door open/close events, accumulating open durations, and managing alarms.

use crate::logger::{AlarmTimerExpired, AlarmTimerTrigger};
use crate::timestamp::Timestamp;

// // TODO: rework to have functions that get and reset accumulators all at once?  

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DoorEvent {
    Closed,
    Opened,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum DoorState {
    #[default]
    Closed,
    OpenNoAlarm(Timestamp), // Timestamp when the door was opened, which could be before the present sample period.
    OpenAlarm(Timestamp), // Timestamp when the door was closed.
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Door {
    status: DoorState, 
    open_count: u16, // Count of opening in this sample period.
    open_accum: u32, // Accumulated open time in this sample period, in seconds.
    new_alarm: bool, // Flag indicating if the alarm was triggered in this sample period.
}

impl Door {
    /// Create a new Door instance with the current door state.
    /// Note that if the door is open, the caller should also trigger the alarm timer.
    pub fn new(open: bool, now: Timestamp) -> Self {
        // Assume the door was opened now.
        let opened = if open {
            DoorState::OpenNoAlarm(now)
        } else {
            DoorState::Closed
        };
        let open_count = if open { 1 } else { 0 };

        Self {
            status: opened,
            open_count: open_count,
            open_accum: 0,
            new_alarm: false,
        }
    }

    pub fn door_event(&mut self, event: DoorEvent, now: Timestamp) -> AlarmTimerTrigger {
        
        // TODO: Handle the case where the door is opened while it is already open?
        match event {
            DoorEvent::Opened => {
                self.status = DoorState::OpenNoAlarm(now);
                // Counts incremented when the door is opened.
                self.open_count += 1;

                // Trigger the alarm timer.
                AlarmTimerTrigger::DoorOpenStart
            }
            DoorEvent::Closed => {
                // If the door was open, accumulate the open duration.
                match self.status {
                    DoorState::Closed => {
                        // Door was already closed, do nothing.
                    }
                    DoorState::OpenAlarm(opened) | DoorState::OpenNoAlarm(opened) => {
                        let duration = now.seconds - opened.seconds;
                        self.open_accum += duration;
                    }
                }
                // Set door status to closed
                self.status = DoorState::Closed;
                // Cancel the alarm timer.
                AlarmTimerTrigger::DoorOpenCancel
            }
        }
    }

    pub fn alarm_expired(&mut self, event: AlarmTimerExpired) {
        if event == AlarmTimerExpired::Door {
            self.new_alarm = true;
            match self.status {
                DoorState::Closed => {
                    // This should not happen, but just in case, do nothing.
                }
                DoorState::OpenNoAlarm(opened) => {
                    self.status = DoorState::OpenAlarm(opened);
                }
                DoorState::OpenAlarm(_) => {
                    // Already in alarm state, do nothing.
                }
            }
        }
    }

    /// Returns true if a door alarm is active or a new alarm was
    /// triggered but then resolved.
    pub fn is_alarm(&self) -> bool {
        let currently_alarming = match self.status {
            DoorState::OpenAlarm(_) => true,
            _ => false,
        };
        currently_alarming || self.new_alarm
    }

    pub fn clear_new_alarms(&mut self) {
        self.new_alarm = false;
    }

    pub fn get_values(&self, now: Timestamp) -> (u16, u32,u32) {
        // Return the door open count, accumulated open duration, and current open duration.
        let idrv = self.get_idrv(now);
        (self.open_count, self.open_accum, idrv)
    }

    pub fn get_idrv(&self, now: Timestamp) -> u32 {
        // Return the duration the door has been open at this instant.
        match self.status {
            DoorState::Closed => 0,
            DoorState::OpenNoAlarm(opened) | DoorState::OpenAlarm(opened) => {
                if now.seconds >= opened.seconds {
                    now.seconds - opened.seconds
                } else {
                    0 // If the current time is before the door was opened, return 0.
                }
            }
        }
    }

    pub fn reset_accumulators(&mut self) {
        self.open_count = 0;
        self.open_accum = 0;
        self.new_alarm = false;
    }
    
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::AlarmTimerTrigger;

    #[test]
    fn test_new_door_closed() {
        let now = Timestamp { seconds: 1000 };
        let door = Door::new(false, now);
        
        assert_eq!(door.status, DoorState::Closed);
        assert_eq!(door.open_count, 0);
        assert_eq!(door.open_accum, 0);
        assert!(!door.new_alarm);
        assert!(!door.is_alarm());
    }

    #[test]
    fn test_new_door_open() {
        let now = Timestamp { seconds: 1000 };
        let door = Door::new(true, now);
        
        assert_eq!(door.status, DoorState::OpenNoAlarm(now));
        assert_eq!(door.open_count, 1);
        assert_eq!(door.open_accum, 0);
        assert!(!door.new_alarm);
        assert!(!door.is_alarm());
    }

    #[test]
    fn test_door_open_event() {
        let now = Timestamp { seconds: 1000 };
        let mut door = Door::new(false, now);
        
        let trigger = door.door_event(DoorEvent::Opened, now);
        
        assert_eq!(trigger, AlarmTimerTrigger::DoorOpenStart);
        assert_eq!(door.status, DoorState::OpenNoAlarm(now));
        assert_eq!(door.open_count, 1);
        assert_eq!(door.open_accum, 0);
    }

    #[test]
    fn test_door_close_event() {
        let open_time = Timestamp { seconds: 1000 };
        let close_time = Timestamp { seconds: 1300 };
        let mut door = Door::new(true, open_time);
        
        let trigger = door.door_event(DoorEvent::Closed, close_time);
        
        assert_eq!(trigger, AlarmTimerTrigger::DoorOpenCancel);
        assert_eq!(door.status, DoorState::Closed);
        assert_eq!(door.open_count, 1);
        assert_eq!(door.open_accum, 300); // 300 seconds open
    }

    #[test]
    fn test_multiple_open_close_cycles() {
        let mut door = Door::new(false, Timestamp { seconds: 1000 });
        
        // First open
        door.door_event(DoorEvent::Opened, Timestamp { seconds: 1100 });
        assert_eq!(door.open_count, 1);
        
        // First close after 60 seconds
        door.door_event(DoorEvent::Closed, Timestamp { seconds: 1160 });
        assert_eq!(door.open_accum, 60);
        
        // Second open
        door.door_event(DoorEvent::Opened, Timestamp { seconds: 1200 });
        assert_eq!(door.open_count, 2);
        
        // Second close after 90 seconds
        door.door_event(DoorEvent::Closed, Timestamp { seconds: 1290 });
        assert_eq!(door.open_accum, 150); // 60 + 90
    }

    #[test]
    fn test_close_already_closed_door() {
        let mut door = Door::new(false, Timestamp { seconds: 1000 });
        
        let trigger = door.door_event(DoorEvent::Closed, Timestamp { seconds: 1100 });
        
        assert_eq!(trigger, AlarmTimerTrigger::DoorOpenCancel);
        assert_eq!(door.status, DoorState::Closed);
        assert_eq!(door.open_count, 0);
        assert_eq!(door.open_accum, 0);
    }

    #[test]
    fn test_alarm_expired_when_open() {
        let open_time = Timestamp { seconds: 1000 };
        let mut door = Door::new(true, open_time);
        
        door.alarm_expired(AlarmTimerExpired::Door);
        
        assert_eq!(door.status, DoorState::OpenAlarm(open_time));
        assert!(door.new_alarm);
        assert!(door.is_alarm());
    }

    #[test]
    fn test_alarm_expired_when_closed() {
        let mut door = Door::new(false, Timestamp { seconds: 1000 });
        
        door.alarm_expired(AlarmTimerExpired::Door);
        
        assert_eq!(door.status, DoorState::Closed);
        assert!(door.new_alarm);
        assert!(door.is_alarm()); // Alarm happened and resolved this sample.
    }

    #[test]
    fn test_alarm_expired_when_already_in_alarm() {
        let open_time = Timestamp { seconds: 1000 };
        let mut door = Door::new(true, open_time);
        door.alarm_expired(AlarmTimerExpired::Door);
        
        // Trigger alarm again
        door.alarm_expired(AlarmTimerExpired::Door);
        
        assert_eq!(door.status, DoorState::OpenAlarm(open_time));
        assert!(door.new_alarm);
        assert!(door.is_alarm());
    }

    #[test]
    fn test_non_door_alarm_ignored() {
        let mut door = Door::new(true, Timestamp { seconds: 1000 });
        
        door.alarm_expired(AlarmTimerExpired::HighTemperature);
        
        assert_eq!(door.status, DoorState::OpenNoAlarm(Timestamp { seconds: 1000 }));
        assert!(!door.new_alarm);
        assert!(!door.is_alarm());
    }

    #[test]
    fn test_clear_new_alarms() {
        let mut door = Door::new(true, Timestamp { seconds: 1000 });
        door.alarm_expired(AlarmTimerExpired::Door);
        assert!(door.new_alarm);
        
        door.clear_new_alarms();
        
        assert!(!door.new_alarm);
        assert!(door.is_alarm()); // Still in alarm state
    }

    #[test]
    fn test_get_values_door_closed() {
        let now = Timestamp { seconds: 1500 };
        let door = Door::new(false, Timestamp { seconds: 1000 });
        
        let (count, accum, idrv) = door.get_values(now);
        
        assert_eq!(count, 0);
        assert_eq!(accum, 0);
        assert_eq!(idrv, 0);
    }

    #[test]
    fn test_get_values_door_open() {
        let open_time = Timestamp { seconds: 1000 };
        let now = Timestamp { seconds: 1300 };
        let mut door = Door::new(true, open_time);
        
        // Add some accumulated time from previous cycles
        door.open_accum = 120;
        door.open_count = 2;
        
        let (count, accum, idrv) = door.get_values(now);
        
        assert_eq!(count, 2);
        assert_eq!(accum, 120);
        assert_eq!(idrv, 300); // Current open duration
    }

    #[test]
    fn test_get_idrv_door_closed() {
        let door = Door::new(false, Timestamp { seconds: 1000 });
        let idrv = door.get_idrv(Timestamp { seconds: 1500 });
        
        assert_eq!(idrv, 0);
    }

    #[test]
    fn test_get_idrv_door_open_no_alarm() {
        let open_time = Timestamp { seconds: 1000 };
        let door = Door::new(true, open_time);
        let idrv = door.get_idrv(Timestamp { seconds: 1400 });
        
        assert_eq!(idrv, 400);
    }

    #[test]
    fn test_get_idrv_door_open_with_alarm() {
        let open_time = Timestamp { seconds: 1000 };
        let mut door = Door::new(true, open_time);
        door.alarm_expired(AlarmTimerExpired::Door);
        
        let idrv = door.get_idrv(Timestamp { seconds: 1600 });
        
        assert_eq!(idrv, 600);
        assert_eq!(door.status, DoorState::OpenAlarm(open_time));
    }

    #[test]
    fn test_get_idrv_time_goes_backwards() {
        let open_time = Timestamp { seconds: 1000 };
        let door = Door::new(true, open_time);
        
        // Time is before door was opened
        let idrv = door.get_idrv(Timestamp { seconds: 900 });
        
        assert_eq!(idrv, 0);
    }

    #[test]
    fn test_reset_accumulators() {
        let mut door = Door::new(true, Timestamp { seconds: 1000 });
        door.open_count = 5;
        door.open_accum = 300;
        door.new_alarm = true;
        
        door.reset_accumulators();
        
        assert_eq!(door.open_count, 0);
        assert_eq!(door.open_accum, 0);
        assert!(!door.new_alarm);
        // State should remain unchanged
        assert_eq!(door.status, DoorState::OpenNoAlarm(Timestamp { seconds: 1000 }));
    }

    #[test]
    fn test_door_open_close_with_alarm_transition() {
        let open_time = Timestamp { seconds: 1000 };
        let mut door = Door::new(false, Timestamp { seconds: 1000 });
        
        // Open door
        door.door_event(DoorEvent::Opened, open_time);
        assert_eq!(door.status, DoorState::OpenNoAlarm(open_time));
        
        // Alarm expires while door is open
        door.alarm_expired(AlarmTimerExpired::Door);
        assert_eq!(door.status, DoorState::OpenAlarm(open_time));
        assert!(door.is_alarm());
        
        // Close door
        let close_time = Timestamp { seconds: 1400 };
        door.door_event(DoorEvent::Closed, close_time);
        assert_eq!(door.status, DoorState::Closed);
        assert_eq!(door.open_accum, 400); // Still accumulated the open time
        assert!(door.is_alarm()); // There was still an alarm this sample.
    }

    #[test]
    fn test_door_state_consistency() {
        let mut door = Door::new(false, Timestamp { seconds: 1000 });
        
        // Verify initial state consistency
        assert_eq!(door.get_idrv(Timestamp { seconds: 1050 }), 0);
        assert!(!door.is_alarm());
        
        // Open and verify
        door.door_event(DoorEvent::Opened, Timestamp { seconds: 1100 });
        assert!(door.get_idrv(Timestamp { seconds: 1200 }) > 0);
        assert!(!door.is_alarm());
        
        // Trigger alarm and verify
        door.alarm_expired(AlarmTimerExpired::Door);
        assert!(door.is_alarm());
        
        // Close and verify
        door.door_event(DoorEvent::Closed, Timestamp { seconds: 1500 });
        assert_eq!(door.get_idrv(Timestamp { seconds: 1600 }), 0);
        assert!(door.is_alarm()); // There was still an alarm this sample.
    }

    #[test]
    fn test_edge_case_same_timestamp() {
        let timestamp = Timestamp { seconds: 1000 };
        let mut door = Door::new(true, timestamp);
        
        // Close at the same timestamp
        door.door_event(DoorEvent::Closed, timestamp);
        
        assert_eq!(door.open_accum, 0); // No time elapsed
        assert_eq!(door.status, DoorState::Closed);
    }
}

