//! A measured quantity as a person reads it: the unit on the number, and
//! only as many digits as the number has to say.
//!
//! Here rather than in a surface for the reason [`crate::text`] is: the
//! receipt the engine writes and the summary the CLI prints say the same
//! number about the same run, and two formatters are two answers to how
//! that number reads.

use std::fmt;
use std::time::Duration;

/// A count of tokens.
///
/// Exact below a thousand and three significant digits above, with the
/// scale on the number — `999`, `20.2k`, `1.86M` — and no zero after the
/// point that is only padding: `20k`, never `20.0k`, which would claim a
/// digit the count was rounded away from. The figure takes
/// [`Tokens::WIDEST`] cells or fewer for any count under a thousand
/// trillion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tokens(pub u64);

impl Tokens {
    /// The cells the widest figure takes: three digits, a point and a
    /// scale.
    pub const WIDEST: usize = 5;

    /// A mean or a median, which a derivation computes as a fraction,
    /// as the whole count it stands for.
    pub fn rounded(value: f64) -> Self {
        // A count is never negative; the cast saturates past `u64::MAX`.
        Tokens(value.max(0.0).round() as u64)
    }

    /// The count without its unit, for a line whose label already says
    /// what is counted: `tokens: 1.86M in / 20.2k out`.
    pub fn figure(self) -> String {
        significant(self.0)
    }

    /// The count in every digit, its unit on it, for a line that sets it
    /// beside a limit written in the same digits: rounded, a count just
    /// past its limit would read as equal to it.
    pub fn exact(self) -> String {
        let unit = if self.0 == 1 { "token" } else { "tokens" };
        format!("{} {unit}", self.0)
    }

    /// The count in a column of counts: the figure right-aligned to
    /// [`Tokens::WIDEST`] and its unit beside it, so the figures line up
    /// on their last digit and every row is as wide as the next.
    pub fn column(self) -> String {
        let unit = if self.0 == 1 { "token " } else { "tokens" };
        format!("{:>width$} {unit}", self.figure(), width = Self::WIDEST)
    }
}

impl fmt::Display for Tokens {
    /// The figure and its unit: `1 token`, `20.2k tokens`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = if self.0 == 1 { "token" } else { "tokens" };
        f.pad(&format!("{} {unit}", self.figure()))
    }
}

/// Each scale a count is read in, smallest first.
const SCALES: [(u64, &str); 4] = [
    (1_000, "k"),
    (1_000_000, "M"),
    (1_000_000_000, "B"),
    (1_000_000_000_000, "T"),
];

/// `n` in three significant digits, its scale on it.
fn significant(n: u64) -> String {
    let n = u128::from(n);
    let digits = n.checked_ilog10().unwrap_or(0) + 1;
    if digits <= 3 {
        return n.to_string();
    }
    let step = 10u128.pow(digits - 3);
    // Half up, the rounding a reader does in their head.
    let rounded = (n + step / 2) / step * step;
    let (scale, suffix) = SCALES
        .iter()
        .rev()
        .map(|(scale, suffix)| (u128::from(*scale), *suffix))
        .find(|(scale, _)| rounded >= *scale)
        .unwrap_or((1, ""));
    let hundredths = rounded * 100 / scale;
    let whole = hundredths / 100;
    let fraction = format!("{:02}", hundredths % 100);
    let fraction = fraction.trim_end_matches('0');
    match fraction.is_empty() {
        true => format!("{whole}{suffix}"),
        false => format!("{whole}.{fraction}{suffix}"),
    }
}

/// One quantity over another: a share of the whole up to one, read as
/// whole percent, and a multiple of it past one, read with the sign of
/// one — `35%`, `100%`, `1.5×`, `120×`. A rate past one hundred percent
/// is not a share of anything, and `11960%` asks a reader to divide by
/// a hundred to learn what `120×` says.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Ratio(pub f64);

impl Ratio {
    /// The ratio as it reads, `times` being the multiplication sign the
    /// terminal it lands on draws. A ratio that is not a number reads
    /// as `n/a`.
    pub fn reads(self, times: char) -> String {
        let Ratio(value) = self;
        if !value.is_finite() {
            return "n/a".to_string();
        }
        if value <= 1.0 {
            return format!("{:.0}%", value * 100.0);
        }
        let decimals = match value {
            v if v < 10.0 => 2,
            v if v < 100.0 => 1,
            _ => 0,
        };
        let fixed = format!("{value:.decimals$}");
        let fixed = match fixed.contains('.') {
            true => fixed.trim_end_matches('0').trim_end_matches('.'),
            false => fixed.as_str(),
        };
        format!("{fixed}{times}")
    }
}

/// The cells the widest [`duration`] takes, for a column that holds one.
pub const DURATION_WIDEST: usize = 6;

