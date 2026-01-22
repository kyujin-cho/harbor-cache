//! Shared utility functions

use chrono::{DateTime, Utc};

/// Parse a datetime string (RFC3339 format) or return current time
///
/// This helper is used throughout the database layer to handle datetime parsing
/// with a fallback to the current time if parsing fails.
pub fn parse_datetime_or_now(s: &str) -> DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// Format bytes as human-readable string
///
/// Converts byte sizes into human-readable format with appropriate units
/// (B, KB, MB, GB, TB). Uses binary units (1024 base).
///
/// # Examples
///
/// ```
/// use harbor_db::utils::format_bytes;
///
/// assert_eq!(format_bytes(1024), "1.00 KB");
/// assert_eq!(format_bytes(1048576), "1.00 MB");
/// assert_eq!(format_bytes(500), "500 B");
/// ```
pub fn format_bytes(bytes: i64) -> String {
    if bytes < 0 {
        return format!("{} B", bytes);
    }

    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;
    const TB: i64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
        assert_eq!(format_bytes(1099511627776), "1.00 TB");
        assert_eq!(format_bytes(-100), "-100 B");
    }

    #[test]
    fn test_parse_datetime_or_now() {
        let valid_time = "2024-01-01T12:00:00Z";
        let parsed = parse_datetime_or_now(valid_time);
        assert_eq!(parsed.to_rfc3339(), "2024-01-01T12:00:00+00:00");

        // Invalid time should return current time (just check it doesn't panic)
        let invalid_time = "invalid";
        let now_before = Utc::now();
        let parsed = parse_datetime_or_now(invalid_time);
        let now_after = Utc::now();
        assert!(parsed >= now_before && parsed <= now_after);
    }
}
