use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const STAGE_SHOWFILE_VERSION: u32 = 3;

pub const TRUSS_PROFILE_SIZE_M: f32 = 0.29;
pub const TRUSS_CHORD_DIAMETER_M: f32 = 0.05;
pub const TRUSS_MIN_LENGTH_M: f32 = 0.25;
pub const TRUSS_MAX_LENGTH_M: f32 = 50.0;
pub const TRUSS_LENGTH_STEP_M: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrussProfile {
    Square,
    Triangular,
    Ladder,
}

impl Default for TrussProfile {
    fn default() -> Self {
        Self::Square
    }
}

impl TrussProfile {
    pub const ALL: [Self; 3] = [Self::Square, Self::Triangular, Self::Ladder];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Square => "Square",
            Self::Triangular => "Triangular",
            Self::Ladder => "Ladder",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrussPart {
    Straight,
    Corner90,
    TJunction,
    Cross,
    BasePlate,
}

impl Default for TrussPart {
    fn default() -> Self {
        Self::Straight
    }
}

impl TrussPart {
    pub const ALL: [Self; 5] = [
        Self::Straight,
        Self::Corner90,
        Self::TJunction,
        Self::Cross,
        Self::BasePlate,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Straight => "Straight",
            Self::Corner90 => "90 deg Corner",
            Self::TJunction => "T-Junction",
            Self::Cross => "Cross",
            Self::BasePlate => "Base Plate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrussSpec {
    #[serde(default)]
    pub profile: TrussProfile,
    #[serde(default)]
    pub part: TrussPart,
    #[serde(default = "default_truss_length")]
    pub length_m: f32,
}

const fn default_truss_length() -> f32 {
    3.0
}

impl Default for TrussSpec {
    fn default() -> Self {
        Self {
            profile: TrussProfile::Square,
            part: TrussPart::Straight,
            length_m: default_truss_length(),
        }
    }
}

impl TrussSpec {
    pub fn sanitize(&mut self) {
        let length = if self.length_m.is_finite() {
            self.length_m
        } else {
            default_truss_length()
        };
        self.length_m = ((length / TRUSS_LENGTH_STEP_M).round() * TRUSS_LENGTH_STEP_M)
            .clamp(TRUSS_MIN_LENGTH_M, TRUSS_MAX_LENGTH_M);
    }

    pub fn endpoint_count(self) -> u8 {
        match self.part {
            TrussPart::Straight => 2,
            TrussPart::Corner90 => 2,
            TrussPart::TJunction => 3,
            TrussPart::Cross => 4,
            TrussPart::BasePlate => 1,
        }
    }

    pub fn branch_count(self) -> u8 {
        match self.part {
            TrussPart::Straight | TrussPart::BasePlate => 1,
            TrussPart::Corner90 => 2,
            TrussPart::TJunction => 3,
            TrussPart::Cross => 4,
        }
    }

    pub fn branch_length(self, branch: u8) -> Option<f32> {
        if branch >= self.branch_count() {
            return None;
        }
        Some(match self.part {
            TrussPart::Straight => self.length_m,
            TrussPart::BasePlate => 0.25,
            TrussPart::Corner90 | TrussPart::TJunction | TrussPart::Cross => 0.5,
        })
    }

