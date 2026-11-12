//! The project file (.plumecreate): one JSON text holding the document and its pictures (PNG in base64).
//! A picture used by several layers (or several undo steps) is stored once.

use super::{
    doc::{Doc, Kind, Layer},
    img::{Gray, Img},
    png,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path, sync::Arc};

pub const EXT: &str = "plumecreate";

#[derive(Serialize, Deserialize)]
struct File {
    app: String,
    version: u32,
    doc: Doc,
    images: Vec<String>,
    masks: Vec<String>,
}

pub fn to_string(doc: &Doc) -> String {
    let mut doc = doc.clone();
    let mut images: Vec<String> = vec![];
    let mut masks: Vec<String> = vec![];
    let mut seen_i: HashMap<usize, u32> = HashMap::new();
    let mut seen_m: HashMap<usize, u32> = HashMap::new();
    fn walk(v: &mut [Layer], images: &mut Vec<String>, masks: &mut Vec<String>, si: &mut HashMap<usize, u32>, sm: &mut HashMap<usize, u32>) {
        for l in v {
            if let Some(m) = &mut l.mask {
                let p = Arc::as_ptr(&m.px) as usize;
                m.key = *sm.entry(p).or_insert_with(|| {
                    masks.push(png::base64(&png::encode_gray(&m.px)));
                    masks.len() as u32 - 1
                });
            }
            match &mut l.kind {
                Kind::Image(im) => {
                    let p = Arc::as_ptr(&im.px) as usize;
                    im.key = *si.entry(p).or_insert_with(|| {
                        images.push(png::base64(&png::encode_rgba(&im.px)));
                        images.len() as u32 - 1
                    });
                }
                Kind::Group(c) => walk(c, images, masks, si, sm),
                _ => {}
            }
        }
    }
    walk(&mut doc.layers, &mut images, &mut masks, &mut seen_i, &mut seen_m);
    serde_json::to_string(&File { app: "plume-create".into(), version: 1, doc, images, masks }).unwrap_or_default()
}

pub fn from_str(s: &str) -> Result<Doc, String> {
    let f: File = serde_json::from_str(s).map_err(|e| e.to_string())?;
    if f.app != "plume-create" {
        return Err("not a Plume Create project".into());
    }
    let images: Vec<Arc<Img>> =
        f.images.iter().map(|b| png::unbase64(b).and_then(|d| png::decode_rgba(&d)).map(Arc::new).ok_or("damaged picture")).collect::<Result<_, _>>()?;
    let masks: Vec<Arc<Gray>> =
        f.masks.iter().map(|b| png::unbase64(b).and_then(|d| png::decode_gray(&d)).map(Arc::new).ok_or("damaged mask")).collect::<Result<_, _>>()?;
    let mut doc = f.doc;
    fn walk(v: &mut [Layer], images: &[Arc<Img>], masks: &[Arc<Gray>]) -> Result<(), String> {
        for l in v {
            if let Some(m) = &mut l.mask {
                m.px = masks.get(m.key as usize).cloned().ok_or("missing mask")?;
            }
            match &mut l.kind {
                Kind::Image(im) => im.px = images.get(im.key as usize).cloned().ok_or("missing picture")?,
                Kind::Group(c) => walk(c, images, masks)?,
                _ => {}
            }
        }
        Ok(())
    }
    walk(&mut doc.layers, &images, &masks)?;
    if doc.w == 0 || doc.h == 0 {
        return Err("empty canvas".into());
    }
    // ids stay unique whatever the file says
    let max = doc.all().iter().map(|l| l.id).max().unwrap_or(0);
    doc.next_id = doc.next_id.max(max + 1);
    Ok(doc)
}

/// Writes through a temporary file: a failure leaves the old project as it was.
pub fn save(doc: &Doc, path: &Path) -> Result<(), String> {
    let tmp = path.with_extension("plumecreate-tmp");
    std::fs::write(&tmp, to_string(doc)).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

pub fn load(path: &Path) -> Result<Doc, String> {
    from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::doc::{Adjust, Blend, ImageL, Look, Mask, ShapeL, TextL, Xf};

    #[test]
    fn save_and_load_keep_everything() {
        let mut d = Doc::new(300, 200);
        let mut img = Img::filled(30, 20, [10, 20, 30, 255]);
        img.set(3, 4, [200, 100, 0, 128]);
        let shared = Arc::new(img);
        let id = d.id();
        let mut a = Layer::new(id, "photo", Xf::new(50.0, 50.0, 60.0, 40.0), Kind::Image(ImageL { px: shared.clone(), key: 0 }));
        a.adj = Adjust { look: Look::Film, exposure: 0.3, ..Adjust::default() };
        a.blend = Blend::Screen;
        a.mask = Some(Mask { px: Arc::new(Gray::new(30, 20, 77)), key: 0, on: true });
        let id = d.id();
        let b = Layer::new(id, "copy", Xf::new(150.0, 50.0, 60.0, 40.0), Kind::Image(ImageL { px: shared, key: 0 }));
        let id = d.id();
        let t = Layer::new(id, "title", Xf::new(100.0, 150.0, 200.0, 50.0), Kind::Text(TextL { text: "Hé là".into(), ..TextL::default() }));
        let id = d.id();
        let g = Layer::new(
            id,
            "group",
            Xf::new(0.0, 0.0, 0.0, 0.0),
            Kind::Group(vec![t, Layer::new(99, "s", Xf::new(1.0, 1.0, 2.0, 2.0), Kind::Shape(ShapeL::default()))]),
        );
        d.layers = vec![a, b, g];
        let s = to_string(&d);
        // the shared picture is stored once
        let f: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(f["images"].as_array().unwrap().len(), 1);
        let back = from_str(&s).unwrap();
        assert_eq!((back.w, back.h), (300, 200));
        assert_eq!(back.layers.len(), 3);
        assert_eq!(back.layers[0].adj, d.layers[0].adj);
        assert_eq!(back.layers[0].blend, Blend::Screen);
        assert_eq!(back.layers[0].image().unwrap().get(3, 4), [200, 100, 0, 128]);
        assert!(Arc::ptr_eq(back.layers[0].image().unwrap(), back.layers[1].image().unwrap()));
        assert_eq!(back.layers[0].mask.as_ref().unwrap().px.get(1, 1), 77);
        match &back.find(99).unwrap().kind {
            Kind::Shape(_) => {}
            _ => panic!(),
        }
        assert!(back.next_id > 99);
        assert!(from_str("{}").is_err());
        // on disk
        let dir = std::env::temp_dir().join(format!("plume-create-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.plumecreate");
        save(&d, &p).unwrap();
        assert_eq!(load(&p).unwrap().layers.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
