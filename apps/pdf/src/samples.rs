//! Documents for the screenshots and the test socket (test builds only): a six-page annual report of a fictional
//! ferry company (colored header bars, headings, paragraphs, tables, a small chart, bookmarks) and a one-page
//! membership form. All the text is made up.

use crate::edit::{helv_bold_width, helv_width, page_ids, text_width_pt, winansi, wrap};
use lopdf::{
    Bookmark, Document, Object, ObjectId, Stream, StringFormat,
    content::{Content, Operation},
    dictionary,
};

const W: f32 = 595.0;
const H: f32 = 842.0;
const NAVY: [f32; 3] = [0.09, 0.2, 0.3];
const INK: [f32; 3] = [0.13, 0.15, 0.18];
const GREY: [f32; 3] = [0.45, 0.48, 0.52];
const AMBER: [f32; 3] = [0.95, 0.65, 0.25];
const PALE: [f32; 3] = [0.92, 0.95, 0.98];

fn n(v: f32) -> Object {
    Object::Real((v * 100.0).round() / 100.0)
}

/// One page's drawing, with y measured from the top of the page.
#[derive(Default)]
struct Pg {
    ops: Vec<Operation>,
}

impl Pg {
    fn op(&mut self, name: &str, args: Vec<Object>) {
        self.ops.push(Operation::new(name, args));
    }
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: [f32; 3]) {
        self.op("q", vec![]);
        self.op("rg", c.iter().map(|v| n(*v)).collect());
        self.op("re", vec![n(x), n(H - y - h), n(w), n(h)]);
        self.op("f", vec![]);
        self.op("Q", vec![]);
    }
    fn frame(&mut self, x: f32, y: f32, w: f32, h: f32, c: [f32; 3], lw: f32) {
        self.op("q", vec![]);
        self.op("RG", c.iter().map(|v| n(*v)).collect());
        self.op("w", vec![n(lw)]);
        self.op("re", vec![n(x), n(H - y - h), n(w), n(h)]);
        self.op("S", vec![]);
        self.op("Q", vec![]);
    }
    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, c: [f32; 3], lw: f32) {
        self.op("q", vec![]);
        self.op("RG", c.iter().map(|v| n(*v)).collect());
        self.op("w", vec![n(lw)]);
        self.op("m", vec![n(x1), n(H - y1)]);
        self.op("l", vec![n(x2), n(H - y2)]);
        self.op("S", vec![]);
        self.op("Q", vec![]);
    }
    /// Text with its baseline at `y`. `bold`: Helvetica-Bold.
    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, c: [f32; 3], s: &str) {
        self.op("BT", vec![]);
        self.op("Tf", vec![Object::Name(if bold { b"F2".to_vec() } else { b"F1".to_vec() }), n(size)]);
        self.op("rg", c.iter().map(|v| n(*v)).collect());
        self.op("Td", vec![n(x), n(H - y)]);
        self.op("Tj", vec![Object::String(winansi(s), StringFormat::Literal)]);
        self.op("ET", vec![]);
    }
    fn text_right(&mut self, right: f32, y: f32, size: f32, bold: bool, c: [f32; 3], s: &str) {
        let w = if bold { s.chars().map(helv_bold_width).sum::<f32>() * size / 1000.0 } else { text_width_pt(s, size) };
        self.text(right - w, y, size, bold, c, s);
    }
    /// A paragraph from `y` (first baseline); returns the baseline after it.
    fn para(&mut self, x: f32, y: f32, width: f32, size: f32, s: &str) -> f32 {
        let lead = size * 1.5;
        let mut y = y;
        for line in wrap(s, width, size) {
            self.text(x, y, size, false, INK, &line);
            y += lead;
        }
        y + size * 0.6
    }
    fn heading(&mut self, y: f32, s: &str) -> f32 {
        self.text(56.0, y, 19.0, true, NAVY, s);
        self.rect(56.0, y + 8.0, 36.0, 3.0, AMBER);
        y + 34.0
    }
    fn running_head(&mut self, section: &str) {
        self.rect(0.0, 0.0, W, 34.0, NAVY);
        self.text(56.0, 21.5, 9.0, true, [1.0, 1.0, 1.0], "LUMEN BAY FERRIES");
        self.text_right(W - 56.0, 21.5, 9.0, false, [0.78, 0.85, 0.92], section);
    }
    fn footer(&mut self, page: usize) {
        self.line(56.0, H - 44.0, W - 56.0, H - 44.0, [0.85, 0.87, 0.9], 0.6);
        self.text(56.0, H - 30.0, 8.5, false, GREY, "Lumen Bay Ferries \u{b7} Annual report 2026");
        self.text_right(W - 56.0, H - 30.0, 8.5, false, GREY, &page.to_string());
    }
    /// A table: column x positions (right-aligned columns marked), a navy header row and shaded rows.
    fn table(&mut self, y: f32, cols: &[(f32, bool)], head: &[&str], rows: &[Vec<&str>], total: bool) -> f32 {
        let (x0, x1, rh) = (56.0, W - 56.0, 22.0);
        self.rect(x0, y, x1 - x0, rh, NAVY);
        for (k, h) in head.iter().enumerate() {
            let (cx, right) = cols[k];
            if right {
                self.text_right(cx, y + 14.5, 9.0, true, [1.0, 1.0, 1.0], h);
            } else {
                self.text(cx, y + 14.5, 9.0, true, [1.0, 1.0, 1.0], h);
            }
        }
        let mut ry = y + rh;
        for (i, row) in rows.iter().enumerate() {
            let last = total && i + 1 == rows.len();
            if i % 2 == 1 && !last {
                self.rect(x0, ry, x1 - x0, rh, PALE);
            }
            if last {
                self.line(x0, ry, x1, ry, NAVY, 1.0);
            }
            for (k, cell) in row.iter().enumerate() {
                let (cx, right) = cols[k];
                if right {
                    self.text_right(cx, ry + 14.5, 9.5, last, INK, cell);
                } else {
                    self.text(cx, ry + 14.5, 9.5, last, INK, cell);
                }
            }
            ry += rh;
        }
        self.line(x0, ry, x1, ry, [0.8, 0.83, 0.87], 0.6);
        ry + 26.0
    }
}

