use crate::stage::StageAsset;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use three_d_asset::{Geometry, Model, Scene};

const MAX_GLB_BYTES: u64 = 100 * 1024 * 1024;
const MAX_TRIANGLES: usize = 2_000_000;

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

    let scene: Scene = three_d_asset::io::load_and_deserialize(source)
        .map_err(|err| anyhow::anyhow!("Invalid GLB: {err}"))?;
    let model = Model::from(scene);
    let triangle_count: usize = model
        .geometries
        .iter()
        .map(|primitive| match &primitive.geometry {
            Geometry::Triangles(mesh) => mesh.triangle_count(),
            Geometry::Points(_) => 0,
        })
        .sum();
    if triangle_count > MAX_TRIANGLES {
        bail!("GLB contains more than two million triangles");
    }

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
}
