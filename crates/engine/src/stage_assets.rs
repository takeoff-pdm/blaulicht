//! Model files of the stage: installing imported models next to the
//! showfile, resolving and copying them. Loading and preparing a model for
//! rendering is frontend work (`stage_assets` in the egui crate).

use crate::stage::StageAsset;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Largest model file accepted for import.
pub const MAX_GLB_BYTES: u64 = 100 * 1024 * 1024;

/// Checks that `file_name` names a `.glb` model.
pub fn check_glb_name(file_name: &str) -> Result<()> {
    if Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        != Some("glb".to_string())
    {
        bail!("Only .glb models are supported");
    }
    Ok(())
}

/// Checks the size limit and the binary glTF header of model `bytes`.
pub fn check_glb_bytes(bytes: &[u8]) -> Result<()> {
    if bytes.len() as u64 > MAX_GLB_BYTES {
        bail!("GLB exceeds the 100 MB import limit");
    }
    if bytes.len() < 12 || &bytes[..4] != b"glTF" {
        bail!("File is not a binary glTF model");
    }
    Ok(())
}

/// Stores model `bytes` (originally named `file_name`) next to `showfile`, in
/// `<showfile stem>.assets/<content hash>.glb` (kept if it is already
/// there), and returns the asset that points at it. Checks the name, size
/// and glTF header only; the frontend parses the model before it sends it
/// (`read_glb_for_import` in the egui crate) and again when it loads it.
pub fn install_glb(file_name: &str, bytes: &[u8], showfile: &Path) -> Result<StageAsset> {
    check_glb_name(file_name)?;
    check_glb_bytes(bytes)?;

    let hash = format!("{:x}", Sha256::digest(bytes));
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
    let stored_name = format!("{hash}.glb");
    let destination = asset_dir.join(&stored_name);
    if !destination.exists() {
        crate::config::write_atomic(&destination, bytes)
            .with_context(|| format!("Cannot install imported model {destination:?}"))?;
    }

    Ok(StageAsset {
        original_name: file_name.to_string(),
        relative_path: PathBuf::from(folder_name)
            .join(stored_name)
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
    fn rejects_non_glb_contents() {
        let directory = tempdir::TempDir::new("blaulicht-invalid-glb").unwrap();
        let result = install_glb(
            "bad.glb",
            b"not a glb file",
            &directory.path().join("show.json"),
        );

        assert!(result.is_err());
        assert!(!directory.path().join("show.assets").exists());
    }

    #[test]
    fn an_installed_model_lands_next_to_the_showfile_once() {
        let directory = tempdir::TempDir::new("blaulicht-install-glb").unwrap();
        let showfile = directory.path().join("show.json");
        let bytes = fs::read(test_asset("Box.glb")).unwrap();

        let asset = install_glb("Box.glb", &bytes, &showfile).unwrap();
        let again = install_glb("Box.glb", &bytes, &showfile).unwrap();

        assert_eq!(asset, again);
        assert_eq!(asset.original_name, "Box.glb");
        assert!(asset.relative_path.starts_with("show.assets"));
        assert_eq!(
            fs::read(resolve_asset(&showfile, &asset).unwrap()).unwrap(),
            bytes
        );
        assert!(install_glb("box.obj", &bytes, &showfile).is_err());
    }
}
