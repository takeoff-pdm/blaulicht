use crate::stage::StageAsset;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use three_d_asset::{Geometry, Indices, Model, Positions, Scene, Texture2D, TextureData};

const MAX_GLB_BYTES: u64 = 100 * 1024 * 1024;
const MAX_TRIANGLES: usize = 2_000_000;
const MAX_TRIANGLES_PER_PRIMITIVE: usize = 250_000;
const MAX_PRIMITIVES: usize = 512;
const MAX_MATERIALS: usize = 256;
const MAX_TEXTURE_DIMENSION: u32 = 1_024;
const MAX_TEXTURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_TOTAL_TEXTURE_BYTES: usize = 64 * 1024 * 1024;
const MAX_SOURCE_TEXTURE_DIMENSION: u32 = 8_192;
const MAX_SOURCE_TEXTURE_BYTES: usize = 256 * 1024 * 1024;
const MAX_PRIMITIVE_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOTAL_GEOMETRY_BYTES: usize = 192 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default)]
pub struct StageModelStats {
    pub triangles: usize,
    pub primitives: usize,
    pub materials: usize,
    pub textures: usize,
    pub textures_downscaled: usize,
    pub texture_bytes: usize,
    pub geometry_bytes: usize,
}

#[derive(Debug)]
pub struct PreparedStageModel {
    pub model: Model,
    pub stats: StageModelStats,
}

pub fn load_prepared_model(path: &Path) -> Result<PreparedStageModel> {
    let metadata = fs::metadata(path).with_context(|| format!("Cannot read {path:?}"))?;
    if metadata.len() > MAX_GLB_BYTES {
        bail!("GLB exceeds the 100 MiB import limit");
    }
    let scene: Scene = three_d_asset::io::load_and_deserialize(path)
        .map_err(|error| anyhow::anyhow!("Invalid GLB: {error}"))?;
    let mut model = Model::from(scene);
    let stats = validate_and_prepare_model(&mut model)?;
    Ok(PreparedStageModel { model, stats })
}

fn validate_and_prepare_model(model: &mut Model) -> Result<StageModelStats> {
    if model.geometries.len() > MAX_PRIMITIVES {
        bail!("Model contains more than {MAX_PRIMITIVES} primitives");
    }
    if model.materials.len() > MAX_MATERIALS {
        bail!("Model contains more than {MAX_MATERIALS} materials");
    }

    let mut stats = StageModelStats {
        primitives: model.geometries.len(),
        materials: model.materials.len(),
        ..Default::default()
    };
    for primitive in &model.geometries {
        if primitive
            .material_index
            .is_some_and(|index| index >= model.materials.len())
        {
            bail!(
                "Primitive {:?} references a missing material",
                primitive.name
            );
        }
        let Geometry::Triangles(mesh) = &primitive.geometry else {
            bail!("Point-cloud primitives are not supported by the visualizer");
        };
        let triangles = mesh.triangle_count();
        if triangles > MAX_TRIANGLES_PER_PRIMITIVE {
            bail!(
                "Primitive {:?} exceeds the {MAX_TRIANGLES_PER_PRIMITIVE} triangle limit",
                primitive.name
            );
        }
        stats.triangles = stats.triangles.saturating_add(triangles);
        let bytes = triangle_mesh_bytes(mesh);
        if bytes > MAX_PRIMITIVE_BYTES {
            bail!(
                "Primitive {:?} exceeds the 32 MiB decoded geometry limit",
                primitive.name
            );
        }
        stats.geometry_bytes = stats.geometry_bytes.saturating_add(bytes);
    }
    if stats.triangles > MAX_TRIANGLES {
        bail!("GLB contains more than two million triangles");
    }
    if stats.geometry_bytes > MAX_TOTAL_GEOMETRY_BYTES {
        bail!("Model exceeds the 192 MiB total decoded geometry limit");
    }

    for material in &mut model.materials {
        prepare_material_textures(material, &mut stats)?;
        if let Some(texture) = material.albedo_texture.as_mut() {
            texture.data.to_linear_srgb();
        }
        if let Some(texture) = material.emissive_texture.as_mut() {
            texture.data.to_linear_srgb();
        }
    }
    if stats.texture_bytes > MAX_TOTAL_TEXTURE_BYTES {
        bail!("Model exceeds the 64 MiB total decoded texture limit");
    }
    Ok(stats)
}

fn prepare_material_textures(
    material: &mut three_d_asset::PbrMaterial,
    stats: &mut StageModelStats,
) -> Result<()> {
    macro_rules! prepare {
        ($field:expr) => {
            if let Some(texture) = $field.as_mut() {
                if prepare_texture(texture)? {
                    stats.textures_downscaled += 1;
                }
                stats.textures += 1;
                stats.texture_bytes = stats.texture_bytes.saturating_add(texture_bytes(texture));
            }
        };
    }
    prepare!(material.albedo_texture);
    prepare!(material.occlusion_metallic_roughness_texture);
    prepare!(material.metallic_roughness_texture);
    prepare!(material.occlusion_texture);
    prepare!(material.normal_texture);
    prepare!(material.emissive_texture);
    prepare!(material.transmission_texture);
    Ok(())
}

