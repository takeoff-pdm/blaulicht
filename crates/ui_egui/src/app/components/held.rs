//! [`Held`]: a delta-based knob's value while the operator turns it.

use std::time::{Duration, Instant};

/// How long a released knob keeps showing its value when the engine does not
/// report it back (refused, or another operator changed it meanwhile).
const HOLD_TIMEOUT: Duration = Duration::from_millis(500);

/// A knob's value for one key (a scene, or a view's scene, ...) while the
/// operator turns it, until the engine reports it back. The engine applies a
/// sent value on its next tick, so drawing the engine's value would jump back
/// for a frame, and a knob that adds drag deltas to the drawn value
/// (`Knob`, `SpeedKnob`) would lose movement.
pub struct Held<T, K = u8>(Option<(K, T, Instant)>);

impl<T, K> Default for Held<T, K> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: Copy, K: Copy + PartialEq> Held<T, K> {
    /// The value to draw for `key`: the held one, or the engine's.
    pub fn shown(&self, key: K, engine: T) -> T {
        match self.0 {
            Some((held_key, value, _)) if held_key == key => value,
            _ => engine,
        }
    }

    /// The operator set `value` on `key` (and it was sent).
    pub fn hold(&mut self, key: K, value: T) {
        self.0 = Some((key, value, Instant::now()));
    }

    /// For a single knob whose key follows a focus: lets go once the knob is
    /// no longer dragged and the engine shows the held value (`applied`),
    /// when the focus moved to another key, or [`HOLD_TIMEOUT`] after the
    /// last change.
    pub fn settle(&mut self, dragging: bool, key: K, applied: impl FnOnce(T) -> bool) {
        let Some((held_key, value, since)) = self.0 else {
            return;
        };
        if held_key != key || (!dragging && (applied(value) || since.elapsed() >= HOLD_TIMEOUT)) {
            self.0 = None;
        }
    }

    /// For one knob of a list sharing this `Held` (one row per key): as
    /// [`Self::settle`], but other rows' keys leave the held value alone.
    pub fn settle_row(&mut self, dragging: bool, key: K, applied: impl FnOnce(T) -> bool) {
        if matches!(self.0, Some((held_key, _, _)) if held_key == key) {
            self.settle(dragging, key, applied);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Held;

    #[test]
    fn a_turned_knob_shows_its_value_until_the_engine_has_it() {
        let mut held = Held::<f32>::default();
        assert_eq!(held.shown(0, 10.0), 10.0);

        held.hold(0, 42.0);
        // Still dragging: kept even once the engine caught up.
        held.settle(true, 0, |value| value as u8 == 42);
        assert_eq!(held.shown(0, 42.0), 42.0);
        // Another scene draws its own value.
        assert_eq!(held.shown(1, 7.0), 7.0);

        // Released, but the engine still has the old value: kept.
        held.settle(false, 0, |value| value as u8 == 10);
        assert_eq!(held.shown(0, 10.0), 42.0);
        // The engine reports it back: let go.
        held.settle(false, 0, |value| value as u8 == 42);
        assert_eq!(held.shown(0, 10.0), 10.0);

        // A focus change lets go too.
        held.hold(0, 42.0);
        held.settle(true, 1, |_| false);
        assert_eq!(held.shown(0, 10.0), 10.0);
    }

    #[test]
    fn a_row_settles_only_its_own_held_value() {
        let mut held = Held::<f32, (u8, u8)>::default();
        held.hold((0, 1), 42.0);
        // Another row of the list leaves it alone.
        held.settle_row(false, (0, 2), |_| true);
        assert_eq!(held.shown((0, 1), 10.0), 42.0);
        assert_eq!(held.shown((0, 2), 7.0), 7.0);
        // Its own row lets go once the engine has it.
        held.settle_row(false, (0, 1), |value| value as u8 == 42);
        assert_eq!(held.shown((0, 1), 10.0), 10.0);
    }
}
