#![no_std]
#![no_main]

mod alarm_timer_state;
mod fmt;
mod rtclock;
mod temp_sensor;

// Use declarations
// Core libraries
use core::f32::consts;
use core::fmt::Write;

// External libraries
use arrayvec::ArrayString;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};
use embassy_executor::Spawner;
use embassy_stm32::{bind_interrupts, exti::ExtiInput, peripherals};
use embassy_stm32::{gpio::{Level, Output, Pull, Speed}, i2c::{ErrorInterruptHandler, EventInterruptHandler, I2c}, rtc::{Rtc, RtcConfig}, time::Hertz, Config};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_sync::mutex::Mutex;
use embassy_embedded_hal;
use embedded_hal_bus;
use embassy_time::{Duration, Instant, Ticker, Timer};
#[cfg(not(feature = "defmt"))]
use panic_halt as _;

// use spi_nand_devices::winbond::w25n::W25N01GW;
// use spi_nand::SpiNandDevice;
// use spi_nand::SpiNand;
use embedded_nand_async::NandFlash;
use spi_nand::cmd_async::SpiNandAsync;
use spi_nand::{SpiNand, SpiNandDevice};
use spi_nand_devices::winbond::w25n::{asyn::BBMAsync, W25N01GW};


// Internal modules, both this crate and the business logic crate.
use business_logic::{door::DoorEvent, logger::{self, AlarmTimerTrigger, Logger, LoggerEvent, TemperatureSample}};
use business_logic::timestamp::Timestamp;
use alarm_timer_state::{AlarmTimerState, TempTimerActive};
use fmt::{info, warn, unwrap};
use rtclock::{Rtclock};
// use temp_sensor::{AMBIENT_ADDRESS, DualTempSensor, VACCINE_ADDRESS};


// Communicate between tasks using channels.
static EVENT_CHANNEL: Channel<ThreadModeRawMutex, LoggerEvent, 8> = Channel::new();
static ALARM_CHANNEL: Channel<ThreadModeRawMutex, AlarmTimerTrigger, 8> = Channel::new();