fn prepare_texture(texture: &mut Texture2D) -> Result<bool> {
    validate_texture_source(texture)?;
    let needs_resize = texture.width > MAX_TEXTURE_DIMENSION
        || texture.height > MAX_TEXTURE_DIMENSION
        || texture_bytes(texture) > MAX_TEXTURE_BYTES;
    if needs_resize {
        let scale = (MAX_TEXTURE_DIMENSION as f32 / texture.width as f32)
            .min(MAX_TEXTURE_DIMENSION as f32 / texture.height as f32)
            .min(1.0);
        let width = ((texture.width as f32 * scale).round() as u32).max(1);
        let height = ((texture.height as f32 * scale).round() as u32).max(1);
        resize_texture_data(texture, width, height);
        texture.width = width;
        texture.height = height;
        texture.mipmap = Some(three_d_asset::Mipmap::default());
    }
    if texture_bytes(texture) > MAX_TEXTURE_BYTES {
        bail!("Texture {:?} exceeds the 4 MiB prepared limit", texture.name);
    }
    Ok(needs_resize)
}

fn validate_texture_source(texture: &Texture2D) -> Result<()> {
    if texture.width == 0 || texture.height == 0 {
        bail!("Texture {:?} has zero dimensions", texture.name);
    }
    if texture.width > MAX_SOURCE_TEXTURE_DIMENSION
        || texture.height > MAX_SOURCE_TEXTURE_DIMENSION
    {
        bail!(
            "Texture {:?} is {}x{}; the source safety limit is {}x{}",
            texture.name,
            texture.width,
            texture.height,
            MAX_SOURCE_TEXTURE_DIMENSION,
            MAX_SOURCE_TEXTURE_DIMENSION
        );
    }
    let expected_pixels = texture.width as usize * texture.height as usize;
    let actual_pixels = texture_pixels(texture);
    if actual_pixels != expected_pixels {
        bail!(
            "Texture {:?} contains {} pixels but {}x{} requires {expected_pixels}",
            texture.name,
            actual_pixels,
            texture.width,
            texture.height
        );
    }
    if texture_bytes(texture) > MAX_SOURCE_TEXTURE_BYTES {
        bail!("Texture {:?} exceeds the 256 MiB source safety limit", texture.name);
    }
    Ok(())
}

fn resize_texture_data(texture: &mut Texture2D, width: u32, height: u32) {
    let source_width = texture.width;
    let source_height = texture.height;
    match &mut texture.data {
        TextureData::RU8(values) => {
            let image = image::GrayImage::from_raw(source_width, source_height, std::mem::take(values))
                .expect("validated grayscale texture dimensions");
            *values = image::imageops::resize(
                &image,
                width,
                height,
                image::imageops::FilterType::Triangle,
            )
            .into_raw();
        }
        TextureData::RgU8(values) => {
            let raw = std::mem::take(values)
                .into_iter()
                .flat_map(|pixel| pixel)
                .collect();
            let image = image::ImageBuffer::<image::LumaA<u8>, Vec<u8>>::from_raw(
                source_width,
                source_height,
                raw,
            )
            .expect("validated grayscale-alpha texture dimensions");
            *values = image::imageops::resize(
                &image,
                width,
                height,
                image::imageops::FilterType::Triangle,
            )
            .into_raw()
            .chunks_exact(2)
            .map(|pixel| [pixel[0], pixel[1]])
            .collect();
        }
        TextureData::RgbU8(values) => {
            let raw = std::mem::take(values)
                .into_iter()
                .flat_map(|pixel| pixel)
                .collect();
            let image = image::RgbImage::from_raw(source_width, source_height, raw)
                .expect("validated RGB texture dimensions");
            *values = image::imageops::resize(
                &image,
                width,
                height,
                image::imageops::FilterType::Triangle,
            )
            .into_raw()
            .chunks_exact(3)
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();
        }
        TextureData::RgbaU8(values) => {
            let raw = std::mem::take(values)
                .into_iter()
                .flat_map(|pixel| pixel)
                .collect();
            let image = image::RgbaImage::from_raw(source_width, source_height, raw)
                .expect("validated RGBA texture dimensions");
            *values = image::imageops::resize(
                &image,
                width,
                height,
                image::imageops::FilterType::Triangle,
            )
            .into_raw()
            .chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
            .collect();
        }
        TextureData::RF16(values) => *values = resize_nearest(values, source_width, source_height, width, height),
        TextureData::RgF16(values) => *values = resize_nearest(values, source_width, source_height, width, height),
        TextureData::RgbF16(values) => *values = resize_nearest(values, source_width, source_height, width, height),
        TextureData::RgbaF16(values) => *values = resize_nearest(values, source_width, source_height, width, height),
        TextureData::RF32(values) => *values = resize_nearest(values, source_width, source_height, width, height),
        TextureData::RgF32(values) => *values = resize_nearest(values, source_width, source_height, width, height),
        TextureData::RgbF32(values) => *values = resize_nearest(values, source_width, source_height, width, height),
        TextureData::RgbaF32(values) => *values = resize_nearest(values, source_width, source_height, width, height),
    }
}

