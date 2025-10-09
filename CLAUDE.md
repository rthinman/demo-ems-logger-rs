# Claude Development Context

## Project Overview
This is a Rust embedded systems project for an EMS (Equipment Monitoring System) temperature monitoring device running on STM32L476JE microcontroller.

## Architecture
- **Workspace Structure**: 2 crates
  - `business_logic/` - Core logic library (no_std)
  - `hardware_main/` - STM32 firmware with Embassy async framework

## Key Components
- Dual I2C temperature sensors (ambient + vaccine storage)
- RTC-based timestamping
- Event-driven architecture using Embassy channels
- Power management and door state monitoring

## Development Commands
- Build: `cargo build`
- Test business logic: `cargo test -p business_logic`
- Build hardware firmware: `cargo build -p hardware_main --target thumbv7em-none-eabi`
- Check hardware firmware: `cargo check -p hardware_main --target thumbv7em-none-eabi`
- Flash firmware: `cargo run -p hardware_main` (requires probe-rs/OpenOCD setup)

## Hardware Pins
- LED: PB0
- Button: PB5 (with EXTI interrupt)
- I2C: PB6 (SCL), PB7 (SDA)
- Power control: PA15

## Business Logic Modules
- `door.rs` - Door state management and event handling
- `power_availability.rs` - Power monitoring logic
- `temperature_aggregator.rs` - Temperature data processing
- `timestamp.rs` - RTC timestamp utilities

## Current Work
Working on temperature aggregation logic and power availability features on `rod/add_testing` branch.