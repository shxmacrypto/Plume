//! Printing: Windows' print dialog, then each page drawn by Windows' PDF engine at the printer's resolution
//! (300 dpi at most) and sent to the printer, centered and scaled to fit the printable area.
//! A test build never opens the dialog and never prints: it only reports which pages would have been printed.

use std::sync::Arc;

/// Page numbers chosen in the dialog (from 1, inclusive) → page indices, within `n` pages.
pub fn pages_from_ranges(ranges: &[(u32, u32)], n: usize) -> Vec<usize> {
    if ranges.is_empty() {
        return (0..n).collect();
    }
    let mut out = Vec::new();
    for &(a, b) in ranges {
        for p in a.min(b).max(1)..=a.max(b) {
            let i = p as usize - 1;
            if i < n && !out.contains(&i) {
                out.push(i);
            }
        }
    }
    out
}

/// Where a page of `pw` × `ph` goes in an area of `aw` × `ah`: (x, y, w, h), centered, proportions kept.
#[cfg(any(test, not(feature = "test-hook")))]
pub fn fit(pw: f32, ph: f32, aw: f32, ah: f32) -> (i32, i32, i32, i32) {
    let k = (aw / pw.max(1.0)).min(ah / ph.max(1.0));
    let (w, h) = (pw * k, ph * k);
    (((aw - w) / 2.0).round() as i32, ((ah - h) / 2.0).round() as i32, w.round() as i32, h.round() as i32)
}

/// Asks where to print, then prints in the background. `done` is told the outcome (pages printed, or an error).
#[cfg(not(feature = "test-hook"))]
pub fn run(
    owner: windows::Win32::Foundation::HWND,
    title: &str,
    bytes: Arc<Vec<u8>>,
    password: Option<String>,
    n: usize,
    done: impl FnOnce(Result<usize, String>) + Send + 'static,
) {
    use windows::Win32::{
        Foundation::{GlobalFree, HGLOBAL},
        UI::Controls::Dialogs::{
            PD_NOCURRENTPAGE, PD_NOSELECTION, PD_PAGENUMS, PD_RESULT_PRINT, PD_RETURNDC, PD_USEDEVMODECOPIESANDCOLLATE, PRINTDLGEXW, PRINTPAGERANGE,
            PrintDlgExW, START_PAGE_GENERAL,
        },
    };
    let mut ranges = vec![PRINTPAGERANGE { nFromPage: 1, nToPage: n as u32 }; 16];
    let mut pd = PRINTDLGEXW {
        lStructSize: size_of::<PRINTDLGEXW>() as u32,
        hwndOwner: owner,
        Flags: PD_RETURNDC | PD_NOSELECTION | PD_NOCURRENTPAGE | PD_USEDEVMODECOPIESANDCOLLATE,
        nPageRanges: 0,
        nMaxPageRanges: ranges.len() as u32,
        lpPageRanges: ranges.as_mut_ptr(),
        nMinPage: 1,
        nMaxPage: n as u32,
        nCopies: 1,
        nStartPage: START_PAGE_GENERAL,
        ..Default::default()
    };
    let ok = unsafe { PrintDlgExW(&mut pd) }.is_ok();
    unsafe {
        if pd.hDevMode != HGLOBAL::default() {
            let _ = GlobalFree(Some(pd.hDevMode));
        }
        if pd.hDevNames != HGLOBAL::default() {
            let _ = GlobalFree(Some(pd.hDevNames));
        }
    }
    if !ok || pd.dwResultAction != PD_RESULT_PRINT || pd.hDC.is_invalid() {
        if !pd.hDC.is_invalid() {
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::DeleteDC(pd.hDC);
            }
        }
        return;
    }
    let chosen: Vec<(u32, u32)> =
        if pd.Flags & PD_PAGENUMS == PD_PAGENUMS { ranges[..pd.nPageRanges as usize].iter().map(|r| (r.nFromPage, r.nToPage)).collect() } else { Vec::new() };
    let pages = pages_from_ranges(&chosen, n);
    let hdc = pd.hDC.0 as isize;
    let title = title.to_string();
    std::thread::spawn(move || done(print_pages(hdc, &title, &bytes, password.as_deref(), &pages)));
}

