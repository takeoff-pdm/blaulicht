//! Turning a track map into cues: the light design part.
//!
//! Every section switches to its look. Around each drop:
//!
//! ```text
//!   Build ............ ramp the build scene's alpha and speed up
//!   last beat(s) ..... blackout (or a dark "pre-drop" scene that moves the
//!                      movers into their drop positions while nobody sees it)
//!   downbeat ......... drop look, plus a hit scene for the first beats
//! ```

use serde::{Deserialize, Serialize};

use crate::map::{SectionKind, TrackMap};

/// Index of the x1 speed in the engine's `AnimationSpeedModifier::ALL`.
pub const SPEED_X1: u8 = 4;
/// x2 and x4: how far a build speeds up.
const BUILD_SPEEDS: [u8; 3] = [SPEED_X1, SPEED_X1 + 1, SPEED_X1 + 2];
/// The build alpha moves in steps this large, so a ramp sends about 20
/// events instead of one per tick.
const ALPHA_STEP: u8 = 5;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ShowSettings {
    /// Scene per [`SectionKind`] (by index); `None` keeps the previous look.
    pub looks: [Option<u8>; SectionKind::COUNT],
    /// Half beats of darkness before each drop; 0 turns it off.
    pub blackout_half_beats: u8,
    /// Shown instead of nothing during the blackout. Meant for a scene with
    /// dimmers at zero that aims the movers at their drop positions.
    pub blackout_scene: Option<u8>,
    /// Added on top of the drop look for the first `hit_beats` of a drop.
    pub hit_scene: Option<u8>,
    pub hit_beats: u8,
    /// Fade the build scene's master alpha in over the build.
    pub ramp_alpha: bool,
    /// Start of that fade, in percent.
    pub ramp_from_percent: u8,
    /// Speed the build scene's animations up from x1 to x4.
    pub ramp_speed: bool,
    /// Fire every cue this early, to make up for DMX and fixture latency.
    pub lead_ms: u16,
}

impl Default for ShowSettings {
    fn default() -> Self {
        Self {
            looks: [None; SectionKind::COUNT],
            blackout_half_beats: 2,
            blackout_scene: None,
            hit_scene: None,
            hit_beats: 2,
            ramp_alpha: true,
            ramp_from_percent: 30,
            ramp_speed: true,
            lead_ms: 30,
        }
    }
}

impl ShowSettings {
    pub fn look(&self, kind: SectionKind) -> Option<u8> {
        self.looks[kind.index()]
    }

