use claude_desktop_rust::ui::clipboard::ClipboardHelper;
use claude_desktop_rust::storage::AttachmentKind;
use base64::Engine;

#[test]
fn test_clipboard_decode() {
    let atts = ClipboardHelper::try_paste_attachments();
    println!("Attachments: {}", atts.len());
    for att in &atts {
        println!("Name: {}", att.name);
        if let AttachmentKind::Image { base64_data, width, height, .. } = &att.kind {
            println!("Image dims: {}x{}, base64 len: {}", width, height, base64_data.len());
            let decoded = base64::engine::general_purpose::STANDARD.decode(base64_data).unwrap();
            let dyn_img = image::load_from_memory(&decoded).unwrap();
            println!("Decoded dyn_img dims: {}x{}", dyn_img.width(), dyn_img.height());
            let rgba = dyn_img.to_rgba8();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            );
            println!("ColorImage size: {:?}", color_image.size);
        }
    }
}

#[test]
fn test_zero_alpha_repair() {
    let width = 32;
    let height = 32;
    // Simulate Windows GDI 32-bit screenshot with 0 alpha
    let mut rgba_with_zero_alpha = vec![0u8; width * height * 4];
    for chunk in rgba_with_zero_alpha.chunks_exact_mut(4) {
        chunk[0] = 120; // R
        chunk[1] = 200; // G
        chunk[2] = 255; // B
        chunk[3] = 0;   // Alpha is 0!
    }

    let all_zero_alpha = rgba_with_zero_alpha.chunks_exact(4).all(|p| p[3] == 0);
    assert!(all_zero_alpha, "Should detect zero alpha");

    if all_zero_alpha {
        for p in rgba_with_zero_alpha.chunks_exact_mut(4) {
            p[3] = 255;
        }
    }

    assert!(rgba_with_zero_alpha.chunks_exact(4).all(|p| p[3] == 255), "Alpha should be repaired to 255");
}