fn resize_nearest<T: Copy>(
    source: &[T],
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
) -> Vec<T> {
    let mut output = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        let source_y = ((y as u64 * source_height as u64) / height as u64)
            .min(source_height.saturating_sub(1) as u64) as usize;
        for x in 0..width {
            let source_x = ((x as u64 * source_width as u64) / width as u64)
                .min(source_width.saturating_sub(1) as u64) as usize;
            output.push(source[source_y * source_width as usize + source_x]);
        }
    }
    output
}

fn validate_texture(texture: &Texture2D) -> Result<()> {
    let mut texture = texture.clone();
    prepare_texture(&mut texture).map(|_| ())
}

fn texture_pixels(texture: &Texture2D) -> usize {
    match &texture.data {
        TextureData::RU8(values) => values.len(),
        TextureData::RgU8(values) => values.len(),
        TextureData::RgbU8(values) => values.len(),
        TextureData::RgbaU8(values) => values.len(),
        TextureData::RF16(values) => values.len(),
        TextureData::RgF16(values) => values.len(),
        TextureData::RgbF16(values) => values.len(),
        TextureData::RgbaF16(values) => values.len(),
        TextureData::RF32(values) => values.len(),
        TextureData::RgF32(values) => values.len(),
        TextureData::RgbF32(values) => values.len(),
        TextureData::RgbaF32(values) => values.len(),
    }
}

fn texture_bytes(texture: &Texture2D) -> usize {
    match &texture.data {
        TextureData::RU8(values) => values.len(),
        TextureData::RgU8(values) => values.len() * 2,
        TextureData::RgbU8(values) => values.len() * 3,
        TextureData::RgbaU8(values) => values.len() * 4,
        TextureData::RF16(values) => values.len() * 2,
        TextureData::RgF16(values) => values.len() * 4,
        TextureData::RgbF16(values) => values.len() * 6,
        TextureData::RgbaF16(values) => values.len() * 8,
        TextureData::RF32(values) => values.len() * 4,
        TextureData::RgF32(values) => values.len() * 8,
        TextureData::RgbF32(values) => values.len() * 12,
        TextureData::RgbaF32(values) => values.len() * 16,
    }
}

fn triangle_mesh_bytes(mesh: &three_d_asset::TriMesh) -> usize {
    let positions = match &mesh.positions {
        Positions::F32(values) => values.len() * 3 * std::mem::size_of::<f32>(),
        Positions::F64(values) => values.len() * 3 * std::mem::size_of::<f64>(),
    };
    let indices = match &mesh.indices {
        Indices::None => 0,
        Indices::U8(values) => values.len(),
        Indices::U16(values) => values.len() * 2,
        Indices::U32(values) => values.len() * 4,
    };
    positions
        + indices
        + mesh.normals.as_ref().map_or(0, |values| values.len() * 12)
        + mesh.tangents.as_ref().map_or(0, |values| values.len() * 16)
        + mesh.uvs.as_ref().map_or(0, |values| values.len() * 8)
        + mesh.colors.as_ref().map_or(0, |values| values.len() * 4)
}

pub fn import_glb(source: &Path, showfile: &Path) -> Result<StageAsset> {
    if source
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        != Some("glb".to_string())
    {
        bail!("Only .glb models are supported");
    }
    let metadata = fs::metadata(source).with_context(|| format!("Cannot read {source:?}"))?;
    if !metadata.is_file() {
        bail!("Selected model is not a regular file");
    }
    if metadata.len() > MAX_GLB_BYTES {
        bail!("GLB exceeds the 100 MB import limit");
    }

    let bytes = fs::read(source).with_context(|| format!("Cannot read {source:?}"))?;
    if bytes.len() < 12 || &bytes[..4] != b"glTF" {
        bail!("File is not a binary glTF model");
    }

    let _prepared = load_prepared_model(source)?;

    let hash = format!("{:x}", Sha256::digest(&bytes));
    let parent = showfile.parent().unwrap_or_else(|| Path::new("."));
    let stem = showfile
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("showfile");
    let folder_name = format!("{stem}.assets");
    let asset_dir = parent.join(&folder_name);
    fs::create_dir_all(&asset_dir)
        .with_context(|| format!("Cannot create asset directory {asset_dir:?}"))?;
    let file_name = format!("{hash}.glb");
    let destination = asset_dir.join(&file_name);
    if !destination.exists() {
        crate::config::write_atomic(&destination, &bytes)
            .with_context(|| format!("Cannot install imported model {destination:?}"))?;
    }

    Ok(StageAsset {
        original_name: source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("model.glb")
            .to_string(),
        relative_path: PathBuf::from(folder_name)
            .join(file_name)
            .to_string_lossy()
            .into_owned(),
        content_hash: hash,
    })
}

