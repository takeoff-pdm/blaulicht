pub mod icons;

fn font_bytes() -> &'static [u8] {
    &*include_bytes!("../fonts/icons.ttf")
}

fn font_data() -> egui::FontData {
    egui::FontData::from_static(font_bytes())
}

pub fn add_to_fonts(fonts: &mut egui::FontDefinitions) {
    const FONT_KEY: &str = "blaulicht_custom";

    fonts.font_data.insert(FONT_KEY.into(), font_data().into());

    if let Some(font_keys) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        font_keys.insert(1, FONT_KEY.into());
    }
}

//
// IBM Plex (SIL Open Font License 1.1, see `fonts/plex/OFL.txt`), the desktop
// layout's type: Sans Condensed for UI text, Mono for values.
//

/// Font family of IBM Plex Sans Condensed Medium (500), registered by
/// [`add_plex_fonts`].
pub const PLEX_MEDIUM_FAMILY: &str = "plex-medium";
/// Font family of IBM Plex Sans Condensed SemiBold (600), registered by
/// [`add_plex_fonts`].
pub const PLEX_SEMIBOLD_FAMILY: &str = "plex-semibold";

/// Makes IBM Plex Sans Condensed (400) the first proportional font and IBM
/// Plex Mono (500) the first monospace font, and adds the
/// [`PLEX_MEDIUM_FAMILY`] and [`PLEX_SEMIBOLD_FAMILY`] families. The fonts
/// already in a family (egui's defaults, `egui_phosphor`, the icon font) stay
/// behind Plex as fallbacks for glyphs Plex lacks. Call it after the other
/// fonts were added, since the named families copy the proportional list.
pub fn add_plex_fonts(fonts: &mut egui::FontDefinitions) {
    const REGULAR: &str = "plex-sans-condensed-400";
    const MEDIUM: &str = "plex-sans-condensed-500";
    const SEMIBOLD: &str = "plex-sans-condensed-600";
    const MONO: &str = "plex-mono-500";

    let faces: [(&str, &'static [u8]); 4] = [
        (
            REGULAR,
            include_bytes!("../fonts/plex/IBMPlexSansCondensed-Regular.ttf"),
        ),
        (
            MEDIUM,
            include_bytes!("../fonts/plex/IBMPlexSansCondensed-Medium.ttf"),
        ),
        (
            SEMIBOLD,
            include_bytes!("../fonts/plex/IBMPlexSansCondensed-SemiBold.ttf"),
        ),
        (MONO, include_bytes!("../fonts/plex/IBMPlexMono-Medium.ttf")),
    ];
    for (key, bytes) in faces {
        fonts
            .font_data
            .insert(key.into(), egui::FontData::from_static(bytes).into());
    }

    let proportional = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    proportional.insert(0, REGULAR.into());
    let fallbacks: Vec<String> = proportional[1..].to_vec();
    for (family, face) in [
        (PLEX_MEDIUM_FAMILY, MEDIUM),
        (PLEX_SEMIBOLD_FAMILY, SEMIBOLD),
    ] {
        let mut list = vec![face.to_owned()];
        list.extend(fallbacks.iter().cloned());
        fonts
            .families
            .insert(egui::FontFamily::Name(family.into()), list);
    }
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, MONO.into());
}

//
// Images.
//

pub const LOGO_IMAGE: egui::ImageSource = egui::include_image!("../logo.svg");
/// Only the "blaulicht" line of [`LOGO_IMAGE`], cropped to it.
pub const LOGO_BLAULICHT_IMAGE: egui::ImageSource = egui::include_image!("../logo_blaulicht.svg");
