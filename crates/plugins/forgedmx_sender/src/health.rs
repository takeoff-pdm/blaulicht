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

#[cfg(test)]
mod tests {
    use super::*;

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