    pub fn set_look(&mut self, kind: SectionKind, scene: Option<u8>) {
        self.looks[kind.index()] = scene;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Replace the overlay stack (the render base is the engine's BLANK
    /// scene, so an empty stack is a blackout).
    Look(Vec<u8>),
    /// Scene master alpha in percent.
    Alpha { scene: u8, percent: u8 },
    /// Scene master speed as an index into `AnimationSpeedModifier::ALL`.
    Speed { scene: u8, index: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueKind {
    Section(SectionKind),
    Blackout,
    Hit,
    HitEnd,
}

impl CueKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Section(kind) => kind.name(),
            Self::Blackout => "Blackout",
            Self::Hit => "Drop hit",
            Self::HitEnd => "Hit end",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    /// Grid beat the cue lands on.
    pub beat: f64,
    pub kind: CueKind,
    pub actions: Vec<Action>,
    /// The look in effect from this cue on, for catching up after a jump.
    pub look: Option<Vec<u8>>,
}

/// A build scene fading and speeding up from `start` to `end` (grid beats).
#[derive(Clone, Debug, PartialEq)]
pub struct Ramp {
    pub scene: u8,
    pub start: f64,
    pub end: f64,
    /// Alpha percent at the start, or `None` to leave alpha alone.
    pub alpha_from: Option<u8>,
    pub speed: bool,
}

impl Ramp {
    pub fn contains(&self, beat: f64) -> bool {
        self.start <= beat && beat < self.end
    }

    /// Alpha percent and speed index at `beat`, quantized.
    pub fn values_at(&self, beat: f64) -> (Option<u8>, Option<u8>) {
        let t = ((beat - self.start) / (self.end - self.start)).clamp(0.0, 1.0);
        let alpha = self.alpha_from.map(|from| {
            let value = from as f64 + (100.0 - from as f64) * t;
            ((value / ALPHA_STEP as f64).floor() as u8 * ALPHA_STEP).clamp(from, 100)
        });
        // x1 for the first half, x2 for the third quarter, x4 for the last.
        let speed = self.speed.then(|| {
            let step = if t < 0.5 {
                0
            } else if t < 0.75 {
                1
            } else {
                2
            };
            BUILD_SPEEDS[step]
        });
        (alpha, speed)
    }

    /// What the scene goes back to when the ramp is over.
    pub fn reset_actions(&self) -> Vec<Action> {
        let mut out = Vec::new();
        if self.alpha_from.is_some() {
            out.push(Action::Alpha {
                scene: self.scene,
                percent: 100,
            });
        }
        if self.speed {
            out.push(Action::Speed {
                scene: self.scene,
                index: SPEED_X1,
            });
        }
        out
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Sorted by beat.
    pub cues: Vec<Cue>,
    pub ramps: Vec<Ramp>,
}

impl Plan {
    /// The look in effect at `beat`: that of the last cue at or before it.
    pub fn look_at(&self, beat: f64) -> Option<&Vec<u8>> {
        self.cues
            .iter()
            .take_while(|c| c.beat <= beat)
            .filter_map(|c| c.look.as_ref())
            .last()
    }
}

pub fn plan(map: &TrackMap, settings: &ShowSettings) -> Plan {
    let mut cues: Vec<Cue> = Vec::new();
    let mut ramps = Vec::new();
    // The look built so far, so a section without a scene keeps it.
    let mut current: Option<Vec<u8>> = None;
    let blackout_beats = settings.blackout_half_beats as f64 / 2.0;

    for (i, section) in map.sections.iter().enumerate() {
        let start = section.start as f64;
        let next = map.sections.get(i + 1);
        let scene = settings.look(section.kind);

        if section.kind == SectionKind::Drop {
            let look = scene.map(|s| vec![s]).or_else(|| current.clone());
            let prev_start = i
                .checked_sub(1)
                .map_or(0.0, |p| map.sections[p].start as f64);
            if blackout_beats > 0.0 && start > 0.0 {
                let dark = settings.blackout_scene.map(|s| vec![s]).unwrap_or_default();
                let at = (start - blackout_beats).max(prev_start);
                push(&mut cues, at, CueKind::Blackout, Some(dark));
            }
            let hit_end = start + settings.hit_beats as f64;
            match (settings.hit_scene, look.clone()) {
                (Some(hit), look) if settings.hit_beats > 0 => {
                    let mut with_hit = look.clone().unwrap_or_default();
                    with_hit.retain(|s| *s != hit);
                    with_hit.push(hit);
                    push(&mut cues, start, CueKind::Hit, Some(with_hit));
                    if (hit_end as u32) < section.end {
                        push(&mut cues, hit_end, CueKind::HitEnd, Some(look.unwrap_or_default()));
                    }
                }
                (_, look) => push(&mut cues, start, CueKind::Section(SectionKind::Drop), look),
            }
            current = look;
            continue;
        }

        if let Some(scene) = scene {
            current = Some(vec![scene]);
        }
        push(
            &mut cues,
            start,
            CueKind::Section(section.kind),
            scene.map(|s| vec![s]),
        );

        let before_drop = next.is_some_and(|n| n.kind == SectionKind::Drop);
        let ramps_on = settings.ramp_alpha || settings.ramp_speed;
        if section.kind == SectionKind::Build && before_drop && ramps_on {
            if let (Some(scene), Some(next)) = (scene, next) {
                let end = (next.start as f64 - blackout_beats).max(start + 1.0);
                ramps.push(Ramp {
                    scene,
                    start,
                    end,
                    alpha_from: settings.ramp_alpha.then_some(settings.ramp_from_percent.min(100)),
                    speed: settings.ramp_speed,
                });
            }
        }
    }

    // Stable, so a blackout clamped onto a section start stays after it.
    cues.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    dedupe_looks(&mut cues);
    Plan { cues, ramps }
}

fn push(cues: &mut Vec<Cue>, beat: f64, kind: CueKind, look: Option<Vec<u8>>) {
    cues.push(Cue {
        beat,
        kind,
        actions: look.iter().map(|l| Action::Look(l.clone())).collect(),
        look,
    });
}

/// Drop look actions that repeat the look already in effect.
fn dedupe_looks(cues: &mut [Cue]) {
    let mut current: Option<Vec<u8>> = None;
    for cue in cues.iter_mut() {
        let Some(look) = &cue.look else { continue };
        if current.as_ref() == Some(look) {
            cue.actions.retain(|a| !matches!(a, Action::Look(_)));
        }
        current = Some(look.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tests::test_map;
    use SectionKind::*;

    const INTRO: u8 = 1;
    const BREAK: u8 = 2;
    const BUILD: u8 = 3;
    const DROP: u8 = 4;
    const HIT: u8 = 5;
    const DARK: u8 = 6;

    fn settings() -> ShowSettings {
        let mut s = ShowSettings::default();
        s.set_look(Intro, Some(INTRO));
        s.set_look(Breakdown, Some(BREAK));
        s.set_look(Build, Some(BUILD));
        s.set_look(Drop, Some(DROP));
        s.hit_scene = Some(HIT);
        s
    }

    fn track() -> TrackMap {
        // Downbeat on beat 1: Intro 0..33, Breakdown 33..65, Build 65..97,
        // Drop 97..161.
        test_map(&[(Intro, 8), (Breakdown, 8), (Build, 8), (Drop, 16)])
    }

    fn looks(plan: &Plan) -> Vec<(f64, CueKind, Vec<u8>)> {
        plan.cues
            .iter()
            .filter_map(|c| {
                c.actions.iter().find_map(|a| match a {
                    Action::Look(l) => Some((c.beat, c.kind, l.clone())),
                    _ => None,
                })
            })
            .collect()
    }

    #[test]
    fn drop_gets_blackout_hit_and_ramp() {
        let plan = plan(&track(), &settings());
        assert_eq!(
            looks(&plan),
            vec![
                (0.0, CueKind::Section(Intro), vec![INTRO]),
                (33.0, CueKind::Section(Breakdown), vec![BREAK]),
                (65.0, CueKind::Section(Build), vec![BUILD]),
                (96.0, CueKind::Blackout, vec![]),
                (97.0, CueKind::Hit, vec![DROP, HIT]),
                (99.0, CueKind::HitEnd, vec![DROP]),
            ]
        );
        assert_eq!(
            plan.ramps,
            vec![Ramp {
                scene: BUILD,
                start: 65.0,
                end: 96.0,
                alpha_from: Some(30),
                speed: true
            }]
        );
        assert_eq!(
            plan.ramps[0].reset_actions(),
            vec![
                Action::Alpha { scene: BUILD, percent: 100 },
                Action::Speed { scene: BUILD, index: SPEED_X1 }
            ]
        );
    }

    #[test]
    fn blackout_scene_replaces_darkness_and_can_be_turned_off() {
        let mut s = settings();
        s.blackout_scene = Some(DARK);
        s.blackout_half_beats = 1;
        let p = plan(&track(), &s);
        assert!(looks(&p).contains(&(96.5, CueKind::Blackout, vec![DARK])));

        s.blackout_half_beats = 0;
        let p = plan(&track(), &s);
        assert!(!looks(&p).iter().any(|(_, k, _)| *k == CueKind::Blackout));
        // Without a blackout the ramp runs right up to the drop.
        assert_eq!(p.ramps[0].end, 97.0);
    }

    #[test]
    fn sections_without_a_scene_keep_the_look() {
        let mut s = settings();
        s.set_look(Breakdown, None);
        s.set_look(Drop, None);
        s.hit_scene = None;
        let p = plan(&track(), &s);
        let l = looks(&p);
        assert!(!l.iter().any(|(b, _, _)| *b == 33.0));
        // The drop comes back to the build look after the blackout.
        assert!(l.contains(&(97.0, CueKind::Section(Drop), vec![BUILD])));
        // The build still has its scene, so it ramps.
        assert_eq!(p.ramps.len(), 1);
    }

    #[test]
    fn repeated_looks_are_not_resent() {
        let mut s = settings();
        s.set_look(Build, Some(BREAK));
        s.ramp_alpha = false;
        s.ramp_speed = false;
        let p = plan(&track(), &s);
        let build = p.cues.iter().find(|c| c.kind == CueKind::Section(Build)).unwrap();
        assert!(build.actions.is_empty());
        assert_eq!(build.look, Some(vec![BREAK]));
        assert!(p.ramps.is_empty());
    }

    #[test]
    fn look_at_catches_up_after_a_jump() {
        let p = plan(&track(), &settings());
        assert_eq!(p.look_at(-1.0), None);
        assert_eq!(p.look_at(40.0), Some(&vec![BREAK]));
        assert_eq!(p.look_at(96.2), Some(&vec![]));
        assert_eq!(p.look_at(98.0), Some(&vec![DROP, HIT]));
        assert_eq!(p.look_at(120.0), Some(&vec![DROP]));
    }

    #[test]
    fn ramp_values_step_up() {
        let r = Ramp {
            scene: BUILD,
            start: 0.0,
            end: 32.0,
            alpha_from: Some(30),
            speed: true,
        };
        assert_eq!(r.values_at(0.0), (Some(30), Some(SPEED_X1)));
        assert_eq!(r.values_at(16.0), (Some(65), Some(SPEED_X1 + 1)));
        assert_eq!(r.values_at(31.9), (Some(95), Some(SPEED_X1 + 2)));
        assert_eq!(r.values_at(40.0), (Some(100), Some(SPEED_X1 + 2)));
    }
}