pub fn resolve_asset(showfile: &Path, asset: &StageAsset) -> Result<PathBuf> {
    let relative = Path::new(&asset.relative_path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        })
    {
        bail!("Asset path escapes the showfile directory");
    }
    let path = showfile
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(relative);
    if !path.is_file() {
        bail!("Managed model is missing: {path:?}");
    }
    Ok(path)
}

pub fn copy_assets_for_save_as(
    stage: &crate::stage::StageScene,
    source_showfile: &Path,
    destination_showfile: &Path,
) -> Result<crate::stage::StageScene> {
    if source_showfile == destination_showfile || stage.assets.is_empty() {
        return Ok(stage.clone());
    }
    let mut updated = stage.clone();
    let destination_parent = destination_showfile
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let stem = destination_showfile
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("showfile");
    let folder_name = format!("{stem}.assets");
    let destination_folder = destination_parent.join(&folder_name);
    fs::create_dir_all(&destination_folder)?;

    for asset in updated.assets.values_mut() {
        let source = resolve_asset(source_showfile, asset)?;
        let file_name = format!("{}.glb", asset.content_hash);
        let destination = destination_folder.join(&file_name);
        if !destination.exists() {
            let bytes = fs::read(&source)
                .with_context(|| format!("Cannot copy managed model {source:?}"))?;
            crate::config::write_atomic(&destination, &bytes)?;
        }
        asset.relative_path = PathBuf::from(&folder_name)
            .join(file_name)
            .to_string_lossy()
            .into_owned();
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_asset(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-assets")
            .join(name)
    }

    #[test]
    fn rejects_path_traversal() {
        let asset = StageAsset {
            original_name: "bad.glb".into(),
            relative_path: "../bad.glb".into(),
            content_hash: String::new(),
        };
        assert!(resolve_asset(Path::new("/tmp/show.json"), &asset).is_err());
    }

    #[test]
    fn rejects_non_glb_contents_before_parsing() {
        let directory = tempdir::TempDir::new("blaulicht-invalid-glb").unwrap();
        let source = directory.path().join("bad.glb");
        fs::write(&source, b"not a glb file").unwrap();

        let result = import_glb(&source, &directory.path().join("show.json"));

        assert!(result.is_err());
        assert!(!directory.path().join("show.assets").exists());
    }

    #[test]
    fn bundled_models_load_and_helmet_textures_are_downscaled() {
        let box_model = load_prepared_model(&test_asset("Box.glb")).unwrap();
        assert!(box_model.stats.primitives > 0);

        let helmet = load_prepared_model(&test_asset("DamagedHelmet.glb")).unwrap();
        assert_eq!(helmet.stats.textures, 5);
        assert_eq!(helmet.stats.textures_downscaled, 5);
        assert_eq!(helmet.stats.primitives, 1);
        assert!(helmet.stats.texture_bytes <= MAX_TOTAL_TEXTURE_BYTES);
    }

    #[test]
    fn downsizes_ordinary_textures_and_rejects_unsafe_sources() {
        let mut oversized = Texture2D {
            width: MAX_TEXTURE_DIMENSION + 1,
            height: 1,
            data: TextureData::RU8(vec![0; MAX_TEXTURE_DIMENSION as usize + 1]),
            ..Default::default()
        };
        assert!(prepare_texture(&mut oversized).unwrap());
        assert_eq!(oversized.width, MAX_TEXTURE_DIMENSION);
        assert_eq!(texture_pixels(&oversized), MAX_TEXTURE_DIMENSION as usize);

        let unsafe_source = Texture2D {
            width: MAX_SOURCE_TEXTURE_DIMENSION + 1,
            height: 1,
            data: TextureData::RU8(vec![0]),
            ..Default::default()
        };
        assert!(validate_texture(&unsafe_source)
            .unwrap_err()
            .to_string()
            .contains("source safety limit"));

        let malformed = Texture2D {
            width: 2,
            height: 2,
            data: TextureData::RgbaU8(vec![[0; 4]; 3]),
            ..Default::default()
        };
        assert!(validate_texture(&malformed)
            .unwrap_err()
            .to_string()
            .contains("requires 4"));
    }
}
