use crate::datetime::DateTimeValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, Clone)]
pub struct RRule {
    pub freq: Freq,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<DateTimeValue>,
}

// Error offsets are in characters from the start of the RRULE value, so the
// caller can turn them into a column.
pub type RuleError = (usize, String);

impl RRule {
    pub fn parse(value: &str) -> Result<RRule, RuleError> {
        let mut freq = None;
        let mut interval = 1u32;
        let mut count = None;
        let mut until = None;

        let mut offset = 0usize;
        for part in value.split(';') {
            let part_len = part.chars().count();
            if !part.is_empty() {
                let (key, val) = match part.split_once('=') {
                    Some(kv) => kv,
                    None => {
                        return Err((offset, format!("expected NAME=VALUE, found \"{}\"", part)));
                    }
                };
                let val_offset = offset + key.chars().count() + 1;
                match key.to_ascii_uppercase().as_str() {
                    "FREQ" => {
                        freq = Some(match val.to_ascii_uppercase().as_str() {
                            "DAILY" => Freq::Daily,
                            "WEEKLY" => Freq::Weekly,
                            "MONTHLY" => Freq::Monthly,
                            other => {
                                return Err((
                                    val_offset,
                                    format!(
                                        "FREQ={} is not supported (only DAILY, WEEKLY and MONTHLY)",
                                        other
                                    ),
                                ));
                            }
                        });
                    }
                    "INTERVAL" => interval = parse_positive(val, "INTERVAL", val_offset)?,
                    "COUNT" => count = Some(parse_positive(val, "COUNT", val_offset)?),
                    "UNTIL" => {
                        let mut parsed = DateTimeValue::parse(val)
                            .map_err(|msg| (val_offset, format!("invalid UNTIL: {}", msg)))?;
                        // UNTIL is inclusive, so a bare date has to cover that whole day.
                        if DateTimeValue::is_date_only(val) {
                            parsed.hour = 23;
                            parsed.minute = 59;
                            parsed.second = 59;
                        }
                        until = Some(parsed);
                    }
                    other => {
                        return Err((
                            offset,
                            format!("RRULE part {} is not supported", other),
                        ));
                    }
                }
            }
            offset += part_len + 1;
        }

        if count.is_some() && until.is_some() {
            return Err((0, "COUNT and UNTIL must not appear in the same RRULE".to_string()));
        }
        match freq {
            Some(freq) => Ok(RRule { freq, interval, count, until }),
            None => Err((0, "RRULE has no FREQ part".to_string())),
        }
    }

    // First occurrence at or after `reference`, counting `start` as the first
    // occurrence. Returns None once COUNT or UNTIL is exhausted.
    pub fn next_on_or_after(
        &self,
        start: DateTimeValue,
        reference: DateTimeValue,
    ) -> Option<DateTimeValue> {
        let step = self.interval as i64;
        let mut produced = 0u32;
        let mut k: i64 = 0;
        loop {
            let candidate = match self.freq {
                Freq::Daily => Some(start.add_days(k * step)),
                Freq::Weekly => Some(start.add_days(7 * k * step)),
                Freq::Monthly => {
                    if start.year as i64 + k * step / 12 > 9999 {
                        return None;
                    }
                    start.add_months(k * step)
                }
            };
            k += 1;

            let candidate = match candidate {
                Some(c) => c,
                None => continue,
            };
            if candidate.year > 9999 {
                return None;
            }
            if let Some(until) = self.until {
                if candidate > until {
                    return None;
                }
            }
            produced += 1;
            if let Some(count) = self.count {
                if produced > count {
                    return None;
                }
            }
            if candidate >= reference {
                return Some(candidate);
            }
        }
    }
}

fn parse_positive(val: &str, name: &str, offset: usize) -> Result<u32, RuleError> {
    match val.parse::<u32>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err((
            offset,
            format!("{} must be a positive integer, found \"{}\"", name, val),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> DateTimeValue {
        DateTimeValue::parse(s).unwrap()
    }

    #[test]
    fn daily_with_interval() {
        let rule = RRule::parse("FREQ=DAILY;INTERVAL=3").unwrap();
        let next = rule.next_on_or_after(dt("20250101T090000Z"), dt("20250105T000000Z"));
        assert_eq!(next, Some(dt("20250107T090000Z")));
    }

    #[test]
    fn start_itself_counts_as_first_occurrence() {
        let rule = RRule::parse("FREQ=WEEKLY").unwrap();
        let next = rule.next_on_or_after(dt("20250101T090000Z"), dt("20250101T090000Z"));
        assert_eq!(next, Some(dt("20250101T090000Z")));
    }

    #[test]
    fn weekly_steps_seven_days() {
        let rule = RRule::parse("FREQ=WEEKLY").unwrap();
        let next = rule.next_on_or_after(dt("20250101T090000Z"), dt("20250102T000000Z"));
        assert_eq!(next, Some(dt("20250108T090000Z")));
    }

    #[test]
    fn count_limits_occurrences() {
        let rule = RRule::parse("FREQ=DAILY;COUNT=3").unwrap();
        let start = dt("20250101T090000Z");
        assert_eq!(rule.next_on_or_after(start, dt("20250103T000000Z")), Some(dt("20250103T090000Z")));
        assert_eq!(rule.next_on_or_after(start, dt("20250104T000000Z")), None);
    }

    #[test]
    fn until_date_is_inclusive() {
        let rule = RRule::parse("FREQ=DAILY;UNTIL=20250103").unwrap();
        let start = dt("20250101T090000Z");
        assert_eq!(rule.next_on_or_after(start, dt("20250103T000000Z")), Some(dt("20250103T090000Z")));
        assert_eq!(rule.next_on_or_after(start, dt("20250103T100000Z")), None);
    }

    #[test]
    fn monthly_skips_months_without_the_day() {
        let rule = RRule::parse("FREQ=MONTHLY").unwrap();
        let next = rule.next_on_or_after(dt("20250131T120000Z"), dt("20250201T000000Z"));
        assert_eq!(next, Some(dt("20250331T120000Z")));
    }

    #[test]
    fn count_only_counts_occurrences_that_exist() {
        let rule = RRule::parse("FREQ=MONTHLY;COUNT=2").unwrap();
        let start = dt("20250131T120000Z");
        assert_eq!(rule.next_on_or_after(start, dt("20250301T000000Z")), Some(dt("20250331T120000Z")));
        assert_eq!(rule.next_on_or_after(start, dt("20250401T000000Z")), None);
    }

    #[test]
    fn unsupported_part_points_at_its_name() {
        let (offset, msg) = RRule::parse("FREQ=WEEKLY;BYDAY=MO").unwrap_err();
        assert_eq!(offset, 12);
        assert!(msg.contains("BYDAY"), "unexpected message: {}", msg);
    }

    #[test]
    fn bad_values_point_at_the_value() {
        let (offset, _) = RRule::parse("FREQ=DAILY;INTERVAL=0").unwrap_err();
        assert_eq!(offset, 20);
        let (offset, _) = RRule::parse("FREQ=YEARLY").unwrap_err();
        assert_eq!(offset, 5);
    }

    #[test]
    fn missing_freq_and_count_with_until_are_rejected() {
        assert!(RRule::parse("INTERVAL=2").is_err());
        assert!(RRule::parse("FREQ=DAILY;COUNT=2;UNTIL=20250101").is_err());
    }
}
