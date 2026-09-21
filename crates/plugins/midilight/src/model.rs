use serde::{Deserialize, Serialize};

pub const STEPS: u8 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Fixture(u8, u8),
    Group(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub start: u8,
    pub end: u8,
}

impl Note {
    pub fn level(self, step: f64, sine: bool, maximum: u8) -> u16 {
        if step < f64::from(self.start) || step >= f64::from(self.end) {
            return 0;
        }
        let envelope = if sine {
            let phase = (step - f64::from(self.start)) / f64::from(self.end - self.start);
            (std::f64::consts::PI * phase).sin()
        } else {
            1.0
        };
        (envelope * f64::from(maximum)).round() as u16
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Row {
    pub target: Option<Target>,
    pub notes: Vec<Note>,
}

impl Row {
    pub fn can_place(&self, note: Note, except: Option<usize>) -> bool {
        note.start < note.end
            && note.end <= STEPS
            && self.notes.iter().enumerate().all(|(i, other)| {
                Some(i) == except || note.end <= other.start || note.start >= other.end
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pattern {
    pub version: u8,
    pub groups: bool,
    pub sine: bool,
    pub maximum: u8,
    pub reverse: bool,
    pub reverse_every: u8,
    pub rows: Vec<Row>,
    pub selection: Vec<(u8, u8)>,
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            version: 1,
            groups: false,
            sine: false,
            maximum: 255,
            reverse: false,
            reverse_every: 1,
            rows: Vec::new(),
            selection: Vec::new(),
        }
    }
}

pub fn canonical(fixtures: &[(u8, u8)]) -> Vec<(u8, u8)> {
    let mut selection = fixtures.to_vec();
    selection.sort_unstable();
    selection.dedup();
    selection
}

impl Pattern {
    pub fn targets(&self) -> Vec<Target> {
        let mut targets = Vec::new();
        for &(group, fixture) in &self.selection {
            let target = if self.groups {
                Target::Group(group)
            } else {
                Target::Fixture(group, fixture)
            };
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        targets
    }

    pub fn new_for(fixtures: &[(u8, u8)]) -> Self {
        let mut pattern = Self {
            selection: canonical(fixtures),
            ..Self::default()
        };
        pattern.rows = pattern
            .targets()
            .into_iter()
            .map(|target| Row {
                target: Some(target),
                notes: Vec::new(),
            })
            .collect();
        pattern
    }

    pub fn invalidate(&mut self) {
        for row in &mut self.rows {
            row.target = None;
        }
        self.drop_unmappable_rows();
    }

    /// With `n` distinct targets there are at most `n` unique targets to go around, so every
    /// row beyond the `n`th makes `valid_mapping` permanently false — and the editor only
    /// deletes rows one at a time, six per page. Shrinking the target set (a smaller
    /// selection, or switching to group targets) would otherwise leave a pattern that outputs
    /// nothing and cannot be repaired by hand. Drop the surplus, taking unmapped rows from the
    /// back so rows the user already mapped are never touched.
    fn drop_unmappable_rows(&mut self) {
        let mut surplus = self.rows.len().saturating_sub(self.targets().len());
        if surplus == 0 {
            return;
        }
        let mut doomed = vec![false; self.rows.len()];
        for (index, row) in self.rows.iter().enumerate().rev() {
            if surplus == 0 {
                break;
            }
            if row.target.is_none() {
                doomed[index] = true;
                surplus -= 1;
            }
        }
        let mut doomed = doomed.into_iter();
        self.rows.retain(|_| !doomed.next().unwrap_or(false));
    }

    pub fn update_selection(&mut self, fixtures: &[(u8, u8)]) -> bool {
        let selection = canonical(fixtures);
        if selection == self.selection {
            return false;
        }
        self.selection = selection;
        self.invalidate();
        true
    }

    pub fn valid_mapping(&self) -> bool {
        let targets = self.targets();
        !self.rows.is_empty()
            && self.rows.iter().enumerate().all(|(i, row)| {
                row.target.is_some_and(|t| {
                    targets.contains(&t)
                        && !self.rows[..i].iter().any(|other| other.target == Some(t))
                })
            })
    }

    pub fn validate_loaded(&mut self) -> bool {
        if self.version != 1
            || self.rows.iter().any(|row| {
                row.notes
                    .iter()
                    .enumerate()
                    .any(|(i, note)| !row.can_place(*note, Some(i)))
            })
        {
            return false;
        }
        self.reverse_every = self.reverse_every.clamp(1, 64);
        self.selection = canonical(&self.selection);
        // Heal patterns saved before the surplus was pruned on the way in.
        self.drop_unmappable_rows();
        true
    }

    pub fn output(&self, fixtures: &[(u8, u8)], step: f64, loops: u64) -> Vec<u16> {
        let mut values = vec![0; fixtures.len()];
        if !self.valid_mapping() {
            return values;
        }
        let reversed = self.reverse && (loops / u64::from(self.reverse_every.max(1))) % 2 == 1;
        for (i, row) in self.rows.iter().enumerate() {
            let target = self.rows[if reversed { self.rows.len() - 1 - i } else { i }].target;
            let level = row
                .notes
                .iter()
                .map(|n| n.level(step, self.sine, self.maximum))
                .max()
                .unwrap_or(0);
            for (index, &(g, f)) in fixtures.iter().enumerate() {
                if target == Some(Target::Fixture(g, f)) || target == Some(Target::Group(g)) {
                    values[index] = level;
                }
            }
        }
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelopes_and_half_open_boundaries() {
        let note = Note { start: 2, end: 6 };
        assert_eq!(note.level(1.99, false, 180), 0);
        assert_eq!(note.level(2.0, false, 180), 180);
        assert_eq!(note.level(6.0, false, 180), 0);
        assert_eq!(note.level(2.0, true, 180), 0);
        assert_eq!(note.level(4.0, true, 180), 180);
        assert_eq!(note.level(5.0, true, 180), note.level(3.0, true, 180));
    }

    #[test]
    fn groups_reversal_and_membership_changes() {
        let fixtures = [(1, 1), (1, 2), (2, 1)];
        let mut p = Pattern::new_for(&fixtures);
        p.groups = true;
        p.rows = vec![
            Row {
                target: Some(Target::Group(1)),
                notes: vec![Note { start: 0, end: 4 }],
            },
            Row {
                target: Some(Target::Group(2)),
                notes: vec![Note { start: 4, end: 8 }],
            },
        ];
        p.reverse = true;
        p.reverse_every = 2;
        assert_eq!(p.output(&fixtures, 1.0, 1), [255, 255, 0]);
        assert_eq!(p.output(&fixtures, 1.0, 2), [0, 0, 255]);
        assert_eq!(p.output(&fixtures, 1.0, 4), [255, 255, 0]);
        assert!(!p.update_selection(&[(2, 1), (1, 2), (1, 1)]));
        assert!(p.update_selection(&[(1, 1), (2, 1)]));
        assert!(!p.valid_mapping());
        assert_eq!(p.rows[0].notes.len(), 1);
        assert_eq!(p.output(&fixtures, 1.0, 0), [0, 0, 0]);
    }

    #[test]
    fn shrinking_the_target_set_leaves_a_repairable_row_count() {
        // The failure this guards: a roll built for many fixtures, then re-pointed at a few.
        let wide: Vec<(u8, u8)> = (0..8).map(|f| (20, f)).collect();
        let mut p = Pattern::new_for(&wide);
        for (i, row) in p.rows.iter_mut().enumerate() {
            row.notes.push(Note {
                start: i as u8,
                end: i as u8 + 1,
            });
        }

        let narrow = [(17, 0), (17, 1), (17, 2)];
        assert!(p.update_selection(&narrow));
        // Surplus rows are gone, so remapping can make the pattern valid again...
        assert_eq!(p.rows.len(), 3);
        // ...and the notes drawn into the surviving rows are kept.
        assert_eq!(p.rows[0].notes, vec![Note { start: 0, end: 1 }]);
        assert!(!p.valid_mapping());
        for (row, target) in p.rows.iter_mut().zip(narrow) {
            row.target = Some(Target::Fixture(target.0, target.1));
        }
        assert!(p.valid_mapping());
        assert_eq!(p.output(&narrow, 0.0, 0), [255, 0, 0]);

        // Switching to group targets shrinks the target set the same way.
        let mut p = Pattern::new_for(&wide);
        p.groups = true;
        p.invalidate();
        assert_eq!(p.rows.len(), 1);

        // Mapped rows are never dropped, even when they sit behind unmapped ones.
        let mut p = Pattern::new_for(&narrow);
        p.rows.push(Row {
            target: Some(Target::Fixture(17, 2)),
            notes: Vec::new(),
        });
        p.rows[0].target = None;
        p.rows[1].target = None;
        p.rows[2].target = None;
        p.drop_unmappable_rows();
        assert_eq!(p.rows.len(), 3);
        assert_eq!(p.rows[2].target, Some(Target::Fixture(17, 2)));
    }

    #[test]
    fn loading_heals_a_pattern_saved_with_unmappable_rows() {
        let mut p = Pattern::new_for(&[(17, 0), (17, 1)]);
        p.rows[0].notes.push(Note { start: 1, end: 2 });
        p.rows[1].notes.push(Note { start: 2, end: 3 });
        // What the broken showfile holds: rows left over from a larger selection.
        for _ in 0..40 {
            p.rows.push(Row {
                target: None,
                notes: Vec::new(),
            });
        }
        assert!(!p.valid_mapping());

        let mut loaded: Pattern =
            serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert!(loaded.validate_loaded());
        assert_eq!(loaded.rows.len(), 2);
        assert!(loaded.valid_mapping());
        assert_eq!(loaded.output(&[(17, 0), (17, 1)], 1.0, 0), [255, 0]);
    }

    #[test]
    fn persistence_overlap_and_independent_patterns() {
        let mut p = Pattern::new_for(&[(2, 4)]);
        p.rows[0].notes.push(Note { start: 0, end: 8 });
        assert!(!p.rows[0].can_place(Note { start: 4, end: 9 }, None));
        assert!(p.rows[0].can_place(Note { start: 8, end: 16 }, None));
        let mut loaded: Pattern =
            serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert!(loaded.validate_loaded());
        loaded.maximum = 90;
        assert_eq!(p.maximum, 255);
        assert!(loaded.valid_mapping());
        loaded.rows[0].notes.push(Note { start: 5, end: 7 });
        assert!(!loaded.validate_loaded());
    }
}
