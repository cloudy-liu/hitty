//! Calendar dates shared by crash records and commit metadata.
pub fn format_date(unix: i64, offset_minutes: i32) -> String {
    let local = unix.saturating_add(i64::from(offset_minutes) * 60);
    let (year, month, day) = civil_from_days(local.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::format_date;

    #[test]
    fn dates_respect_the_authored_timezone_and_calendar_boundaries() {
        assert_eq!(format_date(0, 0), "1970-01-01");
        assert_eq!(format_date(0, -60), "1969-12-31");
        assert_eq!(format_date(86_399, 60), "1970-01-02");
        assert_eq!(format_date(19_782 * 86_400, 0), "2024-02-29");
    }
}
