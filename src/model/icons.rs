//! Extracts a browser's executable icon to a cached PNG the UI can show.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW,
    HGDIOBJ, ReleaseDC,
};
use windows::Win32::UI::Shell::SHDefExtractIconW;
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::HSTRING;

const SIZE: u32 = 48;

pub fn cache_dir() -> PathBuf {
    super::local_dir().join("icons")
}

/// Path of the cached PNG for `exe`, extracting it on first use. The cache
/// key includes the file's modification time so updates refresh the icon.
pub fn ensure(exe: &Path) -> Result<PathBuf> {
    let modified = std::fs::metadata(exe)?.modified().ok();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    exe.to_string_lossy().to_lowercase().hash(&mut h);
    modified.hash(&mut h);
    let path = cache_dir().join(format!("{:016x}.png", h.finish()));
    if path.exists() {
        return Ok(path);
    }
    let rgba = extract(exe)?;
    std::fs::create_dir_all(cache_dir())?;
    let tmp = path.with_extension("tmp");
    image::save_buffer_with_format(&tmp, &rgba.2, rgba.0, rgba.1, image::ExtendedColorType::Rgba8, image::ImageFormat::Png)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// (width, height, RGBA pixels) of the exe's first icon.
fn extract(exe: &Path) -> Result<(u32, u32, Vec<u8>)> {
    unsafe {
        let mut icon = HICON::default();
        SHDefExtractIconW(&HSTRING::from(exe.as_os_str()), 0, 0, Some(&mut icon), None, SIZE)
            .ok()
            .map_err(|e| anyhow!("no icon in {}: {e}", exe.display()))?;
        let result = icon_to_rgba(icon);
        let _ = DestroyIcon(icon);
        result
    }
}

unsafe fn icon_to_rgba(icon: HICON) -> Result<(u32, u32, Vec<u8>)> {
    unsafe {
        let mut info = ICONINFO::default();
        GetIconInfo(icon, &mut info)?;
        let color = info.hbmColor;
        let mask = info.hbmMask;
        let cleanup = || {
            let _ = DeleteObject(HGDIOBJ(color.0));
            let _ = DeleteObject(HGDIOBJ(mask.0));
        };
        if color.is_invalid() {
            cleanup();
            return Err(anyhow!("monochrome icon"));
        }
        let mut bm = BITMAP::default();
        GetObjectW(HGDIOBJ(color.0), size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
        let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let lines = GetDIBits(dc, color, 0, h, Some(px.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        cleanup();
        if lines == 0 {
            return Err(anyhow!("GetDIBits failed"));
        }
        let has_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
        for p in px.chunks_exact_mut(4) {
            p.swap(0, 2); // BGRA -> RGBA
            if !has_alpha {
                p[3] = 255;
            }
        }
        Ok((w, h, px))
    }
}
