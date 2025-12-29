use std::ffi::c_void;
use std::ptr;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

pub fn crop_buffer(src: &[u8], src_w: u32, src_h: u32, region: Region) -> Option<Vec<u8>> {
    // Basic bounds check
    if region.x + region.width > src_w || region.y + region.height > src_h {
        return None;
    }

    let bpp = 4; // BGRA
    let row_bytes = (src_w * bpp) as usize;
    let crop_row_bytes = (region.width * bpp) as usize;
    let mut dest = Vec::with_capacity((region.width * region.height * bpp) as usize);

    let start_offset = ((region.y * src_w + region.x) * bpp) as usize;

    for i in 0..region.height {
        let src_idx = start_offset + (i as usize * row_bytes);
        dest.extend_from_slice(&src[src_idx..src_idx + crop_row_bytes]);
    }

    Some(dest)
}

pub struct ScreenCapturer {
    hwnd: HWND,
    width: i32,
    height: i32,
    memory_dc: HDC,
    bitmap: HBITMAP,
    original_bitmap: HBITMAP,
}

impl ScreenCapturer {
    pub fn new() -> Self {
        unsafe {
            let hwnd = GetDesktopWindow();
            let hdc = GetDC(hwnd);
            let memory_dc = CreateCompatibleDC(hdc);

            // Initial placeholders
            ReleaseDC(hwnd, hdc);

            Self {
                hwnd,
                width: 0,
                height: 0,
                memory_dc,
                bitmap: HBITMAP(ptr::null_mut()),
                original_bitmap: HBITMAP(ptr::null_mut()),
            }
        }
    }

    pub fn capture_region(&mut self, x: i32, y: i32, w: i32, h: i32) -> Result<Vec<u8>, String> {
        if w <= 0 || h <= 0 {
            return Err("Invalid dimensions".to_string());
        }

        unsafe {
            let screen_dc = GetDC(self.hwnd);

            // Recreate bitmap if size changes
            if w != self.width || h != self.height {
                if !self.bitmap.is_invalid() {
                    SelectObject(self.memory_dc, self.original_bitmap);
                    DeleteObject(self.bitmap);
                }

                self.width = w;
                self.height = h;
                self.bitmap = CreateCompatibleBitmap(screen_dc, w, h);
                self.original_bitmap = HBITMAP(SelectObject(self.memory_dc, self.bitmap).0 as _);
            }

            // BitBlt: The heart of speed
            let success = BitBlt(self.memory_dc, 0, 0, w, h, screen_dc, x, y, SRCCOPY);

            ReleaseDC(self.hwnd, screen_dc);

            if success.is_err() {
                return Err("BitBlt failed".to_string());
            }

            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // Top-down
                    biPlanes: 1,
                    biBitCount: 32,          // BGRA
                    biCompression: BI_RGB.0, // Extract u32 value
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut pixels = vec![0u8; (w * h * 4) as usize];

            let lines = GetDIBits(
                self.memory_dc,
                self.bitmap,
                0,
                h as u32,
                Some(pixels.as_mut_ptr() as *mut c_void),
                &mut bmi,
                DIB_RGB_COLORS,
            );

            if lines == 0 {
                return Err("GetDIBits failed".to_string());
            }

            // Pixels are now in BGRA format
            Ok(pixels)
        }
    }
}

impl Drop for ScreenCapturer {
    fn drop(&mut self) {
        unsafe {
            if !self.bitmap.is_invalid() {
                SelectObject(self.memory_dc, self.original_bitmap);
                DeleteObject(self.bitmap);
            }
            DeleteDC(self.memory_dc);
        }
    }
}