/// `duration` as a person reads one: seconds below a minute, minutes and
/// seconds below an hour, hours and minutes below a day, and days and
/// hours past it. The unit is always on the number, so no column header
/// has to carry it.
///
/// [`DURATION_WIDEST`] cells or fewer for anything under ten thousand
/// days — `9s`, `59m59s`, `23h59m`, `99d23h`, `100d` — so a column that
/// right-aligns to it never moves under the reader as a run ages.
pub fn duration(duration: Duration) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    let total = duration.as_secs();
    match total {
        t if t < MINUTE => format!("{t}s"),
        t if t < HOUR => format!("{}m{:02}s", t / MINUTE, t % MINUTE),
        t if t < DAY => format!("{}h{:02}m", t / HOUR, (t % HOUR) / MINUTE),
        t if t < 100 * DAY => format!("{}d{:02}h", t / DAY, (t % DAY) / HOUR),
        t => format!("{}d", t / DAY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn figure(n: u64) -> String {
        Tokens(n).figure()
    }

    #[test]
    fn tokens_read_in_three_significant_digits_without_a_false_decimal() {
        assert_eq!(figure(0), "0");
        assert_eq!(figure(999), "999");
        assert_eq!(figure(1_000), "1k");
        assert_eq!(figure(1_049), "1.05k");
        assert_eq!(figure(20_000), "20k");
        assert_eq!(figure(20_213), "20.2k");
        assert_eq!(figure(123_456), "123k");
        assert_eq!(figure(1_859_898), "1.86M");
        assert_eq!(figure(2_500_000_000), "2.5B");
    }

    #[test]
    fn a_count_that_rounds_up_to_the_next_scale_reads_in_it() {
        assert_eq!(figure(999_499), "999k");
        assert_eq!(figure(999_500), "1M");
        assert_eq!(figure(999_999_999), "1B");
    }

    #[test]
    fn every_figure_under_a_thousand_trillion_fits_the_widest() {
        let mut n: u64 = 1;
        while n < 1_000_000_000_000_000 {
            for count in [n, n * 5 - 1, n * 9 + n / 2] {
                let shown = figure(count);
                assert!(shown.len() <= Tokens::WIDEST, "{count} reads as {shown}");
            }
            n *= 10;
        }
    }

    #[test]
    fn a_count_names_its_unit_in_the_number_it_takes() {
        assert_eq!(Tokens(1).to_string(), "1 token");
        assert_eq!(Tokens(1_000).to_string(), "1k tokens");
        assert_eq!(Tokens::rounded(499.6).to_string(), "500 tokens");
        assert_eq!(Tokens(2_000_431).exact(), "2000431 tokens");
    }

    #[test]
    fn a_column_of_counts_lines_up_on_the_last_digit() {
        let rows = [
            Tokens(1).column(),
            Tokens(20_213).column(),
            Tokens(999).column(),
        ];
        assert_eq!(rows, ["    1 token ", "20.2k tokens", "  999 tokens"]);
    }

    #[test]
    fn a_ratio_up_to_one_reads_as_a_share() {
        assert_eq!(Ratio(0.0).reads('×'), "0%");
        assert_eq!(Ratio(0.354).reads('×'), "35%");
        assert_eq!(Ratio(1.0).reads('×'), "100%");
    }

    #[test]
    fn a_ratio_above_one_reads_as_a_multiple() {
        assert_eq!(Ratio(1.5).reads('×'), "1.5×");
        assert_eq!(Ratio(1.04).reads('x'), "1.04x");
        assert_eq!(Ratio(2.0).reads('×'), "2×");
        assert_eq!(Ratio(11.96).reads('×'), "12×");
        assert_eq!(Ratio(119.6).reads('×'), "120×");
        assert_eq!(Ratio(f64::NAN).reads('×'), "n/a");
    }

    #[test]
    fn a_duration_names_the_units_it_carries() {
        let read = |secs| duration(Duration::from_secs(secs));
        assert_eq!(read(9), "9s");
        assert_eq!(read(60), "1m00s");
        assert_eq!(read(3_599), "59m59s");
        assert_eq!(read(3_600), "1h00m");
        assert_eq!(read(86_399), "23h59m");
    }

    #[test]
    fn a_duration_past_a_day_names_days_and_hours() {
        let read = |secs| duration(Duration::from_secs(secs));
        assert_eq!(read(86_400), "1d00h");
        assert_eq!(read(5 * 86_400 + 13 * 3_600 + 59), "5d13h");
        assert_eq!(read(100 * 86_400 + 3_600), "100d");
    }

    #[test]
    fn every_duration_under_ten_thousand_days_fits_the_widest() {
        for secs in [0, 59, 3_599, 86_399, 99 * 86_400 + 86_399, 9_999 * 86_400] {
            let shown = duration(Duration::from_secs(secs));
            assert!(shown.len() <= DURATION_WIDEST, "{secs}s reads as {shown}");
        }
    }
}
