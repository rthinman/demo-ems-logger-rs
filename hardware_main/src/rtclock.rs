use embassy_stm32::rtc::{Rtc, DateTime, DayOfWeek};
use business_logic::timestamp::Timestamp;

const RTC_BACKUP_KEY_INDEX: usize = 0; // Index to RTC backup register where key is stored
const RTC_BACKUP_RTCW_INDEX: usize = 1; // Index to RTC backup register where RTCW is stored
const RTC_BACKUP_KEY_VALUE: u32 = 0xA53C4B69; // Value stored at RTC_BACKUP_KEY_INDEX if RTCW value is good
const EMBASSY_DATETIME_OFFSET: u16 = 2000; // Offset for the year in DateTime, since embassy-stm32 uses 2000-2099, but the RTC uses 0-99.

pub struct Rtclock {
    rtc: Rtc, // <'static, embassy_stm32::rtc::RtcConfig>
    rtcw: u32,
}

impl Rtclock {
    /// Create a new Rtclock instance if the RTC is already running.
    pub fn from_running(mut rtc: Rtc) -> Self {
        let rtcw = rtc.read_backup_register(RTC_BACKUP_RTCW_INDEX)
            .unwrap_or(0); // Read the RTCW value from the backup register, or 0 if not set.
        Self { rtc, rtcw }
    }
    /// Create a new Rtclock instance with a specific RTCW value.
    /// This is typically used when the RTC is starting from a power outage.
    pub fn from_rtcw(mut rtc: Rtc, rtcw: u32) -> Self {
        // Store the RTCW value in the backup register.
        rtc.write_backup_register(RTC_BACKUP_RTCW_INDEX, rtcw);
        // Write the key to the backup register to indicate that the RTCW value is valid.
        rtc.write_backup_register(RTC_BACKUP_KEY_INDEX, RTC_BACKUP_KEY_VALUE);
        // Convert to datetime and set RTC.
        let dt = Rtclock::seconds_to_datetime(rtcw);
        rtc.set_datetime(dt).expect("Failed to set datetime");
        // Return the Rtclock instance.
        Self { rtc, rtcw }
    }

    /// Get RELT, the uptime in seconds since first boot.
    pub fn get_uptime_seconds(&self) -> u32 {
        // Get the current time in seconds since the epoch.
        let now = self.rtc.now().unwrap();
        // Convert to seconds since the epoch (0, 3, 1).
        Rtclock::datetime_to_seconds(now).expect("Failed to convert datetime to seconds")
    }

    pub fn get_timestamp(&self) -> Timestamp {
        // Get the current time in seconds since the epoch.
        let now = self.rtc.now().unwrap();
        // Convert to seconds since the epoch (0, 3, 1).
        let seconds = Rtclock::datetime_to_seconds(now).expect("Failed to convert datetime to seconds");
        Timestamp { seconds }
    }

    /// Get RTCWake, the value of RELT at the last "brownout" event.
    pub fn get_rtcw(&self) -> Timestamp {
        Timestamp { seconds: self.rtcw }
    }

    // Static methods for Rtclock

    /// Check if the RTC is running.
    pub fn is_running(rtc: &Rtc) -> bool {
        // Check if the RTC is running by reading the backup register.
        rtc.read_backup_register(RTC_BACKUP_KEY_INDEX).unwrap_or(0) == RTC_BACKUP_KEY_VALUE
    }

    /// Convert seconds since the epoch (0, 3, 1) to a DateTime.
    pub fn seconds_to_datetime(seconds: u32) -> DateTime {
        // Get the total number of days represented, plus the time of day.
        // A u32 can hold up to 49,710 days, which is about 136 years.
        let ts = Timestamp { seconds };
        let (days, hour, minute, second) = ts.to_dhms();

        // Calculate the Julian date from the number of days, since computational epoch
        // March 1, 0000.
        // Based on the algorithm from below, but not yet using the Euclidean 
        // affine function optimizations:
        // Neri C, Schneider L. "Euclidean affine functions and their application 
        // to calendar algorithms". Softw Pract Exper. 2022;1-34. doi: 10.1002/spe.3172.
        // https://onlinelibrary.wiley.com/doi/full/10.1002/spe.3172
        // References to variable names are to the paper above, Section 5, which uses uppercase.
        let n_1 = 4 * days + 3;  // N1
        let year_computational = n_1 / 1461;  // Y
        let n_y = n_1 % 1461 / 4; // N_Y
        let n_2 = 5 * n_y + 461;  // N_2
        let m = n_2 / 153; // M
        let day_julian: u8 = (n_2 % 153 / 5 + 1) as u8; // D_J (skipped computing D)
        // J = 1{M>=13}
        let j = if m >= 13 {
            1
        } else {
            0
        };

        let month_julian: u8 = (m - 12 * j) as u8; // M_J
        // Embassy's DateTime uses a year offset of 2000, so we need to add that.
        let year_julian: u16 = (year_computational + j) as u16 + EMBASSY_DATETIME_OFFSET; // Y_J + 2000

        // We do not use day of week, so the choice is arbitrary.
        DateTime::from(year_julian, month_julian, day_julian, DayOfWeek::Monday, hour as u8, minute as u8, second as u8).expect("Invalid date")
    }

    /// Convert a DateTime to seconds since the epoch (0, 3, 1) Julian date.
    pub fn datetime_to_seconds(datetime: DateTime) -> Option<u32> {
        // The STM32 RTC only holds year values 0-99.
        // Embassy's DateTime assumes the year is 2000-2099, so get last two digits.

        let year: u32 = (datetime.year() % 100).into(); 
        let month: u32 = datetime.month().into();
        if year == 0 && month < 3 {
            // The computational calendar does not support this combination with
            // unsigned values, and we should always initialize the RTC to later than this.
            return None;
        }
        let day: u32 = datetime.day().into();
        let hour: u32 = datetime.hour().into();
        let minute: u32 = datetime.minute().into();
        let second: u32 = datetime.second().into();

        let j: u32 = if month <= 2 {
            // This is because the computational calendar starts on March 1st.
            1
        } else {
            0
        };
        let y = year - j;
        let m = month + 12 * j; // Month is 1-12, but we need to adjust for the computational calendar.
        let d = day - 1; // Day is 1-31.

        let y0 = 1461 * y / 4;
        let m0 = (153 * m - 457) / 5;

        let days_since_epoch =  y0 + m0 + d;

        // Calculate the total seconds since the epoch.
        Some(days_since_epoch * 86400 + hour * 3600 + minute * 60 + second)
    }

}
