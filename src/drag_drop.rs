//! OLE drag source: let the displayed image be dragged into other applications.
//!
//! The data object comes from the shell, so it carries CF_HDROP and everything else
//! Explorer offers, which is what most drop targets expect from a file drag.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Once;

use windows::core::{implement, Result, GUID, HRESULT, PCWSTR};
use windows::Win32::Foundation::{
    BOOL, COLORREF, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, HANDLE,
    POINT, S_OK, SIZE,
};
use windows::Win32::Graphics::Gdi::{
    CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::System::Com::{CoCreateInstance, IDataObject, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, OleInitialize, DROPEFFECT, DROPEFFECT_COPY,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::{
    IDragSourceHelper, IShellItem, IShellItemArray, SHCreateItemFromParsingName,
    SHCreateShellItemArrayFromShellItem, BHID_DataObject, CLSID_DragDropHelper, SHDRAGIMAGE,
};

/// Longest edge of the thumbnail shown under the cursor while dragging.
const DRAG_IMAGE_SIZE: u32 = 192;

/// CLR_NONE: the drag image uses its own per-pixel alpha, not a color key.
const CLR_NONE: COLORREF = COLORREF(0xFFFF_FFFF);

/// Drag `path` into another application. Blocks in the modal OLE drag loop until the
/// user drops or cancels; returns true if the file was actually dropped somewhere.
///
/// Copy is the only offered effect, so a drop target can never move the original out
/// of the directory being watched.
pub fn drag_file(path: &Path) -> Result<bool> {
    ensure_ole_initialized();

    let data_object = shell_data_object(path)?;
    attach_drag_image(&data_object, path);

    let drop_source: IDropSource = DropSource.into();
    let mut effect = DROPEFFECT::default();
    let hr = unsafe {
        DoDragDrop(
            &data_object,
            &drop_source,
            DROPEFFECT_COPY,
            &mut effect as *mut DROPEFFECT,
        )
    };
    Ok(hr == DRAGDROP_S_DROP)
}

/// winit already puts the event loop thread into an OLE apartment, but initializing is
/// refcounted, so claiming our own reference costs nothing and keeps us independent of it.
fn ensure_ole_initialized() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if let Err(e) = unsafe { OleInitialize(None) } {
            log::warn!("OleInitialize failed: {}", e);
        }
    });
}

/// Ask the shell for a data object describing `path`, the same one Explorer would drag.
fn shell_data_object(path: &Path) -> Result<IDataObject> {
    // SHCreateItemFromParsingName needs an absolute path, and chokes on the \\?\ prefix
    let canonical = path.canonicalize().map_err(windows::core::Error::from)?;
    let display = canonical.to_string_lossy();
    let clean = display.strip_prefix("\\\\?\\").unwrap_or(&display);
    let wide: Vec<u16> = clean.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
        let items: IShellItemArray = SHCreateShellItemArrayFromShellItem(&item)?;
        items.BindToHandler(None, &BHID_DataObject as *const GUID)
    }
}

/// Show the image itself under the cursor while dragging. Purely cosmetic: on any
/// failure the drag still runs with the plain copy cursor.
fn attach_drag_image(data_object: &IDataObject, path: &Path) {
    let Some((bitmap, width, height)) = render_drag_bitmap(path) else {
        return;
    };

    let result = unsafe {
        let helper: IDragSourceHelper =
            match CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER) {
                Ok(helper) => helper,
                Err(e) => {
                    log::warn!("DragDropHelper unavailable: {}", e);
                    let _ = DeleteObject(bitmap);
                    return;
                }
            };
        let image = SHDRAGIMAGE {
            sizeDragImage: SIZE {
                cx: width,
                cy: height,
            },
            // Hold the thumbnail by its center so it tracks the cursor naturally
            ptOffset: POINT {
                x: width / 2,
                y: height / 2,
            },
            hbmpDragImage: bitmap,
            crColorKey: CLR_NONE,
        };
        helper.InitializeFromBitmap(&image, data_object)
    };

    // The helper owns the bitmap only once it has accepted it
    if let Err(e) = result {
        log::warn!("Failed to set drag image: {}", e);
        unsafe {
            let _ = DeleteObject(bitmap);
        }
    }
}