/// The character a Windows Latin byte stands for (the ones used here; the rest as Latin-1).
fn decode_winansi(b: u8) -> char {
    match b {
        0x95 => '\u{2022}',
        0x96 => '\u{2013}',
        0x97 => '\u{2014}',
        0x85 => '\u{2026}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201c}',
        0x94 => '\u{201d}',
        0x80 => '\u{20ac}',
        b => b as char,
    }
}

struct Builder {
    doc: Document,
    pages_id: ObjectId,
    resources: ObjectId,
    kids: Vec<Object>,
}

impl Builder {
    fn new() -> Builder {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        // width tables for every Windows Latin code (32..255), so dashes and bullets get their room
        let table = |f: fn(char) -> f32| -> Vec<Object> { (32..=255u8).map(|b| Object::Integer(f(decode_winansi(b)) as i64)).collect() };
        let f1 = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
            "FirstChar" => 32, "LastChar" => 255, "Widths" => table(helv_width),
        });
        let f2 = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding",
            "FirstChar" => 32, "LastChar" => 255, "Widths" => table(helv_bold_width),
        });
        let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => f1, "F2" => f2 } });
        Builder { doc, pages_id, resources, kids: Vec::new() }
    }
    fn add(&mut self, pg: Pg) -> ObjectId {
        let content = self.doc.add_object(Stream::new(dictionary! {}, Content { operations: pg.ops }.encode().unwrap()));
        let page = self.doc.add_object(dictionary! { "Type" => "Page", "Parent" => self.pages_id, "Contents" => content, "Resources" => self.resources });
        self.kids.push(Object::Reference(page));
        page
    }
    fn finish(mut self, title: &str) -> Document {
        let count = self.kids.len() as i64;
        self.doc.objects.insert(
            self.pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => self.kids, "Count" => count,
                "MediaBox" => vec![0.into(), 0.into(), (W as i64).into(), (H as i64).into()],
            }),
        );
        let info = self.doc.add_object(dictionary! { "Title" => Object::string_literal(title), "Author" => Object::string_literal("Lumen Bay Ferries") });
        let catalog = self.doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => self.pages_id });
        self.doc.trailer.set("Root", catalog);
        self.doc.trailer.set("Info", info);
        self.doc
    }
}

