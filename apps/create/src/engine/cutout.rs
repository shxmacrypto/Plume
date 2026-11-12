//! Background remover. Not a trained model: it learns the background's colours from the picture's border,
//! spreads from the border over pixels close to those colours without crossing strong edges, and keeps the rest.
//! It works best on a subject in front of a plain or simple background; the mask it makes can be touched up with the
//! refine brush (hide / show).

use super::img::{Gray, Img, blur};

/// The longest side the analysis works at (the mask is then brought back to full size).
const WORK: u32 = 640;

fn kmeans(samples: &[[f32; 3]], k: usize) -> Vec<[f32; 3]> {
    if samples.is_empty() {
        return vec![[1.0; 3]];
    }
    // start from samples spread along the border
    let mut c: Vec<[f32; 3]> = (0..k).map(|i| samples[i * samples.len() / k]).collect();
    for _ in 0..8 {
        let mut sum = vec![[0.0f32; 3]; k];
        let mut n = vec![0usize; k];
        for s in samples {
            let j = nearest(&c, *s).0;
            for ch in 0..3 {
                sum[j][ch] += s[ch];
            }
            n[j] += 1;
        }
        for j in 0..k {
            if n[j] > 0 {
                c[j] = [sum[j][0] / n[j] as f32, sum[j][1] / n[j] as f32, sum[j][2] / n[j] as f32];
            }
        }
    }
    c
}

fn nearest(c: &[[f32; 3]], p: [f32; 3]) -> (usize, f32) {
    let mut best = (0, f32::MAX);
    for (j, q) in c.iter().enumerate() {
        let d = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
        if d < best.1 {
            best = (j, d);
        }
    }
    (best.0, best.1.sqrt())
}