/// Decode `path` into a top-down 32-bit DIB with premultiplied alpha, which is what
/// the drag-image helper expects. Returns the bitmap and its size.
fn render_drag_bitmap(path: &Path) -> Option<(HBITMAP, i32, i32)> {
    let image = image::open(path).ok()?;
    // thumbnail() scales up as well as down, so only shrink when the image is oversized
    let thumbnail = if image.width() > DRAG_IMAGE_SIZE || image.height() > DRAG_IMAGE_SIZE {
        image.thumbnail(DRAG_IMAGE_SIZE, DRAG_IMAGE_SIZE).to_rgba8()
    } else {
        image.to_rgba8()
    };
    let width = i32::try_from(thumbnail.width()).ok()?;
    let height = i32::try_from(thumbnail.height()).ok()?;
    if width <= 0 || height <= 0 {
        return None;
    }

    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative height means top-down, matching the row order of the decoded image
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut c_void = std::ptr::null_mut();
    let bitmap = unsafe {
        CreateDIBSection(
            HDC::default(),
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            HANDLE::default(),
            0,
        )
        .ok()?
    };
    if bits.is_null() {
        unsafe {
            let _ = DeleteObject(bitmap);
        }
        return None;
    }

    let len = (width as usize) * (height as usize) * 4;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, len) };
    for (dest, src) in pixels.chunks_exact_mut(4).zip(thumbnail.pixels()) {
        let [r, g, b, a] = src.0;
        let premultiply = |c: u8| ((c as u32 * a as u32 + 127) / 255) as u8;
        dest[0] = premultiply(b);
        dest[1] = premultiply(g);
        dest[2] = premultiply(r);
        dest[3] = a;
    }

    Some((bitmap, width, height))
}

/// Copy-only drop source: cancel on Escape, drop when the button comes up.
#[implement(IDropSource)]
struct DropSource;

#[allow(non_snake_case)]
impl IDropSource_Impl for DropSource {
    fn QueryContinueDrag(&self, escape_pressed: BOOL, key_state: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape_pressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if key_state & MK_LBUTTON == MODIFIERKEYS_FLAGS(0) {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::FORMATETC;

    /// CF_HDROP: the format Explorer and most applications read a dropped file from.
    const CF_HDROP: u16 = 15;
    const DVASPECT_CONTENT: u32 = 1;
    const TYMED_HGLOBAL: u32 = 1;

    fn sample_image(name: &str, width: u32, height: u32) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        image::RgbaImage::from_pixel(width, height, image::Rgba([200, 40, 40, 255]))
            .save(&path)
            .expect("write sample image");
        path
    }

    /// The whole drag hinges on the shell handing back a data object that offers
    /// CF_HDROP; without it nothing would accept the drop.
    #[test]
    fn shell_data_object_offers_the_file_as_cf_hdrop() {
        ensure_ole_initialized();
        let path = sample_image("f2viewer_drag_hdrop.png", 8, 6);

        let data_object = shell_data_object(&path).expect("shell data object");
        let format = FORMATETC {
            cfFormat: CF_HDROP,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT,
            lindex: -1,
            tymed: TYMED_HGLOBAL,
        };
        unsafe { data_object.QueryGetData(&format) }
            .ok()
            .expect("data object should offer CF_HDROP");

        // Attaching the preview must succeed too, and must not double-free the bitmap
        attach_drag_image(&data_object, &path);

        let _ = std::fs::remove_file(&path);
    }

    /// The preview must shrink oversized images but never blow up small ones.
    #[test]
    fn drag_bitmap_only_scales_down() {
        let small = sample_image("f2viewer_drag_small.png", 8, 6);
        let large = sample_image("f2viewer_drag_large.png", 800, 400);

        let (bitmap, width, height) = render_drag_bitmap(&small).expect("drag bitmap");
        assert_eq!((width, height), (8, 6), "small images keep their size");
        unsafe {
            assert!(DeleteObject(bitmap).as_bool(), "bitmap should be valid");
        }

        let (bitmap, width, height) = render_drag_bitmap(&large).expect("drag bitmap");
        assert_eq!(
            (width, height),
            (DRAG_IMAGE_SIZE as i32, DRAG_IMAGE_SIZE as i32 / 2),
            "large images are capped, keeping their aspect ratio"
        );
        unsafe {
            assert!(DeleteObject(bitmap).as_bool(), "bitmap should be valid");
        }

        let _ = std::fs::remove_file(&small);
        let _ = std::fs::remove_file(&large);
    }
}
