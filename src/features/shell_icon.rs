//! The real Windows shell icon for a file (its .exe/.lnk/.url/.ico art), as RGBA for a Spotlight row.
//! Local only: it asks the shell for the file's icon and copies the pixels out.

use std::ffi::c_void;
use std::path::Path;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, HGDIOBJ, ReleaseDC,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::{HSTRING, PCWSTR};

/// `(width, height, rgba)` of the file's icon, or `None` when the shell has no icon for it.
pub fn icon_rgba(path: &Path) -> Option<([usize; 2], Vec<u8>)> {
    unsafe {
        let wide = HSTRING::from(path.as_os_str());
        let mut info = SHFILEINFOW::default();
        let r =
            SHGetFileInfoW(PCWSTR(wide.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(0), Some(&mut info), size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON);
        if r == 0 || info.hIcon.is_invalid() {
            return None;
        }
        let out = hicon_rgba(info.hIcon);
        let _ = DestroyIcon(info.hIcon);
        out
    }
}

/// Copies an HICON's colour bitmap into RGBA. When the icon carries no alpha channel (older, mask-based icons),
/// its 1-bpp AND mask decides which pixels are transparent.
unsafe fn hicon_rgba(hicon: HICON) -> Option<([usize; 2], Vec<u8>)> {
    unsafe {
        let mut ii = ICONINFO::default();
        GetIconInfo(hicon, &mut ii).ok()?;
        let mut bm = BITMAP::default();
        GetObjectW(HGDIOBJ(ii.hbmColor.0), size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut c_void));
        let (w, h) = (bm.bmWidth.max(0) as usize, bm.bmHeight.max(0) as usize);
        let dc = GetDC(None);
        let mut color = read_32bpp(dc, ii.hbmColor, w, h);
        let has_alpha = color.chunks_exact(4).any(|p| p[3] != 0);
        if !has_alpha {
            let mask = read_32bpp(dc, ii.hbmMask, w, h);
            for (px, m) in color.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
                px[3] = if m[0] != 0 { 0 } else { 255 }; // AND-mask bit set = transparent
            }
        }
        ReleaseDC(None, dc);
        let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
        let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
        for px in color.chunks_exact_mut(4) {
            px.swap(0, 2); // GetDIBits gives BGRA; egui wants RGBA
        }
        (w > 0 && h > 0).then_some(([w, h], color))
    }
}

/// Reads a GDI bitmap into a top-down 32-bpp BGRA buffer, zero-filled on failure.
unsafe fn read_32bpp(dc: windows::Win32::Graphics::Gdi::HDC, hbm: windows::Win32::Graphics::Gdi::HBITMAP, w: usize, h: usize) -> Vec<u8> {
    unsafe {
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // negative = top-down, so row 0 is the top
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; w * h * 4];
        GetDIBits(dc, hbm, 0, h as u32, Some(buf.as_mut_ptr() as *mut c_void), &mut bi, DIB_RGB_COLORS);
        buf
    }
}
