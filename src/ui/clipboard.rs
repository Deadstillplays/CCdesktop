use crate::storage::{AttachmentKind, ChatAttachment};
use std::path::Path;
use uuid::Uuid;

#[cfg(windows)]
mod win_clipboard {
    use std::path::PathBuf;

    #[link(name = "user32")]
    extern "system" {
        fn OpenClipboard(hWndNewOwner: *mut std::ffi::c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(uFormat: u32) -> *mut std::ffi::c_void;
        fn IsClipboardFormatAvailable(format: u32) -> i32;
        fn RegisterClipboardFormatW(lpszFormat: *const u16) -> u32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(uFormat: u32, hMem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalLock(hMem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn GlobalUnlock(hMem: *mut std::ffi::c_void) -> i32;
        fn GlobalSize(hMem: *mut std::ffi::c_void) -> usize;
        fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> *mut std::ffi::c_void;
    }

    #[link(name = "shell32")]
    extern "system" {
        fn DragQueryFileW(
            hDrop: *mut std::ffi::c_void,
            iFile: u32,
            lpszFile: *mut u16,
            cch: u32,
        ) -> u32;
    }

    /// Read raw PNG bytes if Chrome, Discord, Snipping Tool, etc. registered "PNG" clipboard format
    pub fn get_raw_png() -> Option<Vec<u8>> {
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return None;
            }

            // Register format for "PNG"
            let format_name: Vec<u16> = "PNG\0".encode_utf16().collect();
            let png_fmt = RegisterClipboardFormatW(format_name.as_ptr());
            if png_fmt != 0 && IsClipboardFormatAvailable(png_fmt) != 0 {
                let hdata = GetClipboardData(png_fmt);
                if !hdata.is_null() {
                    let ptr = GlobalLock(hdata);
                    let size = GlobalSize(hdata);
                    if !ptr.is_null() && size > 8 {
                        let slice = std::slice::from_raw_parts(ptr as *const u8, size);
                        // Check PNG magic number \x89PNG\r\n\x1a\n
                        if slice.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
                            let bytes = slice.to_vec();
                            GlobalUnlock(hdata);
                            CloseClipboard();
                            return Some(bytes);
                        }
                    }
                    if !ptr.is_null() {
                        GlobalUnlock(hdata);
                    }
                }
            }

            CloseClipboard();
        }
        None
    }

