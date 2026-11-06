//! String and duration formatting helpers for CLI outputs.

use chrono::{DateTime, Utc};

/// Converts an ISO-8601 UTC timestamp to a human-readable relative duration.
#[must_use]
pub fn format_human_time(iso_str: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(iso_str) else {
        return iso_str.get(..19).unwrap_or(iso_str).to_string();
    };
    let now = Utc::now();
    let diff = now.signed_duration_since(dt.with_timezone(&Utc));
    let secs = diff.num_seconds();

    if secs < 0 || secs < 45 {
        "just now".to_string()
    } else if secs < 90 {
        "1m ago".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 7200 {
        "1h ago".to_string()
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 172_800 {
        "yesterday".to_string()
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Formats the elapsed duration between two ISO-8601 timestamps.
#[must_use]
pub fn format_duration_between(start_iso: &str, end_iso: &str) -> String {
    let Ok(start_dt) = DateTime::parse_from_rfc3339(start_iso) else {
        return "unknown".to_string();
    };
    let Ok(end_dt) = DateTime::parse_from_rfc3339(end_iso) else {
        return "unknown".to_string();
    };
    let diff = end_dt.signed_duration_since(start_dt);
    format_seconds(diff.num_seconds().max(0))
}

/// Formats the elapsed duration from an ISO-8601 timestamp until current UTC time.
#[must_use]
pub fn format_duration_to_now(start_iso: &str) -> String {
    let Ok(start_dt) = DateTime::parse_from_rfc3339(start_iso) else {
        return "unknown".to_string();
    };
    let now = Utc::now();
    let diff = now.signed_duration_since(start_dt.with_timezone(&Utc));
    format_seconds(diff.num_seconds().max(0))
}

fn format_seconds(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let mins = secs / 60;
        let rem_s = secs % 60;
        if rem_s == 0 {
            format!("{mins}m")
        } else {
            format!("{mins}m {rem_s:02}s")
        }
    } else {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        format!("{hours}h {mins}m")
    }
}

/// Returns the visible character length of a string, ignoring ANSI escape sequences.
#[must_use]
pub fn visible_len(s: &str) -> usize {
    let mut in_esc = false;
    let mut len = 0;
    for c in s.chars() {
        if c == '\x1b' {
            in_esc = true;
        } else if in_esc {
            if c == 'm' {
                in_esc = false;
            }
        } else {
            len += 1;
        }
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_human_time_returns_relative_strings() {
        let now_str = Utc::now().to_rfc3339();
        assert_eq!(format_human_time(&now_str), "just now");

        let five_mins_ago = (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        assert_eq!(format_human_time(&five_mins_ago), "5m ago");

        let two_hours_ago = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        assert_eq!(format_human_time(&two_hours_ago), "2h ago");
    }

    #[test]
    fn format_seconds_works() {
        assert_eq!(format_seconds(45), "45s");
        assert_eq!(format_seconds(60), "1m");
        assert_eq!(format_seconds(125), "2m 05s");
        assert_eq!(format_seconds(3660), "1h 1m");
    }
}