#[cfg(not(feature = "test-hook"))]
fn print_pages(hdc: isize, title: &str, bytes: &[u8], password: Option<&str>, pages: &[usize]) -> Result<usize, String> {
    use windows::{
        Win32::{
            Graphics::{
                Gdi::{
                    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteDC, GetDeviceCaps, HALFTONE, HDC, HORZRES, LOGPIXELSX, LOGPIXELSY, SRCCOPY,
                    SetStretchBltMode, StretchDIBits, VERTRES,
                },
                Imaging::{CLSID_WICImagingFactory, IWICImagingFactory},
            },
            Storage::Xps::{DOCINFOW, EndDoc, EndPage, StartDocW, StartPage},
            System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
        },
        core::PCWSTR,
    };
    let hdc = HDC(hdc as _);
    let result = (|| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message().to_string())?;
        let doc = crate::render::open(bytes, password).map_err(|e| format!("{e:?}"))?;
        let sizes = crate::render::sizes(&doc);
        let (aw, ah) = (GetDeviceCaps(Some(hdc), HORZRES) as f32, GetDeviceCaps(Some(hdc), VERTRES) as f32);
        let dpi = GetDeviceCaps(Some(hdc), LOGPIXELSX).max(GetDeviceCaps(Some(hdc), LOGPIXELSY)).max(72) as f32;
        let name: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        let info = DOCINFOW { cbSize: size_of::<DOCINFOW>() as i32, lpszDocName: PCWSTR(name.as_ptr()), ..Default::default() };
        if StartDocW(hdc, &info) <= 0 {
            return Err("The printer refused the document.".to_string());
        }
        let mut printed = 0;
        let mut bmp = true;
        for &i in pages {
            let (pw, ph) = sizes.get(i).copied().unwrap_or((612.0, 792.0));
            let (x, y, w, h) = fit(pw, ph, aw, ah);
            // at most 300 dots per inch: enough for paper, and a page stays under 35 MB of pixels
            let k = (300.0f32.min(dpi) / dpi).min(1.0);
            let (rw, rh) = (((w as f32) * k).round().max(1.0) as u32, ((h as f32) * k).round().max(1.0) as u32);
            let Some(px) = crate::render::draw(&doc, &wic, i, rw, rh, false, &mut bmp) else { continue };
            if StartPage(hdc) <= 0 {
                break;
            }
            let bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: px.w as i32,
                    biHeight: -(px.h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            SetStretchBltMode(hdc, HALFTONE);
            StretchDIBits(hdc, x, y, w, h, 0, 0, px.w as i32, px.h as i32, Some(px.data.as_ptr() as _), &bi, DIB_RGB_COLORS, SRCCOPY);
            EndPage(hdc);
            printed += 1;
        }
        EndDoc(hdc);
        Ok(printed)
    })();
    unsafe {
        let _ = DeleteDC(hdc);
    }
    result
}

/// Test build: no dialog, nothing printed; the pages that would have been printed are reported.
#[cfg(feature = "test-hook")]
pub fn run(
    _owner: windows::Win32::Foundation::HWND,
    _title: &str,
    _bytes: Arc<Vec<u8>>,
    _password: Option<String>,
    n: usize,
    done: impl FnOnce(Result<usize, String>) + Send + 'static,
) {
    done(Err(format!("test build: print skipped ({} pages would print)", pages_from_ranges(&[], n).len())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_and_fitting() {
        assert_eq!(pages_from_ranges(&[], 3), vec![0, 1, 2]);
        assert_eq!(pages_from_ranges(&[(2, 3), (3, 1), (9, 9)], 4), vec![1, 2, 0]);
        // an A4 page on a wider area: full height, centered horizontally
        let (x, y, w, h) = fit(595.0, 842.0, 4000.0, 4000.0);
        assert_eq!((y, h), (0, 4000));
        assert!((x - (4000 - w) / 2).abs() <= 1);
    }
}