/// "Annual report 2026" of Lumen Bay Ferries, six pages with bookmarks.
pub fn report() -> Document {
    let mut b = Builder::new();
    let tw = W - 112.0;
    let mut marks = Vec::new();

    // 1: cover and letter
    let mut p = Pg::default();
    p.rect(0.0, 0.0, W, 150.0, NAVY);
    p.rect(0.0, 150.0, W, 5.0, AMBER);
    p.text(56.0, 58.0, 10.0, true, [0.98, 0.78, 0.45], "LUMEN BAY FERRIES");
    p.text(56.0, 100.0, 32.0, true, [1.0, 1.0, 1.0], "Annual report 2026");
    p.text(56.0, 126.0, 12.0, false, [0.78, 0.85, 0.92], "Crossing the bay since 1974");
    let mut y = p.heading(204.0, "A letter from our chief executive");
    for s in [
        "Twenty twenty-six was the year our small fleet carried more people across the bay than ever before. Nearly two million passengers chose a Lumen Bay crossing, and nine in ten of them arrived on time, even through the long storm season of early spring.",
        "We welcomed two new hybrid vessels, the Marigold and the Kestrel, which now run the busy morning routes on battery power alone. Their quiet engines have already become a favourite with commuters, and they cut our fuel use on those routes by more than half.",
        "None of this would be possible without the four hundred and twelve people who keep our boats, piers and ticket offices running every day of the year. On behalf of the board, thank you for your care, your patience and your good humour.",
        "In the pages that follow you will find the figures behind our year, the state of our fleet and routes, and our plans for a fully electric service by 2030.",
    ] {
        y = p.para(56.0, y, tw, 10.5, s);
    }
    p.text(56.0, y + 6.0, 10.5, false, INK, "With warm regards,");
    p.line(56.0, y + 62.0, 236.0, y + 62.0, [0.75, 0.78, 0.82], 0.8);
    p.text(56.0, y + 78.0, 11.0, true, NAVY, "Mara Ellison");
    p.text(56.0, y + 92.0, 9.5, false, GREY, "Chief Executive, Lumen Bay Ferries");
    p.footer(1);
    marks.push(("A letter from our chief executive", b.add(p)));

    // 2: the year at a glance
    let mut p = Pg::default();
    p.running_head("The year at a glance");
    let y = p.heading(84.0, "The year at a glance");
    let y = p.para(
        56.0,
        y,
        tw,
        10.5,
        "Passenger numbers grew on every route, led by the night service and the summer islands loop. Punctuality improved for the third year in a row, and revenue passed fifty million euros for the first time.",
    );
    let cw = (tw - 30.0) / 4.0;
    for (k, (big, label)) in
        [("1.94 M", "passengers carried"), ("41,280", "crossings sailed"), ("91.6 %", "arrived on time"), ("58.3 M", "euros of revenue")].iter().enumerate()
    {
        let x = 56.0 + k as f32 * (cw + 10.0);
        p.rect(x, y, cw, 64.0, PALE);
        p.rect(x, y, 3.0, 64.0, AMBER);
        p.text(x + 14.0, y + 32.0, 20.0, true, NAVY, big);
        p.text(x + 14.0, y + 50.0, 9.0, false, GREY, label);
    }
    let y = y + 96.0;
    p.text(56.0, y, 12.0, true, NAVY, "Passengers by route");
    let cols = [(64.0, false), (380.0, true), (460.0, true), (531.0, true)];
    let y = p.table(
        y + 12.0,
        &cols,
        &["Route", "2025", "2026", "Change"],
        &[
            vec!["Harbour \u{2013} North Point", "612,400", "668,900", "+9.2 %"],
            vec!["Harbour \u{2013} Gull Island", "401,100", "433,700", "+8.1 %"],
            vec!["North Point \u{2013} Saltmarsh", "288,950", "301,200", "+4.2 %"],
            vec!["Night service", "92,300", "118,640", "+28.5 %"],
            vec!["Summer islands loop", "360,000", "417,560", "+16.0 %"],
            vec!["All routes", "1,754,750", "1,940,000", "+10.6 %"],
        ],
        true,
    );
    p.para(
        56.0,
        y,
        tw,
        10.5,
        "The night service was extended to Friday and Saturday all year after a trial in 2025, and it is now our fastest growing route. Passengers carried on the summer islands loop rose thanks to a longer season, from early May to the end of September.",
    );
    p.footer(2);
    marks.push(("The year at a glance", b.add(p)));

    // 3: fleet and routes
    let mut p = Pg::default();
    p.running_head("Fleet and routes");
    let mut y = p.heading(84.0, "Fleet and routes");
    y = p.para(
        56.0,
        y,
        tw,
        10.5,
        "Six vessels now sail under the Lumen Bay flag. The Marigold and the Kestrel joined in March and May, built at the Saltmarsh yard to a design shared with two other island operators. Each carries two hundred and eighty passengers and charges at the harbour pier in under twenty minutes.",
    );
    let cols = [(64.0, false), (250.0, true), (330.0, true), (360.0, false)];
    y = p.table(
        y + 4.0,
        &cols,
        &["Vessel", "Built", "Seats", "Power"],
        &[
            vec!["Marigold", "2026", "280", "Hybrid, battery first"],
            vec!["Kestrel", "2026", "280", "Hybrid, battery first"],
            vec!["Tern", "2019", "140", "Hybrid"],
            vec!["Puffin", "2015", "120", "Diesel-electric"],
            vec!["Osprey", "2011", "350", "Diesel"],
            vec!["Heron", "2008", "320", "Diesel"],
        ],
        false,
    );
    y = p.text_block(
        y,
        tw,
        &[
            "The Heron will retire at the end of 2027, after nineteen years and more than ninety thousand crossings. A third hybrid vessel is on order to take its place on the Gull Island route.",
            "Timetables were simplified in September: every route now leaves on the same minutes past the hour, all day, which passengers told us makes the service far easier to remember.",
        ],
    );
    let _ = y;
    p.footer(3);
    marks.push(("Fleet and routes", b.add(p)));

    // 4: people and safety
    let mut p = Pg::default();
    p.running_head("People and safety");
    let mut y = p.heading(84.0, "People and safety");
    y = p.text_block(
        y,
        tw,
        &[
            "Our crews sailed more than forty-one thousand crossings this year without a single serious injury to a passenger or a member of staff. Every deckhand now completes a refresher course in sea rescue each spring, run with the bay's volunteer lifeboat crew.",
            "We hired fifty-eight new colleagues, most of them from the towns we serve, and twelve deckhands began the two-year path to becoming licensed skippers.",
        ],
    );
    p.text(56.0, y, 12.0, true, NAVY, "What changed on board");
    y += 22.0;
    for s in [
        "Step-free boarding on every pier, with new ramps at Gull Island and Saltmarsh.",
        "Quiet carriages on the morning commuter crossings.",
        "Free drinking water and bicycle racks on all six vessels.",
        "Live departure times on screens at every pier and in the timetable app.",
    ] {
        p.text(62.0, y, 10.5, false, AMBER, "\u{2022}");
        y = p.para(76.0, y, tw - 20.0, 10.5, s) - 4.0;
    }
    p.text_block(
        y + 10.0,
        tw,
        &["Staff turnover fell to six percent, the lowest in a decade. In our yearly survey, eighty-seven percent of colleagues said they would recommend Lumen Bay as a place to work."],
    );
    p.footer(4);
    marks.push(("People and safety", b.add(p)));

    // 5: environment, with a small chart
    let mut p = Pg::default();
    p.running_head("Environment");
    let mut y = p.heading(84.0, "Environment");
    y = p.text_block(
        y,
        tw,
        &["Fuel used per crossing fell by more than forty percent since 2022, as hybrid vessels took over the busiest routes and every boat was fitted with a speed and route planner that avoids strong currents."],
    );
    p.text(56.0, y, 12.0, true, NAVY, "Fuel used per crossing, litres");
    let base = y + 190.0;
    let vals = [("2022", 410.0), ("2023", 395.0), ("2024", 372.0), ("2025", 318.0), ("2026", 236.0)];
    for k in 0..=4 {
        let gy = base - k as f32 * 40.0;
        p.line(56.0, gy, W - 56.0, gy, [0.88, 0.9, 0.93], 0.5);
        p.text(56.0, gy - 3.0, 7.5, false, GREY, &(k * 100).to_string());
    }
    for (i, (yr, v)) in vals.iter().enumerate() {
        let x = 110.0 + i as f32 * 82.0;
        let h = v * 0.4;
        p.rect(x, base - h, 46.0, h, if i == 4 { AMBER } else { NAVY });
        p.text(x + 8.0, base + 14.0, 9.0, false, GREY, yr);
        p.text(x + 10.0, base - h - 6.0, 9.0, true, INK, &format!("{v:.0}"));
    }
    p.text_block(
        base + 44.0,
        tw,
        &[
            "Shore power now feeds the boats at night at the harbour and North Point piers, so engines no longer run while crews prepare the morning sailings.",
            "Our goal remains a fully electric service by 2030. The next step is a charging pier at Gull Island, planned with the island council for 2028.",
        ],
    );
    p.footer(5);
    marks.push(("Environment", b.add(p)));

    // 6: financial summary
    let mut p = Pg::default();
    p.running_head("Financial summary");
    let mut y = p.heading(84.0, "Financial summary");
    y = p.text_block(y, tw, &["All figures are in millions of euros. The accounts were reviewed by the board's audit committee in February."]);
    let cols = [(64.0, false), (440.0, true), (531.0, true)];
    y = p.table(
        y + 4.0,
        &cols,
        &["", "2025", "2026"],
        &[
            vec!["Tickets and passes", "47.9", "53.6"],
            vec!["Freight and vehicles", "3.8", "4.1"],
            vec!["Other income", "0.5", "0.6"],
            vec!["Crews and staff", "-21.4", "-23.0"],
            vec!["Fuel and energy", "-9.7", "-7.9"],
            vec!["Maintenance and piers", "-8.1", "-8.8"],
            vec!["Operating result", "13.0", "18.6"],
        ],
        true,
    );
    y = p.text_block(
        y,
        tw,
        &["The stronger result comes mostly from lower fuel costs and higher passenger numbers. Most of it will be invested in the third hybrid vessel and the Gull Island charging pier."],
    );
    p.text(56.0, y + 10.0, 10.5, false, INK, "Approved by the board on 12 March 2027.");
    p.line(56.0, y + 70.0, 236.0, y + 70.0, [0.75, 0.78, 0.82], 0.8);
    p.text(56.0, y + 86.0, 9.5, false, GREY, "Chair of the board");
    p.footer(6);
    marks.push(("Financial summary", b.add(p)));

    let mut doc = b.finish("Annual report 2026");
    for (title, page) in marks {
        doc.add_bookmark(Bookmark::new(title.to_string(), [0.0, 0.0, 0.0], 0, page), None);
    }
    if let Some(o) = doc.build_outline() {
        doc.catalog_mut().unwrap().set("Outlines", o);
    }
    doc
}

