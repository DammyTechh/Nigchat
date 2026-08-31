//! Value objects.
//!
//! Each type here can only be constructed through a constructor that enforces
//! its invariant. Once you hold a `PhoneNumber`, it is valid E.164 — no
//! handler needs to re-check, and no code path can forget to.

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{DomainError, DomainResult};

// ---------------------------------------------------------------------------
// Phone number
// ---------------------------------------------------------------------------

/// A phone number in E.164 canonical form (spec §3).
///
/// NigChat is explicitly global, so nothing here assumes a country. Full
/// national-format parsing belongs on the client with a real libphonenumber
/// port; this is the server-side floor that guarantees storage consistency.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PhoneNumber(String);

impl PhoneNumber {
    pub fn parse(input: &str) -> DomainResult<Self> {
        let cleaned: String = input
            .chars()
            .filter(|c| !c.is_whitespace() && !matches!(c, '-' | '(' | ')' | '.'))
            .collect();

        if !cleaned.starts_with('+') {
            return Err(DomainError::validation(
                "phone number must start with '+' and a country code",
            ));
        }

        let digits = &cleaned[1..];

        if !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(DomainError::validation(
                "phone number may contain digits only after '+'",
            ));
        }

        // E.164 caps the total at 15 digits. The lower bound is deliberately
        // loose because short national numbering plans exist.
        if !(7..=15).contains(&digits.len()) {
            return Err(DomainError::validation(
                "phone number must be between 7 and 15 digits",
            ));
        }

        // No country code begins with 0.
        if digits.starts_with('0') {
            return Err(DomainError::validation("invalid country code"));
        }

        Ok(Self(cleaned))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Safe for logs and support tooling. Phone numbers are personal data and
    /// must never appear in full in a log line (spec §14).
    pub fn redacted(&self) -> String {
        let len = self.0.len();
        if len <= 6 {
            return "+***".to_string();
        }
        format!("{}****{}", &self.0[..4], &self.0[len - 2..])
    }
}

impl std::fmt::Display for PhoneNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display is the redacted form on purpose: a stray `{}` in a log
        // statement should not leak a user's number.
        write!(f, "{}", self.redacted())
    }
}

// ---------------------------------------------------------------------------
// Username
// ---------------------------------------------------------------------------

/// Optional public handle (spec §3). Case-insensitive and unique.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Username(String);

/// Handles that must never be claimed by a user, because owning one enables
/// impersonation of the platform itself.
const RESERVED_USERNAMES: [&str; 12] = [
    "admin", "administrator", "nigchat", "support", "help", "security", "system", "root",
    "official", "moderator", "staff", "team",
];

