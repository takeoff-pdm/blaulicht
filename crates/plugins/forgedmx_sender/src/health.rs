//! Reachability check against forgeDMX's HTTP API (`/api/state`, `/api/sync`).

use serde_json::Value;

/// What the last `/api/state` reply said.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RemoteState {
    pub messages: u64,
    pub bpm: Option<f64>,
    pub beat_in_bar: Option<u64>,
    pub last_beat_age: Option<f64>,
    pub osc_listening: Option<bool>,
}

pub fn parse_state(body: &str) -> Result<RemoteState, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("bad /api/state reply: {e}"))?;
    let messages = v
        .get("messages")
        .and_then(Value::as_u64)
        .ok_or("reply has no `messages` (is this forgeDMX?)")?;
    Ok(RemoteState {
        messages,
        bpm: v.get("bpm").and_then(Value::as_f64),
        beat_in_bar: v.get("beat_in_bar").and_then(Value::as_u64),
        last_beat_age: v.get("last_beat_age").and_then(Value::as_f64),
        osc_listening: v.get("osc_listening").and_then(Value::as_bool),
    })
}

/// `/api/sync` returns e.g. `{"source":"Api"}`.
pub fn parse_sync_source(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    v.get("source")?.as_str().map(str::to_string)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Health {
    /// No check finished yet.
    Unknown,
    Unreachable(String),
    /// forgeDMX answers, but its OSC listener is off.
    OscNotListening,
    /// forgeDMX answers, but none of the messages we sent arrived.
    NotArriving,
    /// Our messages arrive.
    Receiving,
    /// Reachable; nothing was sent since the previous check, so arrival is unproven.
    Idle,
}

impl Health {
    pub fn is_error(&self) -> bool {
        matches!(self, Health::Unreachable(_) | Health::OscNotListening | Health::NotArriving)
    }

    pub fn text(&self) -> String {
        match self {
            Health::Unknown => "Checking forgeDMX…".into(),
            Health::Unreachable(e) => format!("forgeDMX not reachable: {e}"),
            Health::OscNotListening => "forgeDMX is reachable, but it is not listening for OSC".into(),
            Health::NotArriving => {
                "forgeDMX is reachable, but our messages don't arrive (check OSC port and firewall for UDP)".into()
            }
            Health::Receiving => "forgeDMX is receiving".into(),
            Health::Idle => "forgeDMX is reachable (nothing sent yet)".into(),
        }
    }

    /// Short cause for a popup, e.g. "Connection refused".
    pub fn short_reason(&self) -> String {
        match self {
            Health::Unreachable(e) => {
                let tail = e.rsplit(": ").next().unwrap_or(e);
                let tail = tail.split(" (os error").next().unwrap_or(tail).trim();
                if tail.is_empty() { e.clone() } else { tail.to_string() }
            }
            Health::OscNotListening => "it is not listening for OSC".into(),
            Health::NotArriving => "our messages don't arrive".into(),
            _ => self.text(),
        }
    }
}

/// Compares two consecutive `/api/state` replies with how many messages we
/// sent in between. UDP gives no acknowledgement, so this counter is the only
/// proof of arrival.
pub fn evaluate(prev: Option<&RemoteState>, now: &RemoteState, sent_between: u64) -> Health {
    if now.osc_listening == Some(false) {
        return Health::OscNotListening;
    }
    let Some(prev) = prev else {
        return Health::Unknown;
    };
    if now.messages > prev.messages {
        Health::Receiving
    } else if sent_between == 0 {
        Health::Idle
    } else if now.messages < prev.messages {
        // forgeDMX restarted between checks; judge on the next round.
        Health::Unknown
    } else {
        Health::NotArriving
    }
}

/// Consecutive failed checks before an outage counts (debounce).
const FAILS_BEFORE_ALERT: u32 = 2;

/// Decides when "forgeDMX became unavailable" deserves a popup: only after it
/// worked, only once per outage, and only after the failure persisted.
#[derive(Debug, Default)]
pub struct OutageDetector {
    worked: bool,
    fails: u32,
    alerted: bool,
}

impl OutageDetector {
    /// Feed each finished check; returns true when an alert should be shown.
    pub fn observe(&mut self, health: &Health) -> bool {
        match health {
            Health::Receiving | Health::Idle => {
                self.worked = true;
                self.fails = 0;
                self.alerted = false;
                false
            }
            Health::Unknown => false,
            _ => {
                self.fails += 1;
                let fire = self.worked && !self.alerted && self.fails >= FAILS_BEFORE_ALERT;
                if fire {
                    self.alerted = true;
                }
                fire
            }
        }
    }
}

/// At most this many outage popups per window.
pub const ALERTS_PER_WINDOW: usize = 5;
pub const ALERT_WINDOW_MS: u32 = 60_000;

/// Sliding-window limit: at most `max` alerts per `window_ms`.
#[derive(Debug)]
pub struct RateLimiter {
    max: usize,
    window_ms: u32,
    sent: Vec<u32>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(ALERTS_PER_WINDOW, ALERT_WINDOW_MS)
    }
}

