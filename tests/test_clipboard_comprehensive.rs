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