    pub fn section_breakdown(self) -> Vec<f32> {
        if self.part != TrussPart::Straight {
            return Vec::new();
        }
        let mut remaining = (self.length_m / TRUSS_LENGTH_STEP_M).round() as u32;
        let mut sections = Vec::new();
        for (quarter_units, metres) in [(8, 2.0), (4, 1.0), (2, 0.5), (1, 0.25)] {
            while remaining >= quarter_units {
                sections.push(metres);
                remaining -= quarter_units;
            }
        }
        sections
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TrussEndpointRef {
    pub object_id: u64,
    pub endpoint: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrussConnection {
    pub a: TrussEndpointRef,
    pub b: TrussEndpointRef,
}

impl TrussConnection {
    pub fn normalized(mut self) -> Self {
        if self.b < self.a {
            std::mem::swap(&mut self.a, &mut self.b);
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixtureTrussAttachment {
    pub group_id: u8,
    pub fixture_id: u8,
    pub truss_id: u64,
    pub branch: u8,
    pub distance_m: f32,
    #[serde(default)]
    pub rotation_offset: [f32; 3],
}

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
    ProceduralTruss(TrussSpec),
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
    #[serde(default)]
    pub truss_connections: Vec<TrussConnection>,
    #[serde(default)]
    pub fixture_attachments: Vec<FixtureTrussAttachment>,
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
            truss_connections: Vec::new(),
            fixture_attachments: Vec::new(),
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
        self.truss_connections
            .retain(|connection| connection.a.object_id != id && connection.b.object_id != id);
        self.fixture_attachments
            .retain(|attachment| attachment.truss_id != id);
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
        for object in self.objects.values_mut() {
            if matches!(object.kind, StageObjectKind::Truss) {
                let mut spec = TrussSpec {
                    length_m: object.transform.scale[0],
                    ..Default::default()
                };
                spec.sanitize();
                object.kind = StageObjectKind::ProceduralTruss(spec);
                object.transform.scale = [1.0; 3];
            }
        }
        self.version = STAGE_SHOWFILE_VERSION;
        self.room.width = finite_clamp(self.room.width, 20.0, 2.0, 200.0);
        self.room.depth = finite_clamp(self.room.depth, 20.0, 2.0, 200.0);
        self.room.height = finite_clamp(self.room.height, 8.0, 2.0, 50.0);
        for object in self.objects.values_mut() {
            object.transform.sanitize();
            if let StageObjectKind::ProceduralTruss(spec) = &mut object.kind {
                spec.sanitize();
                object.transform.scale = [1.0; 3];
            }
        }
        self.sanitize_truss_relationships();
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

    fn sanitize_truss_relationships(&mut self) {
        let trusses: BTreeMap<u64, TrussSpec> = self
            .objects
            .iter()
            .filter_map(|(id, object)| match object.kind {
                StageObjectKind::ProceduralTruss(spec) => Some((*id, spec)),
                _ => None,
            })
            .collect();
        let mut occupied = std::collections::BTreeSet::new();
        let mut connections = std::collections::BTreeSet::new();
        for connection in std::mem::take(&mut self.truss_connections) {
            let connection = connection.normalized();
            let Some(a) = trusses.get(&connection.a.object_id) else {
                continue;
            };
            let Some(b) = trusses.get(&connection.b.object_id) else {
                continue;
            };
            if connection.a.object_id == connection.b.object_id
                || connection.a.endpoint >= a.endpoint_count()
                || connection.b.endpoint >= b.endpoint_count()
                || a.profile != b.profile
                || occupied.contains(&connection.a)
                || occupied.contains(&connection.b)
            {
                continue;
            }
            occupied.insert(connection.a);
            occupied.insert(connection.b);
            connections.insert((connection.a, connection.b));
        }
        self.truss_connections = connections
            .into_iter()
            .map(|(a, b)| TrussConnection { a, b })
            .collect();

        let mut fixture_keys = std::collections::BTreeSet::new();
        self.fixture_attachments.retain_mut(|attachment| {
            let Some(spec) = trusses.get(&attachment.truss_id) else {
                return false;
            };
            let Some(length) = spec.branch_length(attachment.branch) else {
                return false;
            };
            if !fixture_keys.insert((attachment.group_id, attachment.fixture_id)) {
                return false;
            }
            attachment.distance_m = if attachment.distance_m.is_finite() {
                attachment.distance_m.clamp(0.0, length)
            } else {
                length * 0.5
            };
            for value in &mut attachment.rotation_offset {
                if !value.is_finite() {
                    *value = 0.0;
                }
            }
            true
        });
    }

    pub fn truss_spec(&self, id: u64) -> Option<TrussSpec> {
        match self.objects.get(&id)?.kind {
            StageObjectKind::ProceduralTruss(spec) => Some(spec),
            _ => None,
        }
    }

    pub fn connected_trusses(&self, root: u64) -> std::collections::BTreeSet<u64> {
        let mut connected = std::collections::BTreeSet::from([root]);
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            for connection in &self.truss_connections {
                for neighbour in [
                    (connection.a.object_id == id).then_some(connection.b.object_id),
                    (connection.b.object_id == id).then_some(connection.a.object_id),
                ]
                .into_iter()
                .flatten()
                {
                    if connected.insert(neighbour) {
                        pending.push(neighbour);
                    }
                }
            }
        }
        connected
    }

    pub fn disconnect_truss(&mut self, id: u64) {
        self.truss_connections
            .retain(|connection| connection.a.object_id != id && connection.b.object_id != id);
    }

    pub fn is_truss_connected(&self, id: u64) -> bool {
        self.truss_connections
            .iter()
            .any(|connection| connection.a.object_id == id || connection.b.object_id == id)
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
            StageObjectKind::ProceduralTruss(_) => ("Truss", [1.0; 3], [170, 174, 182]),
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

    pub fn truss_preset(index: usize, spec: TrussSpec) -> Self {
        Self {
            name: format!("Truss {index}"),
            visible: true,
            locked: false,
            kind: StageObjectKind::ProceduralTruss(spec),
            transform: StageTransform::default(),
            color: [170, 174, 182],
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

    #[test]
    fn straight_lengths_are_quantized_and_decomposed_largest_first() {
        let mut spec = TrussSpec {
            length_m: 3.74,
            ..Default::default()
        };
        spec.sanitize();
        assert_eq!(spec.length_m, 3.75);
        assert_eq!(spec.section_breakdown(), vec![2.0, 1.0, 0.5, 0.25]);
    }

    #[test]
    fn legacy_truss_is_migrated_without_stretched_scale() {
        let mut scene = StageScene::default();
        scene.version = 1;
        let id = scene.add_object(StageObject::preset(StageObjectKind::Truss, 1));

        scene.sanitize();

        assert_eq!(scene.version, STAGE_SHOWFILE_VERSION);
        assert_eq!(scene.objects[&id].transform.scale, [1.0; 3]);
        assert_eq!(scene.truss_spec(id).unwrap().length_m, 3.0);
    }

    #[test]
    fn relationship_sanitizer_rejects_duplicate_and_incompatible_endpoints() {
        let mut scene = StageScene::default();
        let square_a = scene.add_object(StageObject::truss_preset(1, TrussSpec::default()));
        let square_b = scene.add_object(StageObject::truss_preset(2, TrussSpec::default()));
        let triangle = scene.add_object(StageObject::truss_preset(
            3,
            TrussSpec {
                profile: TrussProfile::Triangular,
                ..Default::default()
            },
        ));
        let valid = TrussConnection {
            a: TrussEndpointRef {
                object_id: square_a,
                endpoint: 1,
            },
            b: TrussEndpointRef {
                object_id: square_b,
                endpoint: 0,
            },
        };
        scene.truss_connections = vec![
            valid,
            valid,
            TrussConnection {
                a: TrussEndpointRef {
                    object_id: square_a,
                    endpoint: 0,
                },
                b: TrussEndpointRef {
                    object_id: triangle,
                    endpoint: 0,
                },
            },
        ];

        scene.sanitize();

        assert_eq!(scene.truss_connections, vec![valid.normalized()]);
        assert_eq!(
            scene.connected_trusses(square_a),
            std::collections::BTreeSet::from([square_a, square_b])
        );
    }

    #[test]
    fn deleting_truss_removes_connections_and_attachments() {
        let mut scene = StageScene::default();
        let a = scene.add_object(StageObject::truss_preset(1, TrussSpec::default()));
        let b = scene.add_object(StageObject::truss_preset(2, TrussSpec::default()));
        scene.truss_connections.push(TrussConnection {
            a: TrussEndpointRef {
                object_id: a,
                endpoint: 1,
            },
            b: TrussEndpointRef {
                object_id: b,
                endpoint: 0,
            },
        });
        scene.fixture_attachments.push(FixtureTrussAttachment {
            group_id: 1,
            fixture_id: 2,
            truss_id: a,
            branch: 0,
            distance_m: 1.0,
            rotation_offset: [0.0; 3],
        });

        scene.remove_object(a);

        assert!(scene.truss_connections.is_empty());
        assert!(scene.fixture_attachments.is_empty());
    }

    #[test]
    fn stage_relationships_survive_json_round_trip() {
        let mut scene = StageScene::default();
        let a = scene.add_object(StageObject::truss_preset(1, TrussSpec::default()));
        let b = scene.add_object(StageObject::truss_preset(2, TrussSpec::default()));
        scene.truss_connections.push(TrussConnection {
            a: TrussEndpointRef {
                object_id: a,
                endpoint: 1,
            },
            b: TrussEndpointRef {
                object_id: b,
                endpoint: 0,
            },
        });
        scene.fixture_attachments.push(FixtureTrussAttachment {
            group_id: 3,
            fixture_id: 4,
            truss_id: a,
            branch: 0,
            distance_m: 0.75,
            rotation_offset: [1.0, 2.0, 3.0],
        });

        let json = serde_json::to_string(&scene).unwrap();
        let restored: StageScene = serde_json::from_str(&json).unwrap();

        assert_eq!(restored, scene);
    }
}
