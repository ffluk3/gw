//! Parsing human-written durations.
//!
//! A configuration file should read the way a person writes an interval:
//! `"3d"`, `"5w"`, `"90m"`. The units match what
//! [`crate::render::human::short_age`] displays, so a value read from the file
//! and an age shown in a table are spelled the same way.
//!
//! This module is a total function over text. It never reads the environment
//! or the clock; the caller supplies whatever "now" it is comparing against.

/// Units accepted by [`parse_duration_secs`], longest suffix first so `mo` is
/// matched before `m`.
const UNITS: [(&str, i64); 6] = [
    ("mo", 30 * 86_400),
    ("s", 1),
    ("m", 60),
    ("h", 3_600),
    ("d", 86_400),
    ("w", 7 * 86_400),
];

/// The forms a caller may write, for an error message.
pub const ACCEPTED_FORMS: &str = "a positive whole number followed by one of \
     s, m, h, d, w, mo (for example \"3d\" or \"5w\")";

/// Parse a duration such as `"3d"` or `"5w"` into seconds.
///
/// Returns a message suitable for a configuration error. Zero is rejected: a
/// zero window would make every worktree stale, which is never what someone
/// means to configure, and the same effect is available through the flag for
/// anyone who really wants it.
pub fn parse_duration_secs(text: &str) -> Result<i64, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(format!("duration is empty; expected {ACCEPTED_FORMS}"));
    }

    let Some((value, unit)) = split_value_and_unit(text) else {
        return Err(format!(
            "'{text}' is not a duration; expected {ACCEPTED_FORMS}"
        ));
    };

    let multiplier = UNITS
        .iter()
        .find(|(suffix, _)| *suffix == unit)
        .map(|(_, seconds)| *seconds)
        .ok_or_else(|| {
            format!("unknown duration unit '{unit}' in '{text}'; expected {ACCEPTED_FORMS}")
        })?;

    let value: i64 = value
        .parse()
        .map_err(|_| format!("'{text}' is not a duration; expected {ACCEPTED_FORMS}"))?;
    if value <= 0 {
        return Err(format!(
            "'{text}' must be greater than zero; expected {ACCEPTED_FORMS}"
        ));
    }

    value
        .checked_mul(multiplier)
        .ok_or_else(|| format!("'{text}' is too large"))
}

/// Render a duration in the same units [`parse_duration_secs`] accepts.
///
/// This is for a *configured limit*, which is not an age: a five-week window
/// is `"5w"`, never `"1mo ago"`. The largest unit that divides evenly wins, so
/// a value round-trips through parse and render unchanged.
#[must_use]
pub fn format_duration_secs(seconds: i64) -> String {
    if seconds <= 0 {
        return "0s".to_string();
    }
    // Pick the largest unit that divides evenly. `UNITS` is not ordered by
    // size, so compare rather than relying on iteration order.
    let best = UNITS
        .iter()
        .filter(|(_, size)| *size > 1 && seconds % size == 0)
        .max_by_key(|(_, size)| *size);
    match best {
        Some((suffix, size)) => format!("{}{suffix}", seconds / size),
        None => format!("{seconds}s"),
    }
}

/// Split `"5mo"` into `("5", "mo")`, accepting letters only for the unit so a
/// stray digit is reported as the whole value rather than a unit typo.
fn split_value_and_unit(text: &str) -> Option<(&str, &str)> {
    let unit_start = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    if unit_start == 0 || unit_start == text.len() {
        return None;
    }
    let (value, unit) = text.split_at(unit_start);
    if !unit.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some((value, unit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_examples_from_the_docs_parse() {
        assert_eq!(parse_duration_secs("3d").unwrap(), 3 * 86_400);
        assert_eq!(parse_duration_secs("5w").unwrap(), 5 * 7 * 86_400);
    }

    #[test]
    fn every_accepted_unit_parses() {
        assert_eq!(parse_duration_secs("90s").unwrap(), 90);
        assert_eq!(parse_duration_secs("30m").unwrap(), 30 * 60);
        assert_eq!(parse_duration_secs("2h").unwrap(), 2 * 3_600);
        assert_eq!(parse_duration_secs("1d").unwrap(), 86_400);
        assert_eq!(parse_duration_secs("2w").unwrap(), 2 * 7 * 86_400);
        assert_eq!(parse_duration_secs("1mo").unwrap(), 30 * 86_400);
    }

    #[test]
    fn the_longest_suffix_wins() {
        // `mo` must not be read as `m` followed by a stray `o`.
        assert_eq!(parse_duration_secs("2mo").unwrap(), 60 * 86_400);
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        assert_eq!(parse_duration_secs("  7d ").unwrap(), 7 * 86_400);
    }

    #[test]
    fn zero_is_rejected_because_it_would_mark_everything_stale() {
        let error = parse_duration_secs("0d").unwrap_err();
        assert!(error.contains("greater than zero"), "{error}");
    }

    #[test]
    fn a_missing_unit_is_rejected_with_the_accepted_forms() {
        let error = parse_duration_secs("7").unwrap_err();
        assert!(error.contains("s, m, h, d, w, mo"), "{error}");
    }

    #[test]
    fn an_unknown_unit_names_itself() {
        let error = parse_duration_secs("5x").unwrap_err();
        assert!(error.contains("unknown duration unit 'x'"), "{error}");
    }

    #[test]
    fn a_negative_value_is_not_mistaken_for_a_duration() {
        let error = parse_duration_secs("-3d").unwrap_err();
        assert!(error.contains("is not a duration"), "{error}");
    }

    #[test]
    fn an_empty_value_is_rejected() {
        assert!(parse_duration_secs("").is_err());
        assert!(parse_duration_secs("   ").is_err());
    }

    #[test]
    fn a_limit_renders_in_the_units_it_is_written_in() {
        assert_eq!(format_duration_secs(3 * 86_400), "3d");
        assert_eq!(format_duration_secs(5 * 7 * 86_400), "5w");
        assert_eq!(format_duration_secs(30 * 86_400), "1mo");
        assert_eq!(format_duration_secs(90), "90s");
    }

    #[test]
    fn parse_and_render_round_trip() {
        for text in ["3d", "5w", "2mo", "90m", "1h"] {
            let seconds = parse_duration_secs(text).unwrap();
            assert_eq!(format_duration_secs(seconds), text);
        }
    }

    #[test]
    fn overflow_is_reported_rather_than_wrapping() {
        let error = parse_duration_secs("9223372036854775807w").unwrap_err();
        assert!(error.contains("too large"), "{error}");
    }
}