impl RateLimiter {
    pub fn new(max: usize, window_ms: u32) -> Self {
        Self { max, window_ms, sent: Vec::new() }
    }

    /// True (and counted) when another alert fits in the window at `now`.
    pub fn allow(&mut self, now: u32) -> bool {
        let window = self.window_ms;
        self.sent.retain(|&t| now.wrapping_sub(t) < window);
        if self.sent.len() >= self.max {
            return false;
        }
        self.sent.push(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down() -> Health {
        Health::Unreachable("connection refused".into())
    }

    #[test]
    fn short_reason_strips_address_and_os_error() {
        let h = Health::Unreachable(
            "connect to 127.0.0.1:3000: Connection refused (os error 111)".into(),
        );
        assert_eq!(h.short_reason(), "Connection refused");
        assert_eq!(Health::Unreachable("timed out".into()).short_reason(), "timed out");
    }

    #[test]
    fn no_alert_if_it_never_worked() {
        let mut d = OutageDetector::default();
        assert!(!(0..5).any(|_| d.observe(&down())));
    }

    #[test]
    fn alerts_once_after_debounce_when_it_worked() {
        let mut d = OutageDetector::default();
        assert!(!d.observe(&Health::Receiving));
        assert!(!d.observe(&down()), "first failure is debounced");
        assert!(d.observe(&Health::NotArriving));
        assert!(!d.observe(&down()), "only once per outage");
        assert!(!d.observe(&down()));
    }

    #[test]
    fn single_blip_does_not_alert_and_recovery_rearms() {
        let mut d = OutageDetector::default();
        d.observe(&Health::Receiving);
        assert!(!d.observe(&down()));
        assert!(!d.observe(&Health::Receiving));
        assert!(!d.observe(&down()));
        assert!(d.observe(&down()));
        d.observe(&Health::Idle);
        d.observe(&down());
        assert!(d.observe(&down()), "re-armed after recovery");
    }

    #[test]
    fn unknown_neither_counts_nor_resets() {
        let mut d = OutageDetector::default();
        d.observe(&Health::Receiving);
        d.observe(&down());
        assert!(!d.observe(&Health::Unknown));
        assert!(d.observe(&down()));
    }

    #[test]
    fn rate_limits_to_five_per_minute() {
        let mut r = RateLimiter::new(5, 60_000);
        assert!((0..5).all(|i| r.allow(i * 1_000)));
        assert!(!r.allow(10_000));
        assert!(!r.allow(59_999));
        assert!(r.allow(60_000), "first one left the window");
        assert!(!r.allow(60_500));
    }

    #[test]
    fn rate_limiter_survives_clock_wrap() {
        let mut r = RateLimiter::new(1, 60_000);
        assert!(r.allow(u32::MAX - 10));
        assert!(!r.allow(5));
        assert!(r.allow(60_000));
    }

    const STATE: &str = r#"{"bpm":128.0,"beat_in_bar":3,"last_beat_age":0.21,"messages":42,"osc_listening":true}"#;

    fn st(messages: u64) -> RemoteState {
        RemoteState { messages, osc_listening: Some(true), ..Default::default() }
    }

    #[test]
    fn parses_state() {
        let s = parse_state(STATE).unwrap();
        assert_eq!(s.messages, 42);
        assert_eq!(s.bpm, Some(128.0));
        assert_eq!(s.beat_in_bar, Some(3));
        assert_eq!(s.osc_listening, Some(true));
    }

    #[test]
    fn null_bpm_is_none() {
        let s = parse_state(r#"{"bpm":null,"messages":0}"#).unwrap();
        assert_eq!(s.bpm, None);
    }

    #[test]
    fn rejects_non_forgedmx_reply() {
        assert!(parse_state("{}").is_err());
        assert!(parse_state("<html>").is_err());
    }

    #[test]
    fn parses_sync_source() {
        assert_eq!(parse_sync_source(r#"{"source":"Api"}"#).as_deref(), Some("Api"));
        assert_eq!(parse_sync_source("nope"), None);
    }

    #[test]
    fn evaluates_arrival() {
        assert_eq!(evaluate(None, &st(5), 3), Health::Unknown);
        assert_eq!(evaluate(Some(&st(5)), &st(9), 4), Health::Receiving);
        assert_eq!(evaluate(Some(&st(5)), &st(5), 4), Health::NotArriving);
        assert_eq!(evaluate(Some(&st(5)), &st(5), 0), Health::Idle);
        assert_eq!(evaluate(Some(&st(50)), &st(2), 4), Health::Unknown);
        let mut off = st(9);
        off.osc_listening = Some(false);
        assert_eq!(evaluate(Some(&st(5)), &off, 4), Health::OscNotListening);
    }
}
