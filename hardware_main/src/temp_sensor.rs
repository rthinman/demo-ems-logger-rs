//! This module contains the implementation of a dual temperature sensor
//! that reads ambient and vaccine temperatures from I2C sensors.

use embassy_stm32::gpio::Output;
use embassy_time::{Duration, Timer};

use business_logic::logger::TemperatureSample;
use crate::fmt::warn;


pub const AMBIENT_ADDRESS: u8 = 0x45; // I2C address for ambient temperature sensor.
pub const VACCINE_ADDRESS: u8 = 0x44; // I2C address for vaccine temperature sensor.
const SENSOR_REGISTER: u8 = 0x00; // Register to read temperature data.
const SENSOR_CONVERSION_TIME: Duration = Duration::from_millis(51); // Time to wait for sensor conversion.

// pub struct DualTempSensor<I2C> {
//     i2c: I2C,
//     amb_address: u8,
//     vax_address: u8,
//     enable_bar: Output<'static>,
// }

// impl<I2C> DualTempSensor<I2C> {
//     pub fn new(i2c: I2C, amb_address: u8, vax_address: u8, enable_bar: Output<'static>) -> Self {
//         Self { i2c, amb_address, vax_address, enable_bar }
//     }
// }

// impl<I2C> DualTempSensor<I2C>
// where
//     I2C: embedded_hal_async::i2c::I2c,
// {
//     pub async fn read_temperature_celsius(&mut self) -> TemperatureSample {
//         self.enable_bar.set_low(); // Enable the temperature sensor.
//         Timer::after(SENSOR_CONVERSION_TIME).await; // Wait for sensor to stabilize.
//         let mut buf = [0u8; 2];
//         let amb_temp  = match self.i2c.write_read(self.amb_address, &[SENSOR_REGISTER], &mut buf).await {
//             Ok(_) => Some(f32::from(i16::from_be_bytes(buf)) * 0.0078125), // Convert to Celsius
//             Err(_) => {
//                 warn!("Failed to read from temperature sensor");
//                 None
//             }
//         };

//         let vax_temp = match self.i2c.write_read(self.vax_address, &[SENSOR_REGISTER], &mut buf).await {
//             Ok(_) => Some(f32::from(i16::from_be_bytes(buf)) * 0.0078125), // Convert to Celsius
//             Err(_) => {
//                 warn!("Failed to read from temperature sensor");
//                 None
//             }
//         };

//         self.enable_bar.set_high(); // Disable the temperature sensor.

//         TemperatureSample {
//             ambient: amb_temp,
//             vaccine: vax_temp,
//         }
//     }
// }
