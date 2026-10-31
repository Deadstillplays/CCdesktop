#[cfg(windows)]
mod test_win {
    use std::path::PathBuf;

    #[link(name = "user32")]
    extern "system" {
        fn OpenClipboard(hWndNewOwner: *mut std::ffi::c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(uFormat: u32) -> *mut std::ffi::c_void;
        fn IsClipboardFormatAvailable(format: u32) -> i32;
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
}

#[test]
fn test_hdrop_fn() {
    #[cfg(windows)]
    {
        let files = test_win::get_clipboard_files();
        println!("Currently on clipboard: {} files", files.len());
    }
}
