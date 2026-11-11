//! What a photo says about itself (EXIF): when it was taken, the camera, the exposure, where. Read by kamadak-exif,
//! in pure Rust, from JPEG, HEIC, PNG, WebP and TIFF-based files (most camera RAW).

use exif::{In, Reader, Tag, Value};
use std::{fs::File, io::BufReader, path::Path};

fn read(path: &Path) -> Option<exif::Exif> {
    let f = File::open(path).ok()?;
    Reader::new().read_from_container(&mut BufReader::new(f)).ok()
}

/// EXIF orientation, 1 (upright) when there is none.
pub fn orientation(path: &Path) -> u32 {
    if !crate::image::is_photo(path) {
        return 1;
    }
    read(path).and_then(|e| e.get_field(Tag::Orientation, In::PRIMARY).and_then(|f| f.value.get_uint(0))).filter(|o| (1..=8).contains(o)).unwrap_or(1)
}

/// When the photo was taken, as local wall-clock seconds since 1970 (no time zone: as the camera wrote it).
pub fn taken(path: &Path) -> Option<i64> {
    let e = read(path)?;
    date_of(&e)
}

fn date_of(e: &exif::Exif) -> Option<i64> {
    for tag in [Tag::DateTimeOriginal, Tag::DateTimeDigitized, Tag::DateTime] {
        if let Some(f) = e.get_field(tag, In::PRIMARY)
            && let Value::Ascii(v) = &f.value
            && let Some(s) = v.first()
            && let Ok(d) = exif::DateTime::from_ascii(s)
            && d.year > 1900
            && (1..=12).contains(&d.month)
        {
            let days = crate::library::days_from_civil(d.year as i64, d.month as u32, d.day.max(1) as u32);
            return Some(days * 86400 + d.hour as i64 * 3600 + d.minute as i64 * 60 + d.second as i64);
        }
    }
    None
}

/// The facts the info panel shows, in order: (label, value).
#[derive(Clone, Debug, Default)]
pub struct Info {
    pub taken: Option<i64>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub exposure: Option<String>,
    pub aperture: Option<String>,
    pub iso: Option<String>,
    pub focal: Option<String>,
    pub flash: Option<String>,
    /// Latitude, longitude in degrees.
    pub gps: Option<(f64, f64)>,
}

fn text(e: &exif::Exif, tag: Tag) -> Option<String> {
    let f = e.get_field(tag, In::PRIMARY)?;
    let s = match &f.value {
        Value::Ascii(v) => v.iter().map(|b| String::from_utf8_lossy(b).trim_matches(char::from(0)).trim().to_string()).collect::<Vec<_>>().join(" "),
        _ => f.display_value().with_unit(e).to_string(),
    };
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn rational(e: &exif::Exif, tag: Tag) -> Option<f64> {
    match &e.get_field(tag, In::PRIMARY)?.value {
        Value::Rational(v) => v.first().map(|r| r.to_f64()),
        Value::SRational(v) => v.first().map(|r| r.to_f64()),
        v => v.get_uint(0).map(|u| u as f64),
    }
}

fn gps(e: &exif::Exif) -> Option<(f64, f64)> {
    let dms = |tag| -> Option<f64> {
        match &e.get_field(tag, In::PRIMARY)?.value {
            Value::Rational(v) if v.len() >= 3 => Some(v[0].to_f64() + v[1].to_f64() / 60.0 + v[2].to_f64() / 3600.0),
            _ => None,
        }
    };
    let mut lat = dms(Tag::GPSLatitude)?;
    let mut lon = dms(Tag::GPSLongitude)?;
    if text(e, Tag::GPSLatitudeRef).is_some_and(|s| s.starts_with('S')) {
        lat = -lat;
    }
    if text(e, Tag::GPSLongitudeRef).is_some_and(|s| s.starts_with('W')) {
        lon = -lon;
    }
    (lat.is_finite() && lon.is_finite() && (lat != 0.0 || lon != 0.0)).then_some((lat, lon))
}

pub fn info(path: &Path) -> Info {
    let Some(e) = read(path) else { return Info::default() };
    let make = text(&e, Tag::Make);
    let model = text(&e, Tag::Model);
    let camera = match (make, model) {
        // "Canon" + "Canon EOS R6": the make once
        (Some(mk), Some(md)) if md.to_lowercase().starts_with(&mk.to_lowercase()) => Some(md),
        (Some(mk), Some(md)) => Some(format!("{mk} {md}")),
        (a, b) => a.or(b),
    };
    let exposure =
        rational(&e, Tag::ExposureTime).filter(|v| *v > 0.0).map(|t| if t >= 1.0 { format!("{t:.1} s") } else { format!("1/{} s", (1.0 / t).round()) });
    let aperture = rational(&e, Tag::FNumber).filter(|v| *v > 0.0).map(|f| format!("f/{}", trim(f)));
    let iso = e.get_field(Tag::PhotographicSensitivity, In::PRIMARY).and_then(|f| f.value.get_uint(0)).map(|v| format!("ISO {v}"));
    let focal = rational(&e, Tag::FocalLength).filter(|v| *v > 0.0).map(|f| format!("{} mm", trim(f)));
    let flash = e
        .get_field(Tag::Flash, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .map(|v| if v & 1 == 1 { crate::i18n::tr("Flash fired") } else { crate::i18n::tr("No flash") }.to_string());
    Info { taken: date_of(&e), camera, lens: text(&e, Tag::LensModel), exposure, aperture, iso, focal, flash, gps: gps(&e) }
}

/// 2.8 → "2.8", 4.0 → "4".
fn trim(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(trim(2.8), "2.8");
        assert_eq!(trim(4.0), "4");
    }

    #[test]
    fn missing_file() {
        assert_eq!(orientation(Path::new("Z:/nope/none.jpg")), 1);
        assert!(taken(Path::new("Z:/nope/none.jpg")).is_none());
    }
}
