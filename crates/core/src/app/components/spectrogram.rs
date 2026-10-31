use blaulicht_audio_engine::SpectrogramImage;
use egui::{Color32, ColorImage};

/// Converts the engine's plain RGBA spectrogram into an egui texture image.
/// The spectrogram draws only opaque pixels, so straight and premultiplied
/// alpha are the same bytes.
pub fn spectrogram_color_image(image: &SpectrogramImage) -> ColorImage {
    let pixels = image
        .pixels
        .iter()
        .map(|&[r, g, b, a]| Color32::from_rgba_premultiplied(r, g, b, a))
        .collect();
    ColorImage::new(image.size(), pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_keeps_size_and_opaque_colours() {
        let mut image = SpectrogramImage::new(3, 2);
        image.pixels[4] = blaulicht_audio_engine::spectrogram_color(200);
        let converted = spectrogram_color_image(&image);
        assert_eq!(converted.size, [3, 2]);
        assert_eq!(converted.pixels[0], Color32::BLACK);
        let [r, g, b, _] = image.pixels[4];
        assert_eq!(converted.pixels[4], Color32::from_rgb(r, g, b));
    }
}
