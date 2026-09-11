//! A measured quantity as it reads in a column.

use std::time::Duration;

/// `duration` as a person reads one: seconds below a minute, minutes and
/// seconds below an hour, hours and minutes above it. The unit is always
/// on the number, so no column header has to carry it.
///
/// The result is two to six cells wide over the range a run spans — `9s`,
/// `59s`, `1m00s`, `59m59s`, `1h00m`, `23h59m` — and one cell wider for
/// every digit an hour count adds beyond two. A column that holds it
/// therefore pads or right-aligns it to a fixed width; a column that
/// takes it as it comes moves under the reader as the run gets longer.
pub(crate) fn format_duration(duration: Duration) -> String {
    let total = duration.as_secs();
    if total < 60 {
        format!("{total}s")
    } else if total < 3600 {
        format!("{}m{:02}s", total / 60, total % 60)
    } else {
        format!("{}h{:02}m", total / 3600, (total % 3600) / 60)
    }
}

/// `fraction` as whole percent, right-aligned in four cells — `  0%`
/// through `100%` — so a column of them lines up on the sign and the
/// eye compares the digits, not their left edges.
pub(crate) fn format_pct(fraction: f64) -> String {
    format!("{:>3.0}%", fraction * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::cell_width;

    #[test]
    fn a_duration_names_the_units_it_carries() {
        assert_eq!(format_duration(Duration::from_secs(9)), "9s");
        assert_eq!(format_duration(Duration::from_secs(59)), "59s");
        assert_eq!(format_duration(Duration::from_secs(60)), "1m00s");
        assert_eq!(format_duration(Duration::from_secs(3599)), "59m59s");
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h00m");
        assert_eq!(format_duration(Duration::from_secs(86_399)), "23h59m");
    }

    #[test]
    fn a_duration_stays_within_the_width_a_column_sizes_for() {
        for secs in [0, 9, 59, 60, 3599, 3600, 86_399] {
            let rendered = format_duration(Duration::from_secs(secs));
            let cells = cell_width(&rendered);
            assert!((2..=6).contains(&cells), "{rendered} is {cells} cells");
        }
    }

    #[test]
    fn a_percentage_is_always_four_cells() {
        for fraction in [0.0, 0.005, 0.5, 1.0] {
            assert_eq!(cell_width(&format_pct(fraction)), 4, "{fraction}");
        }
    }
}
