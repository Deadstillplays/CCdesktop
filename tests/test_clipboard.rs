use arboard::{Clipboard, ImageData};
use claude_desktop_rust::ui::clipboard::ClipboardHelper;
use std::borrow::Cow;

#[test]
fn test_clipboard_image_roundtrip() {
    let mut cb = Clipboard::new().expect("Should create clipboard");

    // Put a dummy 16x16 red image onto clipboard
    let width = 16;
    let height = 16;
    let mut bytes = Vec::with_capacity(width * height * 4);
    for _ in 0..(width * height) {
        bytes.extend_from_slice(&[255, 0, 0, 255]); // RGBA red
    }

    let img = ImageData {
        width,
        height,
        bytes: Cow::Owned(bytes),
    };

    let set_res = cb.set_image(img);
    println!("Set image result: {:?}", set_res);
    assert!(set_res.is_ok(), "Failed to set image to clipboard");

    // Read back via ClipboardHelper
    let att = ClipboardHelper::try_paste_attachment();
    assert!(att.is_some(), "ClipboardHelper should detect the image!");
    let att = att.unwrap();
    println!("Detected attachment: {:?}", att.name);
    assert!(att.name.contains("16x16"));
}
