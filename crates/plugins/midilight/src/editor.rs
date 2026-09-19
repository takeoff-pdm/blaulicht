use blaulicht_plugin_framework as bpf;
use blaulicht_shared::PluginUiEvent;

use crate::{clock::BeatClock, model::{Note, Pattern, Row, Target}};

const CANVAS: u8 = 30;
const LEFT: i32 = 160;
const CELL: i32 = 32;
const TOP: i32 = 28;
const HEIGHT: i32 = 26;
const PAGE_ROWS: usize = 6;

#[derive(Clone, Copy)]
enum DragKind { Draw, Move, Left, Right }
#[derive(Clone, Copy)]
struct Drag {
    row: usize,
    note: usize,
    origin_step: i32,
    original: Note,
    kind: DragKind,
}

#[derive(Default)]
pub struct Editor {
    pub mapping_error: bool,
    row: usize,
    note: Option<usize>,
    page: usize,
    drag: Option<Drag>,
}

impl Editor {
    pub fn cancel_gesture(&mut self) { self.drag = None; self.note = None; }

    fn hit_row(&self, y: i32, pattern: &Pattern) -> Option<usize> {
        if !(TOP..TOP + HEIGHT * PAGE_ROWS as i32).contains(&y) { return None; }
        let row = self.page * PAGE_ROWS + ((y - TOP) / HEIGHT) as usize;
        (row < pattern.rows.len()).then_some(row)
    }

    fn hit_note(row: &Row, x: i32) -> Option<usize> {
        row.notes.iter().position(|n| x >= LEFT + i32::from(n.start) * CELL && x < LEFT + i32::from(n.end) * CELL)
    }

    fn step(x: i32) -> i32 { (x - LEFT).div_euclid(CELL).clamp(0, 15) }
    fn boundary(x: i32) -> i32 { ((x - LEFT + CELL / 2).div_euclid(CELL)).clamp(0, 16) }

    fn assign(&mut self, pattern: &mut Pattern, forward: bool) {
        let targets = pattern.targets();
        let Some(row) = pattern.rows.get(self.row) else { return; };
        let available: Vec<_> = targets.into_iter().filter(|target| !pattern.rows.iter().enumerate()
            .any(|(i, r)| i != self.row && r.target == Some(*target))).collect();
        if available.is_empty() { return; }
        let current = row.target.and_then(|t| available.iter().position(|candidate| *candidate == t));
        let next = match current {
            None => if forward { 0 } else { available.len() - 1 },
            Some(i) => if forward { (i + 1) % available.len() } else { (i + available.len() - 1) % available.len() },
        };
        pattern.rows[self.row].target = Some(available[next]);
        if pattern.valid_mapping() { self.mapping_error = false; }
    }

