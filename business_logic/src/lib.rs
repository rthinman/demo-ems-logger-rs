#![cfg_attr(not(test), no_std)]

#[derive(Debug, PartialEq)]
pub enum Status {
    Success,
    Error,
}

pub fn process_data(input: &str) -> Status {
    if input.is_empty() {
        Status::Error
    } else {
        // Simulate some processing
        Status::Success
    }
}

pub mod aggregator;
pub mod constants;
pub mod door;
pub mod logger;
pub mod power_availability;
pub mod temperatures;
pub mod timestamp;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_data() {
        assert_eq!(process_data("Hello, World!"), Status::Success);
        assert_eq!(process_data(""), Status::Error);
    }
}