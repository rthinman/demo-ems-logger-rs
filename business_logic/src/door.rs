// //! This module contains the business logic for handling door events, including
// //! tracking door open/close events, accumulating open durations, and managing alarms.

use crate::logger::{AlarmTimerExpired, AlarmTimerTrigger};
use crate::timestamp::Timestamp;

// const DOOR_ALARM_THRESHOLD: u32 = 300;    // 5 minutes in seconds.

// // TODO: rework to have functions that get and reset accumulators all at once.  


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
    state: DoorState, 
    open_count: u16, // Count of opening in this sample period.
    open_accum: u32, // Accumulated open time in this sample period, in seconds.
    new_alarm: bool, // Flag indicating if the alarm was triggered in this sample period.
}

impl Door {
    /// Create a new Door instance with the current door state.
    /// Note that if the door is open, the caller should also trigger the alarm timer.
    pub fn new(now: Timestamp, open: bool) -> Self {
        // Assume the door was opened now.
        let opened = if open {
            DoorState::OpenNoAlarm(now)
        } else {
            DoorState::Closed
        };
        let open_count = if open { 1 } else { 0 };

        Self {
            state: opened,
            open_count: open_count,
            open_accum: 0,
            new_alarm: false,
        }
    }

    pub fn process_door_event(&mut self, event: DoorEvent, now: Timestamp) -> AlarmTimerTrigger {
        
        // TODO: Handle the case where the door is opened while it is already open, and vice versa.
        match event {
            DoorEvent::Opened => {
                self.state = DoorState::OpenNoAlarm(now);
                // Counts incremented when the door is opened.
                self.open_count += 1;

                // Trigger the alarm timer.
                AlarmTimerTrigger::DoorOpenStart
            }
            DoorEvent::Closed => {
                // If the door was open, accumulate the open duration.
                match self.state {
                    DoorState::Closed => {
                        // Door was already closed, do nothing.
                    }
                    DoorState::OpenAlarm(opened) | DoorState::OpenNoAlarm(opened) => {
                        let duration = now.seconds - opened.seconds;
                        self.open_accum += duration;
                    }
                }
                // Cancel the alarm timer.
                AlarmTimerTrigger::DoorOpenCancel
            }
        }
    }

    pub fn alarm_expired(&mut self, event: AlarmTimerExpired) {
        if event == AlarmTimerExpired::Door {
            self.new_alarm = true;
            match self.state {
                DoorState::Closed => {
                    // This should not happen, but just in case, do nothing.
                }
                DoorState::OpenNoAlarm(opened) => {
                    self.state = DoorState::OpenAlarm(opened);
                }
                DoorState::OpenAlarm(_) => {
                    // Already in alarm state, do nothing.
                }
            }
        }
    }

    pub fn is_alarm(&self) -> bool {
        match self.state {
            DoorState::OpenAlarm(_) => true,
            _ => false,
        }
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
        match self.state {
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


}

