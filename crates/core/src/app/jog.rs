use std::time::{Duration, Instant};

use blaulicht_shared::{ControlEvent, ControlEventMessage, EventOriginator};

use super::BlaulichtApp;

const RECENT_PROPERTY_WINDOW: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(super) struct RecentFixtureProperty {
    last: Option<(ControlEvent, Instant)>,
}

impl RecentFixtureProperty {
    pub(super) fn observe(&mut self, event: &ControlEvent, now: Instant) {
        match event {
            ControlEvent::Transaction(events) => {
                for event in events {
                    self.observe(event, now);
                }
            }
            ControlEvent::SetAlpha(_)
            | ControlEvent::SetStrobeSpeed(_)
            | ControlEvent::SetFocus(_)
            | ControlEvent::SetPan(_)
            | ControlEvent::SetTilt(_)
            | ControlEvent::SetColorHue(_)
            | ControlEvent::SetColorSaturation(_)
            | ControlEvent::SetColorValue(_) => self.last = Some((event.clone(), now)),
            _ => {}
        }
    }

    pub(super) fn adjust(&mut self, delta: i32, now: Instant) -> Option<ControlEvent> {
        let (event, touched) = self.last.as_mut()?;
        if delta == 0 || now.saturating_duration_since(*touched) > RECENT_PROPERTY_WINDOW {
            return None;
        }
        match event {
            ControlEvent::SetAlpha(value)
            | ControlEvent::SetStrobeSpeed(value)
            | ControlEvent::SetFocus(value)
            | ControlEvent::SetPan(value)
            | ControlEvent::SetTilt(value)
            | ControlEvent::SetColorSaturation(value)
            | ControlEvent::SetColorValue(value) => {
                *value = i32::from(*value).saturating_add(delta).clamp(0, 255) as u8;
            }
            ControlEvent::SetColorHue(value) => {
                *value = i32::from(*value).saturating_add(delta).clamp(0, 360) as u16;
            }
            _ => return None,
        }
        *touched = now;
        Some(event.clone())
    }
}

impl BlaulichtApp {
    pub(super) fn finish_jog_adjustment(&mut self, ctx: &egui::Context) {
        let Some(delta) = super::components::take_relative_adjustment(ctx) else {
            return;
        };
        if let Some(event) = self.recent_fixture_property.adjust(delta, Instant::now()) {
            self.data
                .event_bus_connection
                .send(ControlEventMessage::new(EventOriginator::Web, event));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_value_accumulates_and_expires_after_inactivity() {
        let mut recent = RecentFixtureProperty::default();
        let start = Instant::now();
        assert!(recent.adjust(1, start).is_none());
        recent.observe(&ControlEvent::SetAlpha(90), start);
        assert!(matches!(
            recent.adjust(3, start + Duration::from_secs(4)),
            Some(ControlEvent::SetAlpha(93))
        ));
        assert!(matches!(
            recent.adjust(-1, start + Duration::from_secs(8)),
            Some(ControlEvent::SetAlpha(92))
        ));
        assert!(recent
            .adjust(1, start + Duration::from_millis(13001))
            .is_none());
    }

    #[test]
    fn transaction_order_and_unrelated_events() {
        let mut recent = RecentFixtureProperty::default();
        let now = Instant::now();
        recent.observe(
            &ControlEvent::Transaction(vec![
                ControlEvent::SetAlpha(90),
                ControlEvent::Transaction(vec![ControlEvent::SetPan(50)]),
                ControlEvent::SetColor((255, 0, 0)),
            ]),
            now,
        );
        assert!(matches!(
            recent.adjust(1, now),
            Some(ControlEvent::SetPan(51))
        ));
    }

    #[test]
    fn limits_saturate_and_activity_at_limit_renews_window() {
        let mut recent = RecentFixtureProperty::default();
        let now = Instant::now();
        recent.observe(&ControlEvent::SetAlpha(254), now);
        assert!(matches!(
            recent.adjust(i32::MAX, now),
            Some(ControlEvent::SetAlpha(255))
        ));
        assert!(matches!(
            recent.adjust(1, now + Duration::from_secs(5)),
            Some(ControlEvent::SetAlpha(255))
        ));
        assert!(matches!(
            recent.adjust(i32::MIN, now + Duration::from_secs(9)),
            Some(ControlEvent::SetAlpha(0))
        ));
        recent.observe(&ControlEvent::SetColorHue(359), now);
        assert!(matches!(
            recent.adjust(4, now),
            Some(ControlEvent::SetColorHue(360))
        ));
    }
}