#[embassy_executor::main]
async fn main(spawner: Spawner) {

    // Chip peripheral configuration
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::*;
        use embassy_stm32::rcc::mux::{Adcsel, Clk48sel, I2c1sel};

        // Adjust the configuration from the default.
        // Default for Config.rcc is hse=None, hsi=false, SAI1,2=None
        config.rcc.msi = Some(MSIRange::RANGE4M); // Multi-speed Osc. = 4 MHz

        // PLL creates 48 MHz at its output (PLLCLK).
        config.rcc.pll = Some(Pll {
            source: PllSource::MSI,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL24,
            divp: None, // This was DIV7 in the CubeMX config, but the output would only be for serial audio, which we are not using.
            divq: Some(PllQDiv::DIV2),
            divr: Some(PllRDiv::DIV2), // for sysclk of 48 MHz
        });

        // Clock busses
        config.rcc.sys = Sysclk::PLL1_R; // 48 MHz
        config.rcc.ahb_pre = AHBPrescaler::DIV1; // HCLK = 48 MHz
        config.rcc.apb1_pre = APBPrescaler::DIV1;
        config.rcc.apb2_pre = APBPrescaler::DIV1;

        // Low-speed oscillators
        config.rcc.ls = LsConfig {
            rtc: RtcClockSource::LSE,
            lsi: false, // Not using LSI for either watchdog or RTC.
            lse: Some(LseConfig { frequency: Hertz(32768), mode: LseMode::Oscillator(LseDrive::Low) }),
        };

        // Reconfigure some of the clock mux struct fields.
        config.rcc.mux.adcsel = Adcsel::SYS;  // C firmware used SAI1R clock, also 48 MHz.  Not sure why.
        config.rcc.mux.clk48sel = Clk48sel::PLLSAI1_Q; // TODO: code doc says this is the PLL48M1CLK, but datasheet says PLL48M1CLK comes from PLL1.  C code uses the SAI1clk.  
        config.rcc.mux.i2c1sel = I2c1sel::PCLK1;
    }
    let p = embassy_stm32::init(config);

    // GPIOs
    let mut pwrv_nen = Output::new(p.PA15, Level::High, Speed::Low); // Power enable for the temperature sensor.
    pwrv_nen.set_low(); // Enable the temperature sensor.
    let mut led = Output::new(p.PB0, Level::High, Speed::Low);
    let mut btn = ExtiInput::new(p.PB5, p.EXTI5, Pull::Up);

    // RTC initialization
    let mut rtc = Rtc::new(p.RTC, RtcConfig::default());
    rtc.set_daylight_savings(false);
    let rt_clock = if Rtclock::is_running(&rtc) {
        info!("RTC is running, using existing RTCW value...");
        Rtclock::from_running(rtc)
    } else {
        // RTC was not running, so we need to initialize it.
        info!("RTC not running, initializing...");
        let rtcw = 0_u32; // TODO: Get the RTCW value from non-volatile storage or set to 0.
        Rtclock::from_rtcw(rtc, rtcw)
    };

    // SPI and flash

    let mut spi  = embassy_stm32::spi::Spi::new(
        p.SPI2,
        p.PB13, // SCK
        p.PB15, // COPI/MOSI
        p.PB14, // CIPO/MISO
        p.DMA1_CH5,
        p.DMA1_CH4,
        embassy_stm32::spi::Config::default(),
    );

    let mut flash_pwr_nen = Output::new(p.PA8, Level::Low, Speed::Low); // Power enable for the flash, start enabled.
    let mut cs = Output::new(p.PB12, Level::High, Speed::High); // Chip select for the flash.

    // Create [spi_flash::device::SpiFlash] instance  
    let device = W25N01GW::new();
    //let b = <W25N01GW as SpiNand<2048>>::BLOCK_COUNT;

    // Create async SPI device using embassy shared bus
    let spi_bus = embassy_sync::mutex::Mutex::<embassy_sync::blocking_mutex::raw::NoopRawMutex, _>::new(spi);
    let spi_device = embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice::new(&spi_bus, cs);
    let mut flash = SpiNandDevice::new(spi_device, device);

    let blk = flash.reset_async().await.unwrap();
    let jed = flash.verify_jedec_async().await.unwrap();
    info!("Flash reset result: {:?}", blk);
    info!("Flash JEDEC ID: {:?}", jed);

    let reg1 = flash.device.read_register_cmd(&mut flash.spi, 0xA0).await.unwrap();
    info!("Flash Register 1 (0xA0): {:?}", reg1);

    let reg2 = flash.device.read_register_cmd(&mut flash.spi, 0xB0).await.unwrap();
    info!("Flash Register 2 (0xB0): {:?}", reg2);
    let reg3 = flash.device.read_register_cmd(&mut flash.spi, 0xC0).await.unwrap();
    info!("Flash Register 3 (0xC0): {:?}", reg3);

    let mut buf: [u8; 10] = [0; 10];
    let foo = flash.read(0, &mut buf).await.unwrap();
    // info!("block status {:?}", foo);
    embassy_time::Timer::after_secs(1).await;

    // for i in 0..1024 {
    //     // let (bb0, bb1) = flash
    //     //     .device
    //     //     .block_marked_bad(&mut flash.spi, BlockIndex::new(i))
    //     //     .unwrap();
    //     // info!("Block {} values {}, {}", i, bb0, bb1);
    //     if flash
    //         .device
    //         .block_marked_bad(&mut flash.spi, BlockIndex::new(i))
    //         .unwrap()
    //     {
    //         info!("Block {} is marked bad", i);
    //     }
    // }

    let lut = flash.device.is_lut_full(&mut flash.spi).await.unwrap();
    info!("look-up table full {:?}", lut);

    let lut = flash.device.read_lut_cmd(&mut flash.spi).await.unwrap();

    // // I2C and temp sensor initialization.
    // bind_interrupts!(struct Irqs {
    //     I2C1_EV => EventInterruptHandler<peripherals::I2C1>;
    //     I2C1_ER => ErrorInterruptHandler<peripherals::I2C1>;
    // });

    // let mut i2c = I2c::new(
    //     p.I2C1, 
    //     p.PB6, 
    //     p.PB7, 
    //     Irqs,
    //     p.DMA1_CH6,
    //     p.DMA1_CH7, 
    //     Hertz(400_000),
    //     Default::default(),
    // );
    // let mut temp_sensor = DualTempSensor::new(i2c, AMBIENT_ADDRESS, VACCINE_ADDRESS, pwrv_nen);

    let door_open = btn.is_low();
    let mut logger = Logger::new(rt_clock.get_rtcw(), door_open, rt_clock.get_timestamp());
    // Trigger the door alarm timer if the door is open at startup.
    if door_open {
        ALARM_CHANNEL.send(AlarmTimerTrigger::DoorOpenStart).await;
    }

    // Spawn the tasks
    // spawner.spawn(button(btn, EVENT_CHANNEL.sender())).unwrap();
    spawner.spawn(led_blink(led)).unwrap();
    // spawner.spawn(get_temperature(temp_sensor, EVENT_CHANNEL.sender())).unwrap();
    spawner.spawn(alarm_timeouts(ALARM_CHANNEL.receiver(), EVENT_CHANNEL.sender())).unwrap();

    warn!("Starting main loop");

    let mut x: u32 = 0;

    // The main loop performs the primary logging functions.
    loop {
        // Wait for an an event from various subsystems.
        let event = EVENT_CHANNEL.receive().await;
        let now = rt_clock.get_timestamp();

        // Match is to separate for logging purposes. If not needed, remove and uncomment the line below this statement.
        let alarm_trigger = match event {
            LoggerEvent::DoorEvent(DoorEvent::Opened) => {
                info!("Button pressed event received");
                logger.process_event(event, now).unwrap_or(AlarmTimerTrigger::NoTrigger)
            }
            LoggerEvent::DoorEvent(DoorEvent::Closed) => {
                info!("Button released event received");
                logger.process_event(event, now).unwrap_or(AlarmTimerTrigger::NoTrigger)
            }
            LoggerEvent::TemperatureSample(temperature) => {
                info!("Time: {}, TAMB: {} °C, TVC: {} °C", now.seconds, temperature.ambient, temperature.vaccine);
                info!("{=str}", now.create_iso8601_str());
                logger.process_event(event, now).unwrap_or(AlarmTimerTrigger::NoTrigger)
            }
            LoggerEvent::AlarmStateChange(alarm_trigger) => {
                info!("Alarm state change");
                // info!("Alarm state change: {:?}", alarm_trigger);
                logger.process_event(event, now).unwrap_or(AlarmTimerTrigger::NoTrigger)
            }
        };

        // Process the event in the logger.
        // let alarm_trigger = logger.process_event(event, now).unwrap_or(AlarmTrigger::NoTrigger);

        // If there is an alarm trigger, send it to the alarm channel.
        if alarm_trigger != AlarmTimerTrigger::NoTrigger {
            ALARM_CHANNEL.send(alarm_trigger).await;
        }
    }
}