    /// Read file paths if user copied files from Windows Explorer (CF_HDROP)
    pub fn get_clipboard_files() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return paths;
            }
            const CF_HDROP: u32 = 15;
            if IsClipboardFormatAvailable(CF_HDROP) != 0 {
                let hdrop = GetClipboardData(CF_HDROP);
                if !hdrop.is_null() {
                    let count = DragQueryFileW(hdrop, 0xFFFFFFFF, std::ptr::null_mut(), 0);
                    for i in 0..count {
                        let mut buffer = [0u16; 1024];
                        let len = DragQueryFileW(hdrop, i, buffer.as_mut_ptr(), buffer.len() as u32);
                        if len > 0 {
                            let path_str = String::from_utf16_lossy(&buffer[..len as usize]);
                            paths.push(PathBuf::from(path_str));
                        }
                    }
                }
            }
            CloseClipboard();
        }
        paths
    }

    /// Read native Windows CF_DIB / CF_DIBV5 (bitmaps / screenshots / copied images) directly as BMP bytes
    pub fn get_clipboard_dib_image() -> Option<Vec<u8>> {
        unsafe {
            let mut opened = false;
            for _ in 0..5 {
                if OpenClipboard(std::ptr::null_mut()) != 0 {
                    opened = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if !opened {
                return None;
            }

            const CF_DIB: u32 = 8;
            const CF_DIBV5: u32 = 17;
            let format_to_use = if IsClipboardFormatAvailable(CF_DIB) != 0 {
                Some(CF_DIB)
            } else if IsClipboardFormatAvailable(CF_DIBV5) != 0 {
                Some(CF_DIBV5)
            } else {
                None
            };

            if let Some(fmt) = format_to_use {
                let hdata = GetClipboardData(fmt);
                if !hdata.is_null() {
                    let size = GlobalSize(hdata);
                    if size > 40 {
                        let ptr = GlobalLock(hdata) as *const u8;
                        if !ptr.is_null() {
                            let dib_slice = std::slice::from_raw_parts(ptr, size);

                            let bi_size = u32::from_le_bytes(dib_slice[0..4].try_into().unwrap_or([40, 0, 0, 0]));
                            let bi_bit_count = u16::from_le_bytes(dib_slice[14..16].try_into().unwrap_or([24, 0]));
                            let bi_compression = u32::from_le_bytes(dib_slice[16..20].try_into().unwrap_or([0, 0, 0, 0]));
                            let bi_clr_used = u32::from_le_bytes(dib_slice[32..36].try_into().unwrap_or([0, 0, 0, 0]));

                            let palette_size = if bi_clr_used > 0 {
                                bi_clr_used * 4
                            } else if bi_bit_count <= 8 {
                                let count = if bi_clr_used > 0 { bi_clr_used } else { 1u32 << bi_bit_count };
                                count * 4
                            } else if (bi_compression == 3 || bi_compression == 6) && bi_size == 40 {
                                12
                            } else {
                                0
                            };

                            let off_bits = 14 + bi_size + palette_size;
                            let total_size = 14 + size as u32;

                            let mut bmp = Vec::with_capacity(total_size as usize);
                            bmp.push(b'B');
                            bmp.push(b'M');
                            bmp.extend_from_slice(&total_size.to_le_bytes());
                            bmp.extend_from_slice(&[0u8; 4]);
                            bmp.extend_from_slice(&off_bits.to_le_bytes());
                            bmp.extend_from_slice(dib_slice);

                            GlobalUnlock(hdata);
                            CloseClipboard();
                            return Some(bmp);
                        }
                    }
                    GlobalUnlock(hdata);
                }
            }
            CloseClipboard();
        }
        None
    }

    /// Read raw Unicode text directly from Windows clipboard
    pub fn get_clipboard_text() -> Option<String> {
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return None;
            }
            const CF_UNICODETEXT: u32 = 13;
            if IsClipboardFormatAvailable(CF_UNICODETEXT) != 0 {
                let hdata = GetClipboardData(CF_UNICODETEXT);
                if !hdata.is_null() {
                    let ptr = GlobalLock(hdata) as *const u16;
                    if !ptr.is_null() {
                        let mut len = 0;
                        while *ptr.add(len) != 0 {
                            len += 1;
                        }
                        let slice = std::slice::from_raw_parts(ptr, len);
                        let s = String::from_utf16_lossy(slice);
                        GlobalUnlock(hdata);
                        CloseClipboard();
                        return Some(s);
                    }
                }
            }
            CloseClipboard();
        }
        None
    }

    /// Write raw Unicode text directly to Windows clipboard
    pub fn set_clipboard_text(text: &str) -> bool {
        let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes_len = utf16.len() * 2;
        const GMEM_MOVEABLE: u32 = 0x0002;
        const CF_UNICODETEXT: u32 = 13;

        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return false;
            }
            EmptyClipboard();
            let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes_len);
            if !hmem.is_null() {
                let ptr = GlobalLock(hmem) as *mut u16;
                if !ptr.is_null() {
                    std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
                    GlobalUnlock(hmem);
                    SetClipboardData(CF_UNICODETEXT, hmem);
                    CloseClipboard();
                    return true;
                }
            }
            CloseClipboard();
        }
        false
    }
}

pub struct ClipboardHelper;

impl ClipboardHelper {
    /// Read text directly from clipboard (Windows CF_UNICODETEXT or arboard)
    pub fn get_text() -> Option<String> {
        #[cfg(windows)]
        {
            if let Some(t) = win_clipboard::get_clipboard_text() {
                if !t.is_empty() {
                    return Some(t);
                }
            }
        }
        if let Ok(mut cb) = arboard::Clipboard::new() {
            if let Ok(t) = cb.get_text() {
                if !t.is_empty() {
                    return Some(t);
                }
            }
        }
        None
    }

