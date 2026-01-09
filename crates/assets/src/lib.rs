pub mod icons;

fn font_bytes() -> &'static [u8] {
    &*include_bytes!("../dist/icons.ttf")
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
// Images.
//

pub const LOGO_IMAGE: egui::ImageSource = egui::include_image!("../logo.svg");