/// Task to handle button presses, which simulate door open/close events.
// #[embassy_executor::task]
// async fn button(mut btn: ExtiInput<'static>, msg: Sender<'static, ThreadModeRawMutex, LoggerEvent, 8>) {
//     loop {
//         btn.wait_for_falling_edge().await;
//         info!("Button pressed/door open!");
//         msg.send(LoggerEvent::DoorEvent(DoorEvent::Opened)).await;
//         // Debounce delay
//         Timer::after(Duration::from_millis(50)).await;
//         // Wait for release (rising edge)
//         btn.wait_for_rising_edge().await;
//         info!("Button released/door closed!");
//         msg.send(LoggerEvent::DoorEvent(DoorEvent::Closed)).await;
//         // Debounce delay
//         Timer::after(Duration::from_millis(50)).await;
//     }
// }

/// Task to blink an LED to show the system is alive.
#[embassy_executor::task]
async fn led_blink(mut led: Output<'static>) {
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(500)).await;
        led.set_low();
        Timer::after(Duration::from_millis(500)).await;
    }
}

/// Task to read temperatures from the sensors and send them to the logger.
// #[embassy_executor::task]
// async fn get_temperature(
//     mut temp_sensor: DualTempSensor<I2c<'static, embassy_stm32::mode::Async>>,
//     msg: Sender<'static, ThreadModeRawMutex, LoggerEvent, 8>,
// ) {
//     let mut ticker = Ticker::every(Duration::from_secs(10)); // Read every 10 seconds
//     loop {
//         let temperatures = temp_sensor.read_temperature_celsius().await;
//         msg.send(LoggerEvent::TemperatureSample(temperatures)).await;
//         ticker.next().await;
//     }
// }