/// A mask the size of the picture: white where the subject is. `tolerance` 0 to 1: how far from the background's
/// colours a pixel may be and still count as background (0.5 is a good start).
pub fn remove_background(img: &Img, tolerance: f32) -> Gray {
    let small = if img.w.max(img.h) > WORK {
        let k = WORK as f32 / img.w.max(img.h) as f32;
        img.resized(((img.w as f32 * k).round() as u32).max(1), ((img.h as f32 * k).round() as u32).max(1))
    } else {
        img.clone()
    };
    let (w, h) = (small.w as usize, small.h as usize);
    let col: Vec<[f32; 3]> = small.data.as_chunks::<4>().0.iter().map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]).collect();
    let alpha: Vec<u8> = small.data.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
    // the border's colours
    let ring = ((w.min(h) as f32 * 0.02).ceil() as usize).max(1);
    let mut border = vec![];
    for y in 0..h {
        for x in 0..w {
            if (x < ring || y < ring || x >= w - ring || y >= h - ring) && alpha[y * w + x] > 0 {
                border.push(col[y * w + x]);
            }
        }
    }
    let centres = kmeans(&border, 4);
    // how far each pixel is from the background, and how strong the edges are
    let dist: Vec<f32> = col.iter().map(|p| nearest(&centres, *p).1).collect();
    let luma: Vec<f32> = col.iter().map(|p| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]).collect();
    let mut edge = vec![0.0f32; w * h];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let l = |dx: isize, dy: isize| luma[((y as isize + dy) as usize) * w + (x as isize + dx) as usize];
            let gx = l(1, -1) + 2.0 * l(1, 0) + l(1, 1) - l(-1, -1) - 2.0 * l(-1, 0) - l(-1, 1);
            let gy = l(-1, 1) + 2.0 * l(0, 1) + l(1, 1) - l(-1, -1) - 2.0 * l(0, -1) - l(1, -1);
            edge[y * w + x] = (gx * gx + gy * gy).sqrt();
        }
    }
    let t = tolerance.clamp(0.0, 1.0);
    let near = 0.06 + t * 0.32;
    let wall = 0.35 + t * 0.9;
    // spread from the border
    let mut bg = vec![false; w * h];
    let mut stack = vec![];
    for y in 0..h {
        for x in 0..w {
            if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                let i = y * w + x;
                if alpha[i] == 0 || dist[i] < near * 1.6 {
                    bg[i] = true;
                    stack.push(i);
                }
            }
        }
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        let mut go = |j: usize| {
            if !bg[j] && (alpha[j] == 0 || (dist[j] < near && edge[j] < wall)) {
                bg[j] = true;
                stack.push(j);
            }
        };
        if x > 0 {
            go(i - 1);
        }
        if x + 1 < w {
            go(i + 1);
        }
        if y > 0 {
            go(i - w);
        }
        if y + 1 < h {
            go(i + w);
        }
    }
    // thin lines (where two background colours meet) go: shrink the subject by 2 pixels, then grow it back
    let mut fg: Vec<bool> = bg.iter().map(|b| !b).collect();
    for grow in [false, true] {
        for _ in 0..2 {
            let src = fg.clone();
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    let around = [x > 0 && src[i - 1], x + 1 < w && src[i + 1], y > 0 && src[i - w], y + 1 < h && src[i + w]];
                    fg[i] = if grow { src[i] || around.iter().any(|v| *v) } else { src[i] && around.iter().all(|v| *v) };
                }
            }
        }
    }
    for (b, f) in bg.iter_mut().zip(&fg) {
        *b = !f;
    }
    // specks of "subject" too small to matter go
    let min_area = (w * h / 400).max(4);
    let mut label = vec![0u32; w * h];
    let mut next = 1u32;
    for start in 0..w * h {
        if bg[start] || label[start] != 0 {
            continue;
        }
        let mut comp = vec![start];
        label[start] = next;
        let mut k = 0;
        while k < comp.len() {
            let i = comp[k];
            k += 1;
            let (x, y) = (i % w, i / w);
            for (ok, j) in [(x > 0, i.wrapping_sub(1)), (x + 1 < w, i + 1), (y > 0, i.wrapping_sub(w)), (y + 1 < h, i + w)] {
                if ok && !bg[j] && label[j] == 0 {
                    label[j] = next;
                    comp.push(j);
                }
            }
        }
        if comp.len() < min_area {
            for i in comp {
                bg[i] = true;
            }
        }
        next += 1;
    }
    // soft edge, then back to full size
    let mut m: Vec<[f32; 1]> = bg.iter().map(|b| [if *b { 0.0 } else { 1.0 }]).collect();
    blur(&mut m, w, h, 1.2);
    let small_mask = Gray { w: w as u32, h: h as u32, data: m.iter().map(|v| (v[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect() };
    if small.w == img.w && small.h == img.h {
        return small_mask;
    }
    let (kx, ky) = (small.w as f32 / img.w as f32, small.h as f32 / img.h as f32);
    let mut out = Gray::new(img.w, img.h, 0);
    for y in 0..img.h {
        for x in 0..img.w {
            let v = small_mask.sample((x as f32 + 0.5) * kx, (y as f32 + 0.5) * ky, 0.0);
            // sharpen the edge a little after the stretch
            let v = ((v - 0.5) * 1.6 + 0.5).clamp(0.0, 1.0);
            out.set(x, y, (v * 255.0 + 0.5) as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_on_a_plain_background() {
        // a red disc on a light, slightly noisy blue
        let (w, h) = (300u32, 200u32);
        let mut img = Img::new(w, h);
        let mut seed = 7u32;
        for y in 0..h {
            for x in 0..w {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                let n = ((seed >> 16) % 9) as u8;
                let d = ((x as f32 - 150.0).powi(2) + (y as f32 - 100.0).powi(2)).sqrt();
                let c = if d < 60.0 { [200 + n, 30, 40, 255] } else { [170 + n, 200 + n, 235, 255] };
                img.set(x, y, c);
            }
        }
        let m = remove_background(&img, 0.5);
        assert_eq!((m.w, m.h), (w, h));
        assert!(m.get(150, 100) > 240);
        assert!(m.get(5, 5) < 10 && m.get(290, 190) < 10 && m.get(150, 20) < 10);
        // a sky over grass: the line where they meet goes too
        let mut two = Img::new(200, 100);
        for y in 0..100u32 {
            for x in 0..200u32 {
                let d = ((x as f32 - 100.0).powi(2) + (y as f32 - 40.0).powi(2)).sqrt();
                let c = if d < 20.0 {
                    [200, 30, 40, 255]
                } else if y < 60 {
                    [120, 170, 230, 255]
                } else if y == 60 {
                    [90, 160, 140, 255]
                } else {
                    [60, 140, 60, 255]
                };
                two.set(x, y, c);
            }
        }
        let m2 = remove_background(&two, 0.5);
        assert!(m2.get(100, 40) > 240, "{}", m2.get(100, 40));
        // (away from the disc, whose soft edge reaches that row)
        let line = (0..200).filter(|x| !(70..130).contains(x)).map(|x| m2.get(x, 60)).max();
        assert!(line < Some(40), "{line:?}");
        // and bigger than the working size: the mask is stretched back
        let big = img.resized(1200, 800);
        let mb = remove_background(&big, 0.5);
        assert_eq!((mb.w, mb.h), (1200, 800));
        assert!(mb.get(600, 400) > 240 && mb.get(20, 20) < 10);
    }
}
