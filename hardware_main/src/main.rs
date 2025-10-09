#![no_std]
#![no_main]

mod alarm_timer_state;
mod dhara_nand_async;
mod my_flash;
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
use embedded_hal_bus;
use embassy_time::{Duration, Instant, Ticker, Timer};
#[cfg(not(feature = "defmt"))]
use panic_halt as _;

// Crates in different repositories
use embedded_nand_async::NandFlash;
use embedded_nand::{BlockIndex, BlockStatus, ColumnAddress, PageIndex};
use spi_nand::cmd_async::SpiNandAsync;
use spi_nand::{ECCStatus, SpiNand, SpiNandDevice};
use spi_nand_devices::winbond::w25n::{asyn::{BBMAsync, ECCBasicAsync, ODSAsync}, W25N01GW};

// Internal modules, both this crate and the business logic crate.
use business_logic::{door::DoorEvent, logger::{self, AlarmTimerTrigger, Logger, LoggerEvent, TemperatureSample}};
use business_logic::timestamp::Timestamp;
use alarm_timer_state::{AlarmTimerState, TempTimerActive};
use dhara_nand_async::{DharaNandAsync, DharaError, DharaPage, DharaBlock};
use fmt::{info, warn, unwrap};
use my_flash::MyFlash;
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
    let num_blocks = <W25N01GW as SpiNand<2048>>::BLOCK_COUNT;
    let page_size = <W25N01GW as SpiNand<2048>>::PAGE_SIZE;
    let pages_per_block = <W25N01GW as SpiNand<2048>>::PAGES_PER_BLOCK;
    info!("Flash device W25N01GW: {} blocks, {} pages/block, {} bytes/page", num_blocks, pages_per_block, page_size);
    

    // Create async SPI device using ExclusiveDevice (only one device on bus)
    let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new(spi, cs, embedded_hal_bus::spi::NoDelay).unwrap();
    let mut flash = SpiNandDevice::new(spi_device, device);
    // flash.reset_async().await.unwrap();
    // embassy_time::Timer::after_secs(1).await;
    let jed = flash.verify_jedec_async().await.unwrap();
    // And from SpiNandAsync itself.
    info!("Flash verification completed, JEDEC ID correct: {:?}", jed);

    let mut my_flash = MyFlash::new(
        flash,
        my_flash::log2(page_size),  // log2_page_size = 11 for 2048 byte pages
        my_flash::log2(pages_per_block),  // log2_ppb = 6 for 64 pages/block
        num_blocks,
    );

    my_flash.initialize().await.unwrap();

    info!("checking MyFlash parameters: page size {}, pages/block {}, num blocks {}",
        1 << my_flash.get_log2_page_size(),
        1 << my_flash.get_log2_ppb(),
        my_flash.get_num_blocks()
    );

    // Check MyFlash implementation.
    // One time mark block 0 as bad for testing.
    // my_flash.mark_bad(0).await;

    // One time erase for testing.
    // my_flash.erase(0).await.unwrap();

    let bad = my_flash.is_bad(0).await;
    info!("Block 0 bad? {}", bad);
    let free = my_flash.is_free(0).await; // Should be programmed, as is page 1 at this point.
    info!("Page 0 free? {}", free);

    let mut small_buf: [u8; 10] = [0; 10];
    // // New data for page 0
    small_buf[0] = 13;
    small_buf[1] = 27;
    small_buf[2] = 100;

    // let free = my_flash.is_free(4).await; // Should be free.
    // info!("Page 4 free? {}", free);
    // info!("write page 0");
    // my_flash.prog(0, &small_buf).await.unwrap();

    // info!("copying page 0 to 1");
    // my_flash.copy(0, 1).await.unwrap();
    // Check again.
    let free = my_flash.is_free(1).await; // Should not be free any longer.
    info!("Page 1 free? {}", free);

    let mut small_buf: [u8; 10] = [0; 10];
    let res = my_flash.read(0, 0, 5, &mut small_buf).await;
    // Should be 13, 27, 100, 0, 0.
    info!("Read page 0, first 5 bytes: {:?}", &small_buf[..5]);

    let mut small_buf: [u8; 10] = [0; 10];
    let res = my_flash.read(1, 0, 5, &mut small_buf).await;
    info!("Read page 1, first 5 bytes: {:?}", &small_buf[..5]);

 
 
    // // Test methods from trait SpiNandDevice implemented for SpiNandAsync:
    // let blk = flash.reset_async().await.unwrap();
    // embassy_time::Timer::after_secs(1).await;
    // let jed = flash.verify_jedec_async().await.unwrap();
    // // And from SpiNandAsync itself.
    // info!("Flash reset result: {:?}", blk);
    // info!("Flash JEDEC ID: {:?}", jed);
    // let reg1 = flash.device.read_register_cmd(&mut flash.spi, 0xA0).await.unwrap();
    // info!("Flash Register 1 (0xA0): {:?}", reg1);
    // let reg2 = flash.device.read_register_cmd(&mut flash.spi, 0xB0).await.unwrap();
    // info!("Flash Register 2 (0xB0): {:?}", reg2);
    // let reg3 = flash.device.read_register_cmd(&mut flash.spi, 0xC0).await.unwrap();
    // info!("Flash Register 3 (0xC0): {:?}", reg3);

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

    // Test Winbond-specific traits and their methods
    // Bad block management
    // let lut = flash.device.is_lut_full(&mut flash.spi).await.unwrap();
    // info!("look-up table full {:?}", lut);
    // let lut = flash.device.read_lut_cmd(&mut flash.spi).await.unwrap();
    // // Basic ECC
    // let ecc_status = flash.device.ecc_status(&mut flash.spi).await.unwrap();
    // // Output driver strength
    // let driver_strength = flash.device.get_output_driver_strength(&mut flash.spi).await.unwrap();

    // Test highest level flash interface, the NandFlash trait.

    // Print the flash's capacity
    // let cap = flash.capacity();
    // info!("capacity: {}", cap);
    // // Check block 0 status
    // match flash.block_status(BlockIndex::new(0)).await.unwrap() {
    //     BlockStatus::Ok => {info!("Block 0 OK")},
    //     BlockStatus::Failed => {info!("Block 0 is bad")},
    //     _ => {info!("Block 0 unknown status")},
    // }

    // Read page 0, first few bytes, last few bytes with NandFlash::read(), though it is less flexible.
    // let mut buf: [u8; 2048] = [0; 2048];
    // let foo = flash.read(0, &mut buf).await.unwrap();
    // info!("Page 0 data, {}, {}, {}...{}, {}", buf[0], buf[1], buf[2], buf[2046], buf[2047]);

    // let mut buf: [u8; 2050] = [0; 2050];
    // let foo = flash.read_page_slice_async(PageIndex::new(1), ColumnAddress::new(0), &mut buf).await.unwrap();
    // info!("Page 1 data, {}, {}, {}...{}, {}", buf[0], buf[1], buf[2], buf[2046], buf[2047]);
    // info!("bad and seal bytes {}, {}", buf[2048], buf[2049]);
    // // Check ECC
    // match flash.device.ecc_status(&mut flash.spi).await.unwrap() {
    //     ECCStatus::Ok => {info!("read OK");},
    //     ECCStatus::Corrected => {info!("read corrected");},
    //     ECCStatus::Failed => {info!("read failed");},
    //     _ => {info!("read ECC indeterminate");},
    // }
    // Only do this once, to minimize erase/write cycles:
    //   Remove write protection on all blocks.
    //   Erase block 0
    //   Read page 0, verifying that bytes are 0xFF.
    //   Write first few bytes, last few bytes, and "seal" byte.
    // Done and confirmed.
    // let foo = flash.device.write_register_cmd(&mut flash.spi, <W25N01GW as SpiNand::<2048>>::CONFIGURATION_REGISTER, 0x00).await.unwrap();
    // info!("Erasing block 0 ...");
    // let foo = flash.erase_block(BlockIndex::new(0)).await.unwrap();
    // let foo = flash.read(0, &mut buf).await.unwrap();
    // info!("Page 0 data, {}, {}, {}...{}, {}", buf[0], buf[1], buf[2], buf[2046], buf[2047]);
    // let foo = flash.read_page_slice_async(PageIndex::new(0), ColumnAddress::new(2048), &mut small_buf).await.unwrap();
    // info!("bad and seal bytes {}, {}", small_buf[0], small_buf[1]);

    // // New data for page 0
    // buf[0] = 0;
    // buf[1] = 1;
    // buf[2] = 53;
    // buf[2046] = 10;
    // buf[2047] = 13;
    // buf[2048] = 0xFF; // good block
    // buf[2049] = 0;    // block now sealed
    // info!("Writing new data");
    // // Unprotect array.
    // let foo = flash.device.write_register_cmd(&mut flash.spi, <W25N01GW as SpiNand::<2048>>::CONFIGURATION_REGISTER, 0x00).await.unwrap();
    // // Write the page + two spare.
    // let foo = flash.write_page_slice_async(PageIndex::new(0), ColumnAddress::new(0), &buf).await.unwrap();

    // // Change the buffer to ensure we read back real values.
    // buf[0] = 30;
    // buf[1] = 30;
    // buf[2] = 30;
    // buf[2046] = 30;
    // buf[2047] = 30;
    // buf[2048] = 30;
    // buf[2049] = 30;

    // // Read page 0 again to verify first few bytes, last few bytes, and "seal" byte.
    // let foo = flash.read_page_slice_async(PageIndex::new(0), ColumnAddress::new(0), &mut buf).await.unwrap();
    // info!("Page 0 data, {}, {}, {}...{}, {}", buf[0], buf[1], buf[2], buf[2046], buf[2047]);
    // info!("bad and seal bytes {}, {}", buf[2048], buf[2049]);

    // Copy page 0 to page 1.
    // // Unprotect array.
    // let foo = flash.device.write_register_cmd(&mut flash.spi, <W25N01GW as SpiNand::<2048>>::CONFIGURATION_REGISTER, 0x00).await.unwrap();
    // let foo = flash.copy_page_async(PageIndex::new(0), PageIndex::new(1)).await.unwrap();
    // info!("Copy page 0 to 1");
    // let foo = flash.read_page_slice_async(PageIndex::new(1), ColumnAddress::new(0), &mut buf).await.unwrap();
    // info!("Page 1 data, {}, {}, {}...{}, {}", buf[0], buf[1], buf[2], buf[2046], buf[2047]);
    // info!("bad and seal bytes {}, {}", buf[2048], buf[2049]);


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
