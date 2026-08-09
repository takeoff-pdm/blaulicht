use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const STAGE_SHOWFILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StageTransform {
    pub translation: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
}

impl Default for StageTransform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

impl StageTransform {
    pub fn sanitize(&mut self) {
        for value in &mut self.translation {
            if !value.is_finite() {
                *value = 0.0;
            }
        }
        for value in &mut self.rotation {
            if !value.is_finite() {
                *value = 0.0;
            }
        }
        for value in &mut self.scale {
            if !value.is_finite() {
                *value = 1.0;
            }
            *value = value.abs().clamp(0.01, 1_000.0);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageRoom {
    pub visible: bool,
    pub width: f32,
    pub depth: f32,
    pub height: f32,
    pub floor_color: [u8; 3],
    pub wall_color: [u8; 3],
}

impl Default for StageRoom {
    fn default() -> Self {
        Self {
            visible: true,
            width: 20.0,
            depth: 20.0,
            height: 8.0,
            floor_color: [28, 30, 38],
            wall_color: [38, 41, 50],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StageObjectKind {
    Platform,
    Truss,
    Wall,
    Speaker,
    Screen,
    Box,
    ImportedModel { asset_id: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageObject {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub kind: StageObjectKind,
    pub transform: StageTransform,
    pub color: [u8; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageAsset {
    pub original_name: String,
    pub relative_path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageScene {
    #[serde(default = "stage_version")]
    pub version: u32,
    #[serde(default)]
    pub room: StageRoom,
    #[serde(default)]
    pub objects: BTreeMap<u64, StageObject>,
    #[serde(default)]
    pub assets: BTreeMap<u64, StageAsset>,
    #[serde(default = "first_id")]
    next_object_id: u64,
    #[serde(default = "first_id")]
    next_asset_id: u64,
}

const fn stage_version() -> u32 {
    STAGE_SHOWFILE_VERSION
}

const fn first_id() -> u64 {
    1
}

impl Default for StageScene {
    fn default() -> Self {
        Self {
            version: STAGE_SHOWFILE_VERSION,
            room: StageRoom::default(),
            objects: BTreeMap::new(),
            assets: BTreeMap::new(),
            next_object_id: 1,
            next_asset_id: 1,
        }
    }
}

impl StageScene {
    pub fn add_object(&mut self, mut object: StageObject) -> u64 {
        object.transform.sanitize();
        let id = self.allocate_object_id();
        self.objects.insert(id, object);
        id
    }

    pub fn remove_object(&mut self, id: u64) -> Option<StageObject> {
        self.objects.remove(&id)
    }

    pub fn add_asset(&mut self, asset: StageAsset) -> u64 {
        if let Some((&id, _)) = self
            .assets
            .iter()
            .find(|(_, existing)| existing.content_hash == asset.content_hash)
        {
            return id;
        }
        let id = self.allocate_asset_id();
        self.assets.insert(id, asset);
        id
    }

    pub fn sanitize(&mut self) {
        self.version = STAGE_SHOWFILE_VERSION;
        self.room.width = finite_clamp(self.room.width, 20.0, 2.0, 200.0);
        self.room.depth = finite_clamp(self.room.depth, 20.0, 2.0, 200.0);
        self.room.height = finite_clamp(self.room.height, 8.0, 2.0, 50.0);
        for object in self.objects.values_mut() {
            object.transform.sanitize();
        }
        self.next_object_id = self.next_object_id.max(
            self.objects
                .keys()
                .next_back()
                .copied()
                .unwrap_or(0)
                .saturating_add(1),
        );
        self.next_asset_id = self.next_asset_id.max(
            self.assets
                .keys()
                .next_back()
                .copied()
                .unwrap_or(0)
                .saturating_add(1),
        );
    }

    fn allocate_object_id(&mut self) -> u64 {
        while self.objects.contains_key(&self.next_object_id) {
            self.next_object_id = self.next_object_id.saturating_add(1).max(1);
        }
        let id = self.next_object_id;
        self.next_object_id = self.next_object_id.saturating_add(1).max(1);
        id
    }

    fn allocate_asset_id(&mut self) -> u64 {
        while self.assets.contains_key(&self.next_asset_id) {
            self.next_asset_id = self.next_asset_id.saturating_add(1).max(1);
        }
        let id = self.next_asset_id;
        self.next_asset_id = self.next_asset_id.saturating_add(1).max(1);
        id
    }
}

fn finite_clamp(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

impl StageObject {
    pub fn preset(kind: StageObjectKind, index: usize) -> Self {
        let (name, scale, color) = match kind {
            StageObjectKind::Platform => ("Platform", [4.0, 0.4, 2.0], [65, 68, 76]),
            StageObjectKind::Truss => ("Truss", [3.0, 0.3, 0.3], [155, 160, 168]),
            StageObjectKind::Wall => ("Wall", [4.0, 3.0, 0.2], [92, 96, 105]),
            StageObjectKind::Speaker => ("Speaker", [0.7, 1.2, 0.6], [35, 36, 40]),
            StageObjectKind::Screen => ("Screen", [3.0, 1.7, 0.15], [34, 38, 45]),
            StageObjectKind::Box => ("Box", [1.0, 1.0, 1.0], [100, 105, 115]),
            StageObjectKind::ImportedModel { .. } => ("Model", [1.0, 1.0, 1.0], [180, 180, 180]),
        };
        Self {
            name: format!("{name} {index}"),
            visible: true,
            locked: false,
            kind,
            transform: StageTransform {
                scale,
                ..Default::default()
            },
            color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_not_reused() {
        let mut scene = StageScene::default();
        let first = scene.add_object(StageObject::preset(StageObjectKind::Box, 1));
        scene.remove_object(first);
        let second = scene.add_object(StageObject::preset(StageObjectKind::Box, 2));
        assert!(second > first);
    }

    #[test]
    fn sanitizes_non_finite_transforms() {
        let mut transform = StageTransform {
            translation: [f32::NAN, 2.0, f32::INFINITY],
            rotation: [0.0, f32::NAN, 2.0],
            scale: [0.0, -2.0, f32::NAN],
        };
        transform.sanitize();
        assert_eq!(transform.translation, [0.0, 2.0, 0.0]);
        assert_eq!(transform.scale, [0.01, 2.0, 1.0]);
    }
}