impl Username {
    pub fn parse(input: &str) -> DomainResult<Self> {
        let normalised = input.trim().to_lowercase();

        if !(3..=32).contains(&normalised.chars().count()) {
            return Err(DomainError::validation(
                "username must be between 3 and 32 characters",
            ));
        }

        if !normalised
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
        {
            return Err(DomainError::validation(
                "username may contain letters, digits, '_' and '.' only",
            ));
        }

        if normalised.starts_with(['_', '.']) || normalised.ends_with(['_', '.']) {
            return Err(DomainError::validation(
                "username may not start or end with '_' or '.'",
            ));
        }

        // Blocks the "admin..admin" style of visual confusion.
        if normalised.contains("..") || normalised.contains("__") {
            return Err(DomainError::validation(
                "username may not contain repeated separators",
            ));
        }

        if RESERVED_USERNAMES.contains(&normalised.as_str()) {
            return Err(DomainError::validation("that username is reserved"));
        }

        Ok(Self(normalised))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Sequence numbers
// ---------------------------------------------------------------------------

/// Position of a message within one conversation.
///
/// This — never a timestamp — is the ordering, pagination and sync key.
/// Server clocks disagree with each other; a per-conversation counter cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Seq(pub i64);

impl Seq {
    pub const ZERO: Seq = Seq(0);

    pub fn value(&self) -> i64 {
        self.0
    }

    pub fn next(&self) -> Seq {
        Seq(self.0 + 1)
    }

    /// How many messages sit between a read cursor and the head.
    pub fn distance_from(&self, cursor: Seq) -> i64 {
        (self.0 - cursor.0).max(0)
    }
}

impl std::fmt::Display for Seq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Notification value objects (spec §16)
// ---------------------------------------------------------------------------

/// How much of a message a locked-screen notification may reveal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewMode {
    /// Sender name and decrypted preview, rendered on-device.
    Full,
    /// Sender name only.
    NameOnly,
    /// "New message" with no identifying detail.
    Hidden,
}

impl PreviewMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::NameOnly => "name_only",
            Self::Hidden => "hidden",
        }
    }

    pub fn parse(value: &str) -> DomainResult<Self> {
        match value {
            "full" => Ok(Self::Full),
            "name_only" => Ok(Self::NameOnly),
            "hidden" => Ok(Self::Hidden),
            other => Err(DomainError::validation(format!(
                "unknown preview mode '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vibration {
    Off,
    Short,
    Default,
    Long,
}

impl Vibration {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Short => "short",
            Self::Default => "default",
            Self::Long => "long",
        }
    }

    pub fn parse(value: &str) -> DomainResult<Self> {
        match value {
            "off" => Ok(Self::Off),
            "short" => Ok(Self::Short),
            "default" => Ok(Self::Default),
            "long" => Ok(Self::Long),
            other => Err(DomainError::validation(format!(
                "unknown vibration setting '{other}'"
            ))),
        }
    }
}

/// A do-not-disturb window in the user's own local time.
///
/// Stored as minutes-past-midnight plus an IANA timezone rather than as UTC
/// instants, so "quiet from 22:00" keeps meaning 22:00 after the user flies to
/// another country or the clocks change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuietHours {
    pub start_minute: u16,
    pub end_minute: u16,
    pub timezone: String,
    /// Calls may be allowed to ring through. Messages never do.
    pub allow_calls: bool,
}

impl QuietHours {
    pub fn new(
        start_minute: u16,
        end_minute: u16,
        timezone: impl Into<String>,
        allow_calls: bool,
    ) -> DomainResult<Self> {
        if start_minute > 1439 || end_minute > 1439 {
            return Err(DomainError::validation(
                "quiet hours must be given as minutes past midnight (0–1439)",
            ));
        }
        if start_minute == end_minute {
            return Err(DomainError::validation(
                "quiet hours start and end must differ",
            ));
        }
        Ok(Self {
            start_minute,
            end_minute,
            timezone: timezone.into(),
            allow_calls,
        })
    }

    /// Whether `local_time` falls inside the window.
    ///
    /// The caller converts UTC to the user's zone first; this type only owns
    /// the wrap-around logic, which is the part that gets written wrong.
    /// A window like 22:00–07:00 crosses midnight and is inclusive of the
    /// start, exclusive of the end.
    pub fn contains(&self, local_time: DateTime<impl chrono::TimeZone>) -> bool {
        let minute = (local_time.hour() * 60 + local_time.minute()) as u16;

        if self.start_minute < self.end_minute {
            // Same-day window, e.g. 09:00–17:00.
            minute >= self.start_minute && minute < self.end_minute
        } else {
            // Crosses midnight, e.g. 22:00–07:00.
            minute >= self.start_minute || minute < self.end_minute
        }
    }
}

// ---------------------------------------------------------------------------
// Mute
// ---------------------------------------------------------------------------

/// Per-conversation mute state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MuteState {
    pub muted_until: Option<DateTime<Utc>>,
}

impl MuteState {
    pub fn unmuted() -> Self {
        Self { muted_until: None }
    }

    pub fn is_muted_at(&self, now: DateTime<Utc>) -> bool {
        self.muted_until.is_some_and(|until| until > now)
    }
}

/// The durations the client offers. "Always" is a far-future timestamp rather
/// than a null, so one comparison covers every case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MuteDuration {
    EightHours,
    OneWeek,
    Always,
}

impl MuteDuration {
    pub fn until(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Self::EightHours => now + chrono::Duration::hours(8),
            Self::OneWeek => now + chrono::Duration::weeks(1),
            // Year 2999. Simpler than a nullable column with special cases.
            Self::Always => DateTime::from_timestamp(32_503_680_000, 0).unwrap_or(now),
        }
    }
}

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

/// Keyset pagination over `seq`.
///
/// There is no offset variant, by design: at message 500,000 an OFFSET scan
/// reads half a million rows only to throw them away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub before_seq: Option<Seq>,
    pub after_seq: Option<Seq>,
    pub limit: i64,
}

impl Cursor {
    pub const DEFAULT_LIMIT: i64 = 50;
    pub const MAX_LIMIT: i64 = 200;