/// Task to manage alarm timers and send alarm state changes to the logger.
#[embassy_executor::task]
async fn alarm_timeouts(
    alarm_receiver: embassy_sync::channel::Receiver<'static, ThreadModeRawMutex, AlarmTimerTrigger, 8>,
    event_sender: Sender<'static, ThreadModeRawMutex, LoggerEvent, 8>,
) {
    let mut alarm_state = AlarmTimerState::new();
    
    loop {
        let next_alarm_time = {
            let mut earliest = None;
            
            if alarm_state.door_active {
                earliest = Some(alarm_state.door_expires);
            }
            
            match alarm_state.temperature_active {
                TempTimerActive::LowTemperature | TempTimerActive::HighTemperature => {
                    match earliest {
                        Some(time) if alarm_state.temperature_expires < time => {
                            earliest = Some(alarm_state.temperature_expires);
                        }
                        None => {
                            earliest = Some(alarm_state.temperature_expires);
                        }
                        _ => {}
                    }
                }
                TempTimerActive::NoneActive => {}
            }
            
            earliest
        };
        
        let received_trigger = match next_alarm_time {
            Some(alarm_time) => {
                // There is an active alarm, wait for either a new trigger from the channel or the timer to expire.
                match embassy_futures::select::select(
                    alarm_receiver.receive(),
                    Timer::at(alarm_time)
                ).await {
                    embassy_futures::select::Either::First(trigger) => Some(trigger),
                    embassy_futures::select::Either::Second(_) => None,
                }
            }
            None => {
                // No active alarms, just wait for triggers.
                Some(alarm_receiver.receive().await)
            }
        };
        
        let now = Instant::now();
        
        // Process any received trigger
        if let Some(trigger) = received_trigger {
            alarm_state.process_trigger(trigger, now);
        }
        
        // Check if door alarm timer has expired
        if alarm_state.door_active && now >= alarm_state.door_expires {
            info!("Door alarm timer expired");
            alarm_state.door_active = false;
            event_sender.send(LoggerEvent::AlarmStateChange(logger::AlarmTimerExpired::Door)).await;
        }
        
        // Check if temperature alarm timer has expired
        match alarm_state.temperature_active {
            TempTimerActive::LowTemperature => {
                if now >= alarm_state.temperature_expires {
                    info!("Low temperature alarm timer expired");
                    alarm_state.temperature_active = TempTimerActive::NoneActive;
                    event_sender.send(LoggerEvent::AlarmStateChange(logger::AlarmTimerExpired::LowTemperature)).await;
                }
            }
            TempTimerActive::HighTemperature => {
                if now >= alarm_state.temperature_expires {
                    info!("High temperature alarm timer expired");
                    alarm_state.temperature_active = TempTimerActive::NoneActive;
                    event_sender.send(LoggerEvent::AlarmStateChange(logger::AlarmTimerExpired::HighTemperature)).await;
                }
            }
            TempTimerActive::NoneActive => {}
        }
    }
}
