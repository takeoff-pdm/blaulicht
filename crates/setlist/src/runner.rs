//! Firing a [`Plan`] while the position moves.

use crate::plan::{Action, CueKind, Plan};

/// Moving back more than this, or forward more than [`JUMP_FORWARD`], in one
/// step is a jump (seek, hot cue, loop, re-sync): instead of firing every cue
/// in between, the runner sets the look in effect at the new position.
const JUMP_BACK: f64 = 0.25;
const JUMP_FORWARD: f64 = 1.0;

pub struct Runner {
    plan: Plan,
    /// Index of the next cue to fire.
    next: usize,
    last: Option<f64>,
    /// Last alpha and speed sent per ramp; `None` while outside the ramp.
    ramp_values: Vec<Option<(Option<u8>, Option<u8>)>>,
}

/// What happened in one [`Runner::advance`], for the plugin's log.
#[derive(Debug, PartialEq)]
pub enum Fired {
    Cue(CueKind),
    /// Jumped: the look in effect was set directly.
    CaughtUp,
}

impl Runner {
    pub fn new(plan: Plan) -> Self {
        let ramps = plan.ramps.len();
        Self {
            plan,
            next: 0,
            last: None,
            ramp_values: vec![None; ramps],
        }
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// The next advance catches up instead of firing cues in between.
    pub fn reset(&mut self) {
        self.last = None;
    }

    /// Move to `beat` (lead time already added) and append what to send.
    pub fn advance(&mut self, beat: f64, out: &mut Vec<Action>) -> Vec<Fired> {
        let mut fired = Vec::new();
        let jumped = self
            .last
            .is_none_or(|last| beat < last - JUMP_BACK || beat > last + JUMP_FORWARD);
        if jumped {
            self.next = self.plan.cues.partition_point(|c| c.beat <= beat);
            if let Some(look) = self.plan.look_at(beat) {
                out.push(Action::Look(look.clone()));
                fired.push(Fired::CaughtUp);
            }
        } else {
            while let Some(cue) = self.plan.cues.get(self.next) {
                if cue.beat > beat {
                    break;
                }
                out.extend(cue.actions.iter().cloned());
                fired.push(Fired::Cue(cue.kind));
                self.next += 1;
            }
        }
        // While running backwards a little (deck jitter), cues already fired
        // stay fired; only a jump rewinds `next`.

        for (ramp, sent) in self.plan.ramps.iter().zip(&mut self.ramp_values) {
            if ramp.contains(beat) {
                let (alpha, speed) = ramp.values_at(beat);
                let (old_alpha, old_speed) = sent.unwrap_or((None, None));
                if let Some(percent) = alpha.filter(|a| old_alpha != Some(*a)) {
                    out.push(Action::Alpha {
                        scene: ramp.scene,
                        percent,
                    });
                }
                if let Some(index) = speed.filter(|s| old_speed != Some(*s)) {
                    out.push(Action::Speed {
                        scene: ramp.scene,
                        index,
                    });
                }
                *sent = Some((alpha, speed));
            } else if sent.take().is_some() {
                out.extend(ramp.reset_actions());
            }
        }

        self.last = Some(beat);
        fired
    }

    /// Put every ramped scene back to full alpha and x1 (track change, plugin
    /// switched off).
    pub fn finish(&mut self, out: &mut Vec<Action>) {
        for (ramp, sent) in self.plan.ramps.iter().zip(&mut self.ramp_values) {
            if sent.take().is_some() {
                out.extend(ramp.reset_actions());
            }
        }
        self.last = None;
    }

    /// The next cue that changes something, from `beat` on.
    pub fn upcoming(&self, beat: f64) -> Option<(f64, CueKind)> {
        self.plan
            .cues
            .iter()
            .find(|c| c.beat > beat && !c.actions.is_empty())
            .map(|c| (c.beat, c.kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tests::test_map;
    use crate::map::SectionKind::*;
    use crate::plan::{plan, ShowSettings, SPEED_X1};

    fn runner() -> Runner {
        let mut s = ShowSettings::default();
        s.set_look(Intro, Some(1));
        s.set_look(Breakdown, Some(2));
        s.set_look(Build, Some(3));
        s.set_look(Drop, Some(4));
        s.hit_scene = Some(5);
        // Intro 0..33, Breakdown 33..65, Build 65..97, Drop 97..161.
        let map = test_map(&[(Intro, 8), (Breakdown, 8), (Build, 8), (Drop, 16)]);
        Runner::new(plan(&map, &s))
    }

    /// Steps through `from..to` in 12 ms ticks at 128 BPM.
    fn run(r: &mut Runner, from: f64, to: f64) -> Vec<Action> {
        let step = 0.012 * 128.0 / 60.0;
        let mut out = Vec::new();
        let mut beat = from;
        while beat < to {
            r.advance(beat, &mut out);
            beat += step;
        }
        out
    }

    fn looks(actions: &[Action]) -> Vec<Vec<u8>> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Look(l) => Some(l.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn plays_through_a_track_in_order() {
        let mut r = runner();
        let out = run(&mut r, 0.0, 120.0);
        assert_eq!(
            looks(&out),
            vec![vec![1], vec![2], vec![3], vec![], vec![4, 5], vec![4]]
        );
        let alphas: Vec<u8> = out
            .iter()
            .filter_map(|a| match a {
                Action::Alpha { percent, .. } => Some(*percent),
                _ => None,
            })
            .collect();
        // 30 % to 95 % in 5 % steps, then the reset to 100 % at the blackout.
        assert_eq!(alphas.first(), Some(&30));
        assert_eq!(alphas.len(), 15);
        assert_eq!(alphas.last(), Some(&100));
        assert!(alphas.windows(2).all(|w| w[0] < w[1]));
        let speeds: Vec<u8> = out
            .iter()
            .filter_map(|a| match a {
                Action::Speed { index, .. } => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(speeds, vec![SPEED_X1, SPEED_X1 + 1, SPEED_X1 + 2, SPEED_X1]);
    }

    #[test]
    fn the_blackout_lands_before_the_reset_is_visible() {
        let mut r = runner();
        let mut out = Vec::new();
        run(&mut r, 60.0, 95.9);
        r.advance(96.0, &mut out);
        // Ramp reset and blackout in the same step, so the reset is never seen.
        assert!(out.contains(&Action::Look(vec![])));
        assert!(out.contains(&Action::Alpha { scene: 3, percent: 100 }));
    }

    #[test]
    fn a_jump_catches_up_instead_of_firing_everything() {
        let mut r = runner();
        run(&mut r, 0.0, 10.0);
        let mut out = Vec::new();
        let fired = r.advance(98.0, &mut out);
        assert_eq!(fired, vec![Fired::CaughtUp]);
        assert_eq!(looks(&out), vec![vec![4, 5]]);
        // The hit still ends on time.
        let out = run(&mut r, 98.0, 100.0);
        assert_eq!(looks(&out), vec![vec![4]]);

        // Jump back into the build: its look, and the ramp resumes mid-way.
        let mut out = Vec::new();
        r.advance(81.0, &mut out);
        assert_eq!(looks(&out), vec![vec![3]]);
        assert!(out.contains(&Action::Alpha { scene: 3, percent: 65 }));
        assert!(out.contains(&Action::Speed { scene: 3, index: SPEED_X1 + 1 }));
    }

    #[test]
    fn small_backwards_steps_do_not_refire() {
        let mut r = runner();
        run(&mut r, 30.0, 33.1);
        let mut out = Vec::new();
        r.advance(32.95, &mut out);
        r.advance(33.05, &mut out);
        assert!(looks(&out).is_empty());
    }

    #[test]
    fn finish_resets_a_running_ramp() {
        let mut r = runner();
        run(&mut r, 70.0, 80.0);
        let mut out = Vec::new();
        r.finish(&mut out);
        assert_eq!(
            out,
            vec![
                Action::Alpha { scene: 3, percent: 100 },
                Action::Speed { scene: 3, index: SPEED_X1 }
            ]
        );
        let mut out = Vec::new();
        r.finish(&mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn upcoming_skips_silent_cues() {
        let r = runner();
        assert_eq!(r.upcoming(40.0), Some((65.0, CueKind::Section(Build))));
        assert_eq!(r.upcoming(90.0), Some((96.0, CueKind::Blackout)));
    }
}