    pub fn new(before_seq: Option<i64>, after_seq: Option<i64>, limit: Option<i64>) -> Self {
        Self {
            before_seq: before_seq.map(Seq),
            after_seq: after_seq.map(Seq),
            limit: limit.unwrap_or(Self::DEFAULT_LIMIT).clamp(1, Self::MAX_LIMIT),
        }
    }

    /// Forward means catching up after being offline; backward means scrolling
    /// into history.
    pub fn is_forward(&self) -> bool {
        self.after_seq.is_some()
    }
}

impl Default for Cursor {
    fn default() -> Self {
        Self::new(None, None, None)
    }
}

/// Local calendar day of a timestamp, used by digest/grouping logic.
pub fn local_day(at: DateTime<impl chrono::TimeZone>) -> (i32, u32, u32) {
    (at.year(), at.month(), at.day())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn phone_accepts_international_formats() {
        assert_eq!(
            PhoneNumber::parse("+234 801 234 5678").unwrap().as_str(),
            "+2348012345678"
        );
        assert_eq!(
            PhoneNumber::parse("+1 (415) 555-0132").unwrap().as_str(),
            "+14155550132"
        );
        assert!(PhoneNumber::parse("+44 20 7946 0958").is_ok());
    }

    #[test]
    fn phone_rejects_invalid() {
        assert!(PhoneNumber::parse("08012345678").is_err()); // no country code
        assert!(PhoneNumber::parse("+0348012345").is_err()); // leading zero
        assert!(PhoneNumber::parse("+12").is_err()); // too short
        assert!(PhoneNumber::parse("+1234567890123456").is_err()); // too long
        assert!(PhoneNumber::parse("+234abc12345").is_err()); // letters
    }

    #[test]
    fn phone_display_is_redacted() {
        let phone = PhoneNumber::parse("+2348012345678").unwrap();
        let rendered = format!("{phone}");
        assert!(!rendered.contains("801234"));
        assert!(rendered.starts_with("+234"));
    }

    #[test]
    fn username_rules() {
        assert!(Username::parse("ada_lovelace").is_ok());
        assert_eq!(Username::parse("AdaLovelace").unwrap().as_str(), "adalovelace");
        assert!(Username::parse("ad").is_err());
        assert!(Username::parse("_ada").is_err());
        assert!(Username::parse("ada..lovelace").is_err());
        assert!(Username::parse("admin").is_err());
        assert!(Username::parse("ada lovelace").is_err());
    }

    #[test]
    fn quiet_hours_same_day_window() {
        let window = QuietHours::new(9 * 60, 17 * 60, "UTC", false).unwrap();
        assert!(window.contains(Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap()));
        assert!(!window.contains(Utc.with_ymd_and_hms(2026, 8, 29, 20, 0, 0).unwrap()));
    }

    #[test]
    fn quiet_hours_crossing_midnight() {
        // 22:00 to 07:00 — the case that gets implemented wrong.
        let window = QuietHours::new(22 * 60, 7 * 60, "Africa/Lagos", true).unwrap();
        assert!(window.contains(Utc.with_ymd_and_hms(2026, 8, 29, 23, 30, 0).unwrap()));
        assert!(window.contains(Utc.with_ymd_and_hms(2026, 8, 29, 3, 0, 0).unwrap()));
        assert!(!window.contains(Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap()));
        // Boundaries: start inclusive, end exclusive.
        assert!(window.contains(Utc.with_ymd_and_hms(2026, 8, 29, 22, 0, 0).unwrap()));
        assert!(!window.contains(Utc.with_ymd_and_hms(2026, 8, 29, 7, 0, 0).unwrap()));
    }

    #[test]
    fn mute_expires() {
        let now = Utc::now();
        let state = MuteState {
            muted_until: Some(now + chrono::Duration::hours(1)),
        };
        assert!(state.is_muted_at(now));
        assert!(!state.is_muted_at(now + chrono::Duration::hours(2)));
        assert!(!MuteState::unmuted().is_muted_at(now));
    }

    #[test]
    fn cursor_clamps_limit() {
        assert_eq!(Cursor::new(None, None, Some(9_999)).limit, Cursor::MAX_LIMIT);
        assert_eq!(Cursor::new(None, None, Some(0)).limit, 1);
        assert_eq!(Cursor::new(None, None, None).limit, Cursor::DEFAULT_LIMIT);
    }

    #[test]
    fn seq_distance_never_negative() {
        assert_eq!(Seq(10).distance_from(Seq(4)), 6);
        assert_eq!(Seq(3).distance_from(Seq(9)), 0);
    }
}
