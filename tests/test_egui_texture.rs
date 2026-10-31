use egui::ColorImage;

#[test]
fn test_color_image_creation() {
    let w = 32;
    let h = 32;
    let rgba = vec![255u8; w * h * 4];
    let color_image = ColorImage::from_rgba_unmultiplied([w, h], &rgba);
    assert_eq!(color_image.size, [32, 32]);
}
