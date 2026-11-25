use crate::model::StationTiming;

/// The median is taken over at most this many newest samples.
pub const FACTOR_WINDOW: usize = 20;
/// Fewer samples than this and the scope is not trusted.
pub const FACTOR_MIN_SAMPLES: usize = 5;
/// A handful of wild samples (a station left open overnight) must not make
/// every later ETA absurd, so the factor is clamped.
pub const FACTOR_MIN: f64 = 0.25;
pub const FACTOR_MAX: f64 = 8.0;

/// Median of actual/estimate over the project's newest `FACTOR_WINDOW`
/// samples once it has `FACTOR_MIN_SAMPLES`; else the same over all samples;
/// else 1. `samples` is oldest first.
pub fn correction_factor(samples: &[StationTiming], project: Option<&str>) -> f64 {
    let ratios = |scope: &dyn Fn(&StationTiming) -> bool| -> Vec<f64> {
        samples
            .iter()
            .rev()
            .filter(|s| scope(s) && s.estimate_secs > 0)
            .take(FACTOR_WINDOW)
            .map(|s| f64::from(s.actual_secs) / f64::from(s.estimate_secs))
            .collect()
    };
    let own = project.map(|p| ratios(&|s| s.project.as_deref() == Some(p)));
    let pick = match own {
        Some(r) if r.len() >= FACTOR_MIN_SAMPLES => r,
        _ => ratios(&|_| true),
    };
    if pick.len() < FACTOR_MIN_SAMPLES {
        return 1.0;
    }
    median(pick).clamp(FACTOR_MIN, FACTOR_MAX)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(project: Option<&str>, est: u32, actual: u32) -> StationTiming {
        StationTiming {
            key: format!("k{est}-{actual}"),
            project: project.map(str::to_string),
            estimate_secs: est,
            actual_secs: actual,
        }
    }

    #[test]
    fn below_minimum_samples_is_one() {
        let v: Vec<_> = (0..FACTOR_MIN_SAMPLES - 1)
            .map(|_| s(Some("a"), 60, 180))
            .collect();
        assert_eq!(correction_factor(&v, Some("a")), 1.0);
        assert_eq!(correction_factor(&v, None), 1.0);
        assert_eq!(correction_factor(&[], Some("a")), 1.0);
    }

    #[test]
    fn project_median_once_it_has_enough() {
        let mut v: Vec<_> = [1, 2, 3, 4, 5]
            .iter()
            .map(|&k| s(Some("a"), 100, 100 * k))
            .collect();
        v.extend((0..10).map(|_| s(Some("b"), 100, 50)));
        assert_eq!(correction_factor(&v, Some("a")), 3.0);
        assert_eq!(correction_factor(&v, Some("b")), 0.5);
    }

    #[test]
    fn falls_back_to_global_when_the_project_is_thin() {
        let mut v: Vec<_> = (0..6).map(|_| s(Some("b"), 100, 200)).collect();
        v.push(s(Some("a"), 100, 800));
        assert_eq!(correction_factor(&v, Some("a")), 2.0);
        assert_eq!(correction_factor(&v, None), 2.0);
    }

    #[test]
    fn even_count_takes_the_middle_mean() {
        let v: Vec<_> = [1, 2, 3, 4, 5, 6]
            .iter()
            .map(|&k| s(None, 10, 10 * k))
            .collect();
        assert_eq!(correction_factor(&v, None), 3.5);
    }

    #[test]
    fn only_the_newest_window_counts() {
        let mut v: Vec<_> = (0..50).map(|_| s(None, 100, 700)).collect();
        v.extend((0..FACTOR_WINDOW).map(|_| s(None, 100, 150)));
        assert_eq!(correction_factor(&v, None), 1.5);
    }

    #[test]
    fn factor_is_clamped() {
        let slow: Vec<_> = (0..5).map(|_| s(None, 1, 1000)).collect();
        assert_eq!(correction_factor(&slow, None), FACTOR_MAX);
        let fast: Vec<_> = (0..5).map(|_| s(None, 1000, 1)).collect();
        assert_eq!(correction_factor(&fast, None), FACTOR_MIN);
    }
}