impl Pg {
    fn text_block(&mut self, y: f32, width: f32, paras: &[&str]) -> f32 {
        let mut y = y;
        for s in paras {
            y = self.para(56.0, y, width, 10.5, s);
        }
        y
    }
}

/// A one-page membership form: three text fields and three checkboxes, boxes drawn on the page.
pub fn form() -> Document {
    let mut b = Builder::new();
    let mut p = Pg::default();
    p.rect(0.0, 0.0, W, 110.0, NAVY);
    p.rect(0.0, 110.0, W, 4.0, AMBER);
    p.text(56.0, 52.0, 10.0, true, [0.98, 0.78, 0.45], "LUMEN BAY FERRIES");
    p.text(56.0, 86.0, 24.0, true, [1.0, 1.0, 1.0], "Frequent traveller card");
    let mut y = p.para(
        56.0,
        156.0,
        W - 112.0,
        10.5,
        "Fill in this form to receive a card for discounted crossings on every Lumen Bay route. Cards are sent within ten days and are valid for two years.",
    );
    let text_fields = [("name", "Full name"), ("email", "Email address"), ("harbour", "Home harbour")];
    let mut rects = Vec::new();
    for (key, label) in text_fields {
        p.text(56.0, y + 6.0, 9.5, true, NAVY, label);
        p.rect(56.0, y + 14.0, W - 112.0, 26.0, PALE);
        p.frame(56.0, y + 14.0, W - 112.0, 26.0, [0.72, 0.77, 0.83], 0.8);
        rects.push((key, false, [56.0, H - (y + 40.0), W - 56.0, H - (y + 14.0)]));
        y += 58.0;
    }
    p.text(56.0, y + 8.0, 12.0, true, NAVY, "Passes");
    y += 26.0;
    for (key, label) in
        [("commuter", "Morning commuter pass (weekdays before 9:00)"), ("family", "Weekend family pass"), ("terms", "I accept the terms of the card")]
    {
        p.rect(56.0, y, 16.0, 16.0, PALE);
        p.frame(56.0, y, 16.0, 16.0, [0.72, 0.77, 0.83], 0.8);
        p.text(82.0, y + 12.0, 10.5, false, INK, label);
        rects.push((key, true, [56.0, H - (y + 16.0), 72.0, H - y]));
        y += 28.0;
    }
    p.line(56.0, y + 60.0, 256.0, y + 60.0, [0.75, 0.78, 0.82], 0.8);
    p.text(56.0, y + 76.0, 9.5, false, GREY, "Signature");
    p.line(320.0, y + 60.0, W - 56.0, y + 60.0, [0.75, 0.78, 0.82], 0.8);
    p.text(320.0, y + 76.0, 9.5, false, GREY, "Date");
    let page = b.add(p);
    let mut doc = b.finish("Frequent traveller card");
    let mut ids = Vec::new();
    for (key, check, r) in rects {
        let mut d = dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "T" => Object::string_literal(key), "P" => page, "F" => 4,
            "Rect" => r.iter().map(|v| n(*v)).collect::<Vec<_>>(),
        };
        if check {
            d.set("FT", "Btn");
            d.set("V", "Off");
            d.set("AS", "Off");
        } else {
            d.set("FT", "Tx");
            d.set("DA", Object::string_literal("/Helv 11 Tf 0 g"));
        }
        ids.push(Object::Reference(doc.add_object(d)));
    }
    doc.get_dictionary_mut(page_ids(&doc)[0]).unwrap().set("Annots", ids.clone());
    let form = doc.add_object(dictionary! { "Fields" => ids, "DA" => Object::string_literal("/Helv 0 Tf 0 g") });
    doc.catalog_mut().unwrap().set("AcroForm", form);
    doc
}

#[cfg(test)]
mod tests {
    #[test]
    fn showcase_documents_read_back() {
        let r = crate::edit::load(&crate::edit::to_bytes(&super::report(), None).unwrap(), None).unwrap();
        assert_eq!(crate::edit::page_ids(&r).len(), 6);
        assert_eq!(crate::edit::outline(&r).len(), 6);
        assert!(crate::text::find(&crate::text::all(&r), "passengers").len() >= 4);
        let f = crate::edit::load(&crate::edit::to_bytes(&super::form(), None).unwrap(), None).unwrap();
        assert_eq!(crate::edit::fields(&f).len(), 6);
    }
}