    /// Set text directly to clipboard (Windows CF_UNICODETEXT and arboard)
    pub fn set_text(text: &str) {
        #[cfg(windows)]
        {
            let _ = win_clipboard::set_clipboard_text(text);
        }
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(text);
        }
    }

    /// Universal paste:
    /// - If image / files / valid path: adds to attachments
    /// - If text: appends directly to input_text
    /// Returns true if anything was pasted
    pub fn paste_any(input_text: &mut String, attachments: &mut Vec<ChatAttachment>) -> bool {
        let atts = Self::try_paste_attachments();
        if !atts.is_empty() {
            for att in atts {
                attachments.push(att);
            }
            return true;
        }

        if let Some(text) = Self::get_text() {
            if !text.is_empty() {
                if !input_text.is_empty() && !input_text.ends_with(' ') && !input_text.ends_with('\n') {
                    input_text.push(' ');
                }
                input_text.push_str(&text);
                return true;
            }
        }

        false
    }

    /// Single attachment convenience method
    #[allow(dead_code)]
    pub fn try_paste_attachment() -> Option<ChatAttachment> {
        Self::try_paste_attachments().into_iter().next()
    }

    /// Comprehensive clipboard paste: handles raw PNG, CF_HDROP (Explorer files), CF_DIB (screenshots), and paths
    pub fn try_paste_attachments() -> Vec<ChatAttachment> {
        use base64::Engine;
        let mut results = Vec::new();

        // 1. Check Windows CF_HDROP (files copied in Windows Explorer)
        #[cfg(windows)]
        {
            let files = win_clipboard::get_clipboard_files();
            for f in files {
                if let Some(att) = crate::project::inspect_any_path_standalone(&f.to_string_lossy()) {
                    results.push(att);
                }
            }
            if !results.is_empty() {
                return results;
            }
        }

        // 2. Check native Windows CF_DIB / Bitmap (screenshots, Snipping Tool, Chrome/Edge copied images)
        #[cfg(windows)]
        {
            if let Some(bmp_bytes) = win_clipboard::get_clipboard_dib_image() {
                if let Ok(dyn_img) = image::load_from_memory(&bmp_bytes) {
                    let (w, h) = (dyn_img.width(), dyn_img.height());
                    let mut png_bytes = Vec::new();
                    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
                    let rgba = dyn_img.to_rgba8();
                    if image::ImageEncoder::write_image(
                        encoder,
                        rgba.as_raw(),
                        w,
                        h,
                        image::ExtendedColorType::Rgba8,
                    )
                    .is_ok()
                    {
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
                        results.push(ChatAttachment {
                            id: Uuid::new_v4().to_string(),
                            name: format!("Pasted Image ({}x{})", w, h),
                            kind: AttachmentKind::Image {
                                mime_type: "image/png".to_string(),
                                base64_data: b64,
                                width: w,
                                height: h,
                            },
                        });
                        return results;
                    }
                }
            }
        }

        // 3. Check Raw "PNG" clipboard format (Chrome, Discord, Snipping Tool)
        #[cfg(windows)]
        {
            if let Some(png_bytes) = win_clipboard::get_raw_png() {
                if let Ok(dyn_img) = image::load_from_memory(&png_bytes) {
                    let (w, h) = (dyn_img.width(), dyn_img.height());
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
                    results.push(ChatAttachment {
                        id: Uuid::new_v4().to_string(),
                        name: format!("Pasted PNG ({}x{})", w, h),
                        kind: AttachmentKind::Image {
                            mime_type: "image/png".to_string(),
                            base64_data: b64,
                            width: w,
                            height: h,
                        },
                    });
                    return results;
                }
            }
        }

        // 4. Check if clipboard text contains an existing file/directory path
        if let Some(text) = Self::get_text() {
            let trimmed = text.trim().trim_matches('"').trim();
            let expanded = crate::mcp::client::expand_vars(trimmed);
            let p = Path::new(&expanded);
            if p.exists() {
                if let Some(att) = crate::project::inspect_any_path_standalone(&expanded) {
                    results.push(att);
                    return results;
                }
            }
        }

        // 5. Fallback to arboard (non-Windows or secondary)
        if let Ok(mut cb) = arboard::Clipboard::new() {
            if let Ok(img_data) = cb.get_image() {
                let width = img_data.width as u32;
                let height = img_data.height as u32;
                let rgba_raw = img_data.bytes.into_owned();

                if let Some(rgba_img) = image::RgbaImage::from_raw(width, height, rgba_raw) {
                    let mut png_bytes = Vec::new();
                    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
                    if image::ImageEncoder::write_image(
                        encoder,
                        rgba_img.as_raw(),
                        width,
                        height,
                        image::ExtendedColorType::Rgba8,
                    )
                    .is_ok()
                    {
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
                        results.push(ChatAttachment {
                            id: Uuid::new_v4().to_string(),
                            name: format!("Pasted Image ({}x{})", width, height),
                            kind: AttachmentKind::Image {
                                mime_type: "image/png".to_string(),
                                base64_data: b64,
                                width,
                                height,
                            },
                        });
                        return results;
                    }
                }
            }
        }

        results
    }

    /// Extract a Claude sessionKey from raw text, cookies, JSON, or key string
    pub fn extract_session_key(text: &str) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.contains("sk-ant-sid") {
            for part in trimmed.split(|c: char| c == ';' || c == ' ' || c == '\n' || c == '\r' || c == '"' || c == '\'' || c == '\t') {
                let p = part.trim();
                if let Some(key) = p.strip_prefix("sessionKey=") {
                    let clean = key.trim();
                    if clean.starts_with("sk-ant-sid") {
                        return Some(clean.to_string());
                    }
                } else if p.starts_with("sk-ant-sid") {
                    return Some(p.to_string());
                }
            }
        }
        None
    }
}
