use std::collections::BTreeSet;

use chrono::Timelike;

const MINUTES_PER_DAY: u16 = 24 * 60;

pub fn train_number(departure_local: chrono::NaiveDateTime, taken: &BTreeSet<u16>) -> u16 {
    let start = (departure_local.hour() * 60 + departure_local.minute()) as u16;
    (0..MINUTES_PER_DAY)
        .map(|i| {
            let m = (start + i) % MINUTES_PER_DAY;
            (m / 60) * 100 + m % 60
        })
        .find(|n| !taken.contains(n))
        .unwrap_or((start / 60) * 100 + start % 60)
}

pub fn format_train_number(n: u16) -> String {
    format!("{n:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hhmm_and_collision() {
        let t = chrono::NaiveDate::from_ymd_opt(2026, 11, 23)
            .unwrap()
            .and_hms_opt(14, 23, 0)
            .unwrap();
        assert_eq!(train_number(t, &BTreeSet::new()), 1423);
        assert_eq!(train_number(t, &[1423, 1424].into()), 1425);
        let late = t.date().and_hms_opt(23, 59, 0).unwrap();
        assert_eq!(train_number(late, &[2359].into()), 0);
        assert_eq!(format_train_number(942), "942");
        assert_eq!(format_train_number(5), "005");
        assert_eq!(format_train_number(1423), "1423");
    }

    #[test]
    fn collision_skips_to_next_hour() {
        let t = chrono::NaiveDate::from_ymd_opt(2026, 11, 23)
            .unwrap()
            .and_hms_opt(9, 59, 30)
            .unwrap();
        assert_eq!(train_number(t, &[959].into()), 1000);
    }

    #[test]
    fn full_day_falls_back_to_own_minute() {
        let t = chrono::NaiveDate::from_ymd_opt(2026, 11, 23)
            .unwrap()
            .and_hms_opt(8, 5, 0)
            .unwrap();
        let all: BTreeSet<u16> = (0..24)
            .flat_map(|h| (0..60).map(move |m| h * 100 + m))
            .collect();
        assert_eq!(train_number(t, &all), 805);
    }
}
