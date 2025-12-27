//! Real-Time Clock (RTC) support
//!
//! Reads date and time from the CMOS RTC chip.

use crate::port::{inb, outb};

/// CMOS address port
const CMOS_ADDR: u16 = 0x70;
/// CMOS data port
const CMOS_DATA: u16 = 0x71;

/// RTC register addresses
mod reg {
    pub const SECONDS: u8 = 0x00;
    pub const MINUTES: u8 = 0x02;
    pub const HOURS: u8 = 0x04;
    pub const DAY: u8 = 0x07;
    pub const MONTH: u8 = 0x08;
    pub const YEAR: u8 = 0x09;
    pub const STATUS_A: u8 = 0x0A;
    pub const STATUS_B: u8 = 0x0B;
}

/// Read a CMOS register
fn read_cmos(register: u8) -> u8 {
    unsafe {
        // Select register (NMI disabled by setting bit 7)
        outb(CMOS_ADDR, register);
        inb(CMOS_DATA)
    }
}

/// Check if RTC update is in progress
fn update_in_progress() -> bool {
    read_cmos(reg::STATUS_A) & 0x80 != 0
}

/// Convert BCD to binary
fn bcd_to_bin(bcd: u8) -> u8 {
    (bcd & 0x0F) + ((bcd >> 4) * 10)
}

/// Time structure
#[derive(Clone, Copy, Debug)]
pub struct Time {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
}

/// Date structure
#[derive(Clone, Copy, Debug)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

/// Read current time from RTC
pub fn read_time() -> Time {
    // Wait for update to complete
    while update_in_progress() {}

    let status_b = read_cmos(reg::STATUS_B);
    let is_bcd = (status_b & 0x04) == 0;
    let is_24h = (status_b & 0x02) != 0;

    let mut seconds = read_cmos(reg::SECONDS);
    let mut minutes = read_cmos(reg::MINUTES);
    let mut hours = read_cmos(reg::HOURS);

    // Convert from BCD if needed
    if is_bcd {
        seconds = bcd_to_bin(seconds);
        minutes = bcd_to_bin(minutes);
        hours = bcd_to_bin(hours & 0x7F) | (hours & 0x80); // Preserve PM bit
    }

    // Convert 12-hour to 24-hour if needed
    if !is_24h && (hours & 0x80) != 0 {
        hours = ((hours & 0x7F) + 12) % 24;
    }

    Time {
        hours,
        minutes,
        seconds,
    }
}

/// Read current date from RTC
pub fn read_date() -> Date {
    // Wait for update to complete
    while update_in_progress() {}

    let status_b = read_cmos(reg::STATUS_B);
    let is_bcd = (status_b & 0x04) == 0;

    let mut day = read_cmos(reg::DAY);
    let mut month = read_cmos(reg::MONTH);
    let mut year = read_cmos(reg::YEAR);

    // Convert from BCD if needed
    if is_bcd {
        day = bcd_to_bin(day);
        month = bcd_to_bin(month);
        year = bcd_to_bin(year);
    }

    // Assume 21st century
    let full_year = 2000 + year as u16;

    Date {
        year: full_year,
        month,
        day,
    }
}

/// Get packed time value: hours << 16 | minutes << 8 | seconds
pub fn get_packed_time() -> u32 {
    let t = read_time();
    ((t.hours as u32) << 16) | ((t.minutes as u32) << 8) | (t.seconds as u32)
}

/// Get packed date value: year << 16 | month << 8 | day
pub fn get_packed_date() -> u32 {
    let d = read_date();
    ((d.year as u32) << 16) | ((d.month as u32) << 8) | (d.day as u32)
}