    pub fn event(&mut self, pattern: &mut Pattern, event: PluginUiEvent) -> bool {
        match event {
            PluginUiEvent::Switch { id: 1, value } => {
                if pattern.groups == value { return false; }
                pattern.groups = value;
                pattern.invalidate();
                self.mapping_error = true;
                self.cancel_gesture();
            }
            PluginUiEvent::Switch { id: 2, value } => pattern.sine = value,
            PluginUiEvent::Slider { id: 3, value } => pattern.maximum = value,
            PluginUiEvent::Checkbox { id: 4, checked } => pattern.reverse = checked,
            PluginUiEvent::Slider { id: 5, value } => pattern.reverse_every = value.clamp(1, 64),
            PluginUiEvent::Button { id: 6 } => {
                if let Some(row) = pattern.rows.get_mut(self.row) {
                    if let Some(note) = self.note.take().filter(|i| *i < row.notes.len()) { row.notes.remove(note); }
                }
                self.drag = None;
            }
            PluginUiEvent::Button { id: 7 } => {
                for row in &mut pattern.rows { row.notes.clear(); }
                self.cancel_gesture();
            }
            PluginUiEvent::Button { id: 8 } => {
                pattern.rows.push(Row { target: None, notes: Vec::new() });
                self.row = pattern.rows.len() - 1;
                self.page = self.row / PAGE_ROWS;
                self.cancel_gesture();
            }
            PluginUiEvent::Button { id: 9 } => {
                if self.row < pattern.rows.len() { pattern.rows.remove(self.row); }
                self.row = self.row.min(pattern.rows.len().saturating_sub(1));
                self.page = self.row / PAGE_ROWS;
                self.cancel_gesture();
            }
            PluginUiEvent::Button { id: 10 } => self.assign(pattern, false),
            PluginUiEvent::Button { id: 11 } => self.assign(pattern, true),
            PluginUiEvent::Button { id: 12 } => {
                self.page = self.page.saturating_sub(1); self.cancel_gesture(); return false;
            }
            PluginUiEvent::Button { id: 13 } => {
                self.page = (self.page + 1).min(pattern.rows.len().saturating_sub(1) / PAGE_ROWS);
                self.cancel_gesture(); return false;
            }
            PluginUiEvent::Button { id: 14 } => {
                if let Some(row) = pattern.rows.get_mut(self.row) { row.target = None; }
            }
            PluginUiEvent::CanvasClick { id: CANVAS, x, y } => {
                let Some(row) = self.hit_row(y, pattern) else { return false; };
                self.row = row;
                self.note = None;
                if !(LEFT..LEFT + 16 * CELL).contains(&x) { return false; }
                self.note = Self::hit_note(&pattern.rows[row], x);
                if self.note.is_some() { return false; }
                let start = Self::step(x) as u8;
                pattern.rows[row].notes.push(Note { start, end: start + 1 });
                self.note = Some(pattern.rows[row].notes.len() - 1);
            }
            PluginUiEvent::CanvasDragStart { id: CANVAS, x, y } => {
                self.cancel_gesture();
                let Some(row) = self.hit_row(y, pattern) else { return false; };
                if !(LEFT..LEFT + 16 * CELL).contains(&x) { return false; }
                self.row = row;
                let origin_step = Self::step(x);
                let (note, kind) = if let Some(i) = Self::hit_note(&pattern.rows[row], x) {
                    let note = pattern.rows[row].notes[i];
                    let kind = if x - (LEFT + i32::from(note.start) * CELL) <= 7 { DragKind::Left }
                        else if LEFT + i32::from(note.end) * CELL - x <= 7 { DragKind::Right }
                        else { DragKind::Move };
                    (i, kind)
                } else {
                    pattern.rows[row].notes.push(Note { start: origin_step as u8, end: origin_step as u8 + 1 });
                    (pattern.rows[row].notes.len() - 1, DragKind::Draw)
                };
                self.note = Some(note);
                self.drag = Some(Drag { row, note, origin_step, original: pattern.rows[row].notes[note], kind });
                return false;
            }
            PluginUiEvent::CanvasDrag { id: CANVAS, x, .. } => {
                self.apply_drag(pattern, x); return false;
            }
            PluginUiEvent::CanvasDragEnd { id: CANVAS, x, .. } => {
                self.apply_drag(pattern, x); self.drag = None;
            }
            _ => return false,
        }
        true
    }

    fn apply_drag(&mut self, pattern: &mut Pattern, x: i32) {
        let Some(drag) = self.drag else { return; };
        let Some(row) = pattern.rows.get_mut(drag.row) else { return; };
        if drag.note >= row.notes.len() { return; }
        let step = Self::step(x);
        let mut note = drag.original;
        match drag.kind {
            DragKind::Draw => {
                note.start = drag.origin_step.min(step) as u8;
                note.end = (drag.origin_step.max(step) + 1) as u8;
            }
            DragKind::Move => {
                let length = note.end - note.start;
                note.start = (i32::from(note.start) + step - drag.origin_step).clamp(0, 16 - i32::from(length)) as u8;
                note.end = note.start + length;
            }
            DragKind::Left => note.start = Self::boundary(x).min(i32::from(note.end) - 1) as u8,
            DragKind::Right => note.end = Self::boundary(x).max(i32::from(note.start) + 1) as u8,
        }
        if row.can_place(note, Some(drag.note)) { row.notes[drag.note] = note; }
    }

