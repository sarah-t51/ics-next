use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DateTimeValue {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl DateTimeValue {
    // Accepts the iCalendar DATE (YYYYMMDD) and DATE-TIME (YYYYMMDD"T"HHMMSS["Z"]) forms.
    // TZID parameters are not resolved here: every value is treated as if it were UTC.
    // That's wrong for floating local times, but it's an honest, documented limitation
    // rather than a silent one (see README).
    pub fn parse(value: &str) -> Result<DateTimeValue, String> {
        let s = value.trim();
        let (date_part, time_part) = match s.find('T') {
            Some(idx) => (&s[..idx], Some(&s[idx + 1..])),
            None => (s, None),
        };

        if date_part.len() != 8 || !date_part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("expected an 8-digit date (YYYYMMDD), found \"{}\"", date_part));
        }
        let year: i32 = date_part[0..4].parse().unwrap();
        let month: u32 = date_part[4..6].parse().unwrap();
        let day: u32 = date_part[6..8].parse().unwrap();
        if !(1..=12).contains(&month) {
            return Err(format!("month {:02} is out of range (expected 01-12)", month));
        }
        if !(1..=31).contains(&day) {
            return Err(format!("day {:02} is out of range (expected 01-31)", day));
        }

        let (hour, minute, second) = match time_part {
            None => (0, 0, 0),
            Some(raw) => {
                let t = raw.strip_suffix('Z').unwrap_or(raw);
                if t.len() != 6 || !t.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(format!(
                        "expected a 6-digit time (HHMMSS) after 'T', found \"{}\"",
                        raw
                    ));
                }
                let hour: u32 = t[0..2].parse().unwrap();
                let minute: u32 = t[2..4].parse().unwrap();
                let second: u32 = t[4..6].parse().unwrap();
                if hour > 23 || minute > 59 || second > 59 {
                    return Err(format!(
                        "time {:02}:{:02}:{:02} is out of range",
                        hour, minute, second
                    ));
                }
                (hour, minute, second)
            }
        };

        Ok(DateTimeValue { year, month, day, hour, minute, second })
    }
}

impl fmt::Display for DateTimeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

pub fn now_utc() -> DateTimeValue {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock reports a time before 1970")
        .as_secs() as i64;
    from_unix_time(secs)
}

fn from_unix_time(total_secs: i64) -> DateTimeValue {
    let days = total_secs.div_euclid(86400);
    let secs_of_day = total_secs.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    DateTimeValue {
        year,
        month,
        day,
        hour: (secs_of_day / 3600) as u32,
        minute: ((secs_of_day % 3600) / 60) as u32,
        second: (secs_of_day % 60) as u32,
    }
}

// Howard Hinnant's days-since-epoch to Gregorian civil date algorithm.
// http://howardhinnant.github.io/date_algorithms.html
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m as u32, d as u32)
}
