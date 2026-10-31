use claude_desktop_rust::ui::clipboard::ClipboardHelper;

#[test]
fn test_clipboard_text_roundtrip() {
    let sample = "Antigravity Clipboard Test 456";
    ClipboardHelper::set_text(sample);

    let read_back = ClipboardHelper::get_text();
    println!("Read back from clipboard: {:?}", read_back);
    assert_eq!(read_back.as_deref(), Some(sample));

    let mut input_text = String::new();
    let mut attachments = Vec::new();
    let pasted = ClipboardHelper::paste_any(&mut input_text, &mut attachments);
    assert!(pasted);
    assert_eq!(input_text, sample);
}
