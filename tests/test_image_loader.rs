use base64::Engine;
use egui::ColorImage;

#[test]
fn test_base64_to_color_image() {
    // 2x2 red png in base64
    let dummy_rgba = vec![
        255, 0, 0, 255,  0, 255, 0, 255,
        0, 0, 255, 255,  255, 255, 0, 255,
    ];
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
    image::ImageEncoder::write_image(
        encoder,
        &dummy_rgba,
        2,
        2,
        image::ExtendedColorType::Rgba8,
    ).unwrap();

    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    // Decode back
    let decoded = base64::engine::general_purpose::STANDARD.decode(&b64).unwrap();
    let dyn_img = image::load_from_memory(&decoded).unwrap();
    let rgba = dyn_img.to_rgba8();
    let color_img = ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    );

    assert_eq!(color_img.size, [2, 2]);
}