    fn target_name(target: Option<Target>) -> String {
        bpf::with_dmx(|state| match target {
            None => "Unassigned".into(),
            Some(Target::Group(g)) => state.groups.get(&g).map(|group| format!("G{g}: {}", group.name)).unwrap_or_else(|| format!("Group {g}")),
            Some(Target::Fixture(g, f)) => state.groups.get(&g).and_then(|group| group.fixtures.get(&f))
                .map(|fixture| format!("{g}/{f}: {}", fixture.name)).unwrap_or_else(|| format!("Fixture {g}/{f}")),
        })
    }

    pub fn render(&mut self, pattern: &Pattern, clock: &BeatClock, paused: bool, bpm: f32) {
        use bpf::ui;
        self.page = self.page.min(pattern.rows.len().saturating_sub(1) / PAGE_ROWS);
        ui::begin();
        ui::label_styled("MIDILIGHT  |  16 sixteenths", 18, true);
        ui::begin_horizontal();
        ui::switch("Groups (off: Fixtures)", 1, pattern.groups);
        ui::switch("Sine (off: Gate)", 2, pattern.sine);
        ui::end_horizontal();
        ui::slider("Maximum brightness", 3, 0, 255, pattern.maximum);
        ui::begin_horizontal();
        ui::checkbox("Reverse rows", 4, pattern.reverse);
        ui::slider("Every N loops", 5, 1, 64, pattern.reverse_every);
        ui::end_horizontal();
        let status = if !pattern.valid_mapping() { "ERROR: remap each row or delete unused rows" }
            else if paused { "Paused" }
            else if !clock.running { "Waiting for tempo / fresh beat" }
            else if clock.estimated { "Estimated bar alignment" }
            else { "Following source bar" };
        ui::label(&format!("{status} | {bpm:.1} BPM | loop {}", clock.loops + 1));
        ui::painter_begin(CANVAS, 688, TOP + HEIGHT * PAGE_ROWS as i32 + 4);
        ui::painter_rect(0, 0, 688, 188, 18, 22, 31, 255);
        for step in 0..16 {
            let x = LEFT + step * CELL;
            if step % 4 == 0 {
                ui::painter_rect(x, TOP, CELL * 4, HEIGHT * PAGE_ROWS as i32, if step % 8 == 0 { 32 } else { 25 }, 32, 44, 255);
            }
            ui::painter_text(x + 9, 5, 12, 185, 196, 215, 255, &format!("{}", step + 1));
            ui::painter_line(x, TOP, x, 184, 65, 73, 88, 255, 1);
        }
        for (local, (row_index, row)) in pattern.rows.iter().enumerate().skip(self.page * PAGE_ROWS).take(PAGE_ROWS).enumerate() {
            let y = TOP + local as i32 * HEIGHT;
            if row_index == self.row { ui::painter_rect(0, y, LEFT - 2, HEIGHT, 43, 61, 82, 255); }
            let label: String = Self::target_name(row.target).chars().take(21).collect();
            ui::painter_text(5, y + 6, 12, 207, 218, 232, 255, &label);
            ui::painter_line(0, y + HEIGHT, 672, y + HEIGHT, 56, 62, 75, 255, 1);
            for (i, note) in row.notes.iter().enumerate() {
                let x = LEFT + i32::from(note.start) * CELL;
                let w = i32::from(note.end - note.start) * CELL;
                let active = clock.running && note.level(clock.step, false, 255) > 0;
                let selected = row_index == self.row && self.note == Some(i);
                ui::painter_rect(x + 1, y + 3, w - 2, HEIGHT - 6, if active { 95 } else { 48 }, if selected { 210 } else { 148 }, 177, 255);
                ui::painter_line(x + 5, y + 7, x + 5, y + HEIGHT - 7, 218, 241, 243, 255, 2);
                ui::painter_line(x + w - 5, y + 7, x + w - 5, y + HEIGHT - 7, 218, 241, 243, 255, 2);
            }
        }
        if clock.running {
            let x = LEFT + (clock.step * f64::from(CELL)) as i32;
            ui::painter_line(x, TOP, x, 184, 255, 214, 101, 255, 2);
        }
        ui::painter_end();
        ui::begin_horizontal();
        ui::button("Previous rows", 12);
        ui::label(&format!("Page {}/{}", self.page + 1, pattern.rows.len().saturating_sub(1) / PAGE_ROWS + 1));
        ui::button("Next rows", 13);
        ui::button("Add row", 8);
        ui::button("Delete row", 9);
        ui::end_horizontal();
        ui::begin_horizontal();
        ui::label(&format!("Row {} target:", self.row + 1));
        ui::button("<", 10);
        ui::label(&Self::target_name(pattern.rows.get(self.row).and_then(|r| r.target)));
        ui::button(">", 11);
        ui::button("Unassign", 14);
        ui::end_horizontal();
        ui::begin_horizontal();
        ui::button("Delete note", 6);
        ui::button("Clear roll", 7);
        ui::label("Draw: empty grid | Move: note center | Resize: edges");
        ui::end_horizontal();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draw_move_resize_and_reject_overlap() {
        let mut p = Pattern::new_for(&[(1, 1)]);
        let mut e = Editor::default();
        e.event(&mut p, PluginUiEvent::CanvasDragStart { id: CANVAS, x: LEFT + 10, y: TOP + 10 });
        e.event(&mut p, PluginUiEvent::CanvasDragEnd { id: CANVAS, x: LEFT + 3 * CELL + 10, y: TOP + 10 });
        assert_eq!(p.rows[0].notes[0], Note { start: 0, end: 4 });
        e.event(&mut p, PluginUiEvent::CanvasDragStart { id: CANVAS, x: LEFT + 16, y: TOP + 10 });
        e.event(&mut p, PluginUiEvent::CanvasDragEnd { id: CANVAS, x: LEFT + 4 * CELL + 16, y: TOP + 10 });
        assert_eq!(p.rows[0].notes[0], Note { start: 4, end: 8 });
        e.event(&mut p, PluginUiEvent::CanvasDragStart { id: CANVAS, x: LEFT + 8 * CELL - 3, y: TOP + 10 });
        e.event(&mut p, PluginUiEvent::CanvasDragEnd { id: CANVAS, x: LEFT + 10 * CELL, y: TOP + 10 });
        assert_eq!(p.rows[0].notes[0], Note { start: 4, end: 10 });
        p.rows[0].notes.push(Note { start: 12, end: 16 });
        e.event(&mut p, PluginUiEvent::CanvasDragStart { id: CANVAS, x: LEFT + 10 * CELL - 3, y: TOP + 10 });
        e.event(&mut p, PluginUiEvent::CanvasDragEnd { id: CANVAS, x: LEFT + 14 * CELL, y: TOP + 10 });
        assert_eq!(p.rows[0].notes[0], Note { start: 4, end: 10 });
    }
    #[test]
    fn manual_remapping_and_mode_change_preserve_notes() {
        let mut p = Pattern::new_for(&[(1, 1), (2, 1)]);
        let mut e = Editor::default();
        p.rows[0].notes.push(Note { start: 2, end: 5 });
        e.event(&mut p, PluginUiEvent::Switch { id: 1, value: true });
        assert!(!p.valid_mapping());
        assert_eq!(p.rows[0].notes.len(), 1);
        e.event(&mut p, PluginUiEvent::Button { id: 11 });
        e.row = 1;
        e.event(&mut p, PluginUiEvent::Button { id: 11 });
        assert!(p.valid_mapping());
        assert_ne!(p.rows[0].target, p.rows[1].target);
    }
}
