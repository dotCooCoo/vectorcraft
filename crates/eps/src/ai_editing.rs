//! The editing copy of the art that a PDF-compatible `.ai` file carries in its first page's
//! `PieceInfo` ([`vectorcraft_pdf::PrivateData`]): the document's layers, sublayers, groups, paths,
//! colors, swatches and artboards in the form [`crate::import`]'s `.ai` reader reads, so that apps
//! that open `.ai` files from that copy get them as editable objects.
//!
//! The form is the one the published specification documents (Adobe Illustrator File Format
//! Specification, version 7.0, see [`crate::legacy_ai`]) for paths (§5.3), colors (§5.4),
//! groups and compound paths (§5.3.5), guides (§6) and layers (§9), with the newer forms the reader
//! reads from files users sent:
//!
//! - Header comments: `%%BoundingBox`, `%%HiResBoundingBox`, `%AI5_FileFormat 14.0`, the first
//!   artboard as `%AI3_Cropmarks` and `%AI5_ArtSize`, `%AI5_RulerUnits`, `%AI9_ColorModel` (1 RGB, 2
//!   CMYK), `%AI5_NumLayers`, and the version comment `%%AI8_CreatorVersion` ([`CREATOR_VERSION`]).
//! - Setup: the color swatches (`%AI5_BeginPalette`, `0 0 Pb`, each swatch's color operator and
//!   `(name) Pc`, a color group as `0 (name) count Pg` before its swatches, `PB`), then the
//!   artboards as the `ArtboardArray` of the `/Document` dictionary, on `%_` lines.
//! - Layers: `%AI5_BeginLayer`, `visible preview unlocked printing dimmed 0 1 colorIndex r g b 0
//!   dimPercent 0 Lb` (14 operands; a color that isn't one of the preset layer colors has the
//!   index -1), `(name) Ln`, its art and sublayers, `LB`, `%AI5_EndLayer--`.
//! - Colors: `g` gray, `k` CMYK, `c m y k r g b Xa` RGB (Lab is written as RGB); a spot color
//!   `c m y k (name) tint x` (CMYK) or `c m y k r g b|L a b (name) tint 1|2 Xx` (RGB or Lab); a
//!   global process color `… (name) tint model Xk`; Registration `1 1 1 1 ([Registration]) tint
//!   Xs`. Upper case paints the stroke.
//! - State before each object: locked `A`, hidden `Xw` (`1` or `0`). An object's name follows it as
//!   an `/ArtDictionary` whose `AI10_ArtUID` is the name as an XML id. Guides are `(N) *`, the
//!   path, `(N) *` (`(n) *` when closed).
//!
//! Strings are written in UTF-8. Art space has y up, its origin at the first artboard's bottom-left
//! corner. What the form above can't hold is written as plain filled and stroked paths, each such
//! loss a warning: type as its outlines, gradients as their average color, pattern fills as their
//! tiles cut to the shape, clipping groups and clipping layers as their art cut to the clipping
//! path (the clipping path's fill cut too, its stroke not), images as their outlines filled with
//! their average color, symbols and live objects as their art, strokes a plain line can't draw as
//! their filled outlines, several fills or strokes as a group of a path for each, gradient and
//! pattern swatches, transparency and live effects other than geometry effects left out.
//!
//! The data streams hold the program compressed with Zstandard after `%AI24_ZStandard_Data`, with
//! the version numbers of the newest files users sent ([`private_data`]).

use std::fmt::Write;
use std::sync::Arc;

use vectorcraft_doc::{Document, Node, Unit};
use vectorcraft_geom::{Affine, Rect};
use vectorcraft_pdf::PrivateData;

use crate::ps;

mod writer;

use writer::Writer;

/// The version comment's value: the creator version of the newest files users sent.
pub const CREATOR_VERSION: &str = "30.7.0";
/// The `PieceInfo` numbers that go with Zstandard-compressed data: container 12, creator 30,
/// round trip 24, stream type 2.
const CONTAINER_VERSION: u32 = 12;
const CREATOR: u32 = 30;
const ROUNDTRIP_VERSION: u32 = 24;
const ROUNDTRIP_STREAM_TYPE: u32 = 2;
/// What starts the data streams of Zstandard-compressed data.
const ZSTD_MARKER: &[u8] = b"%AI24_ZStandard_Data";

/// The first line of a PostScript file (the DSC version comment).
const PS_HEADER: &str = "%!PS-Adobe-3.0"; // brand-ok: the PostScript header comment

/// What to write in the header.
#[derive(Clone, Debug, Default)]
pub struct AiEditingOptions {
    /// `%%Title`.
    pub title: String,
    /// `%%CreationDate`, Unix seconds.
    pub created: Option<i64>,
}

/// The editing data and what it couldn't hold.
#[derive(Clone, Debug, Default)]
pub struct AiEditingOutput {
    /// The program: header comments, setup, layers, trailer.
    pub program: Vec<u8>,
    /// Its header comments, up to `%%EndComments`.
    pub header: Vec<u8>,
    pub warnings: Vec<String>,
}

/// Write `doc` as the editing data of a `.ai` file: every artboard and layer.
pub fn export_ai_editing(doc: &Document, o: &AiEditingOptions) -> Result<AiEditingOutput, String> {
    let board = doc.artboards.first().ok_or("the document has no artboard")?.rect;
    if !(board.is_finite() && board.width() > 0.0 && board.height() > 0.0) {
        return Err("the document has no artboard".into());
    }
    // Live geometry effects as their result, as the PDF part draws them.
    let baked = baked(doc);
    let doc = baked.as_ref().unwrap_or(doc);
    // The document is y down; art space y up from the first artboard's bottom-left corner.
    let to_art = Affine::new([1.0, 0.0, 0.0, -1.0, -board.x0, board.y1]);
    let mut w = Writer::new(doc, to_art);
    for layer in &doc.layers {
        w.layer(layer);
    }
    let layers = doc.layers.len();
    let (width, height) = (board.width(), board.height());
    let art = doc
        .layers
        .iter()
        .filter(|l| l.visible)
        .filter_map(|l| l.visual_bounds())
        .map(|b| to_art.transform_rect_bbox(b))
        .filter(|b| b.is_finite())
        .reduce(|a, b| a.union(b))
        .unwrap_or(Rect::new(0.0, 0.0, width, height));
    let header = header(doc, o, &w, art, (width, height), layers);
    let mut s = header.clone();
    s.push_str("%%BeginProlog\n%%EndProlog\n%%BeginSetup\n");
    w.palette(&mut s);
    artboards(&mut s, doc, to_art);
    s.push_str("%%EndSetup\n");
    s.push_str(&w.body);
    s.push_str("%%PageTrailer\ngsave annotatepage grestore showpage\n%%Trailer\n%%EOF\n");
    Ok(AiEditingOutput { program: s.into_bytes(), header: header.into_bytes(), warnings: w.warnings })
}

/// `doc` with its geometry effects baked ([`vectorcraft_effects::bake_document`]), in its symbols'
/// art too (which instances are written as); `None` when nothing changes.
fn baked(doc: &Document) -> Option<Document> {
    let baked = vectorcraft_effects::bake_document(doc);
    let d = baked.as_ref().unwrap_or(doc);
    if !d.symbols.iter().any(|s| vectorcraft_effects::needs_bake(&s.art)) {
        return baked;
    }
    // The symbols' art baked as the layers of a copy of the document.
    let mut art = d.clone();
    art.layers = d.symbols.iter().map(|s| Arc::new(Node::group(s.art.id, vec![s.art.clone()]))).collect();
    // The copy keeps what baking adds to the document (the images color adjustments make).
    let Some(mut out) = vectorcraft_effects::bake_document(&art) else { return baked };
    let arts: Vec<Option<Arc<Node>>> = out.layers.iter().map(|l| l.children().and_then(|c| c.first()).cloned()).collect();
    out.layers = d.layers.clone();
    for (s, a) in out.symbols.iter_mut().zip(arts) {
        if let Some(a) = a {
            s.art = a;
        }
    }
    Some(out)
}

/// The editing data of `doc` as a `.ai` file's first page carries it: the program compressed with
/// Zstandard after `%AI24_ZStandard_Data`, its header as the `AIMetaData`, and the version numbers
/// that go with that form; and what it couldn't hold.
pub fn private_data(doc: &Document, o: &AiEditingOptions) -> Result<(PrivateData, Vec<String>), String> {
    let out = export_ai_editing(doc, o)?;
    let mut data = ZSTD_MARKER.to_vec();
    data.extend(ruzstd::encoding::compress_to_vec(out.program.as_slice(), ruzstd::encoding::CompressionLevel::Fastest));
    let p = PrivateData {
        data,
        metadata: out.header,
        container_version: CONTAINER_VERSION,
        creator_version: CREATOR,
        roundtrip_version: ROUNDTRIP_VERSION,
        roundtrip_stream_type: ROUNDTRIP_STREAM_TYPE,
    };
    Ok((p, out.warnings))
}

/// The header comments, through `%%EndComments`. `art`: the visible art's bounds, `size` the first
/// artboard's, in art space.
fn header(doc: &Document, o: &AiEditingOptions, w: &Writer, art: Rect, (width, height): (f64, f64), layers: usize) -> String {
    let mut s = String::new();
    s.push_str(PS_HEADER);
    s.push('\n');
    let _ = writeln!(s, "%%Creator: VectorCraft {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(s, "%%AI8_CreatorVersion: {CREATOR_VERSION}");
    let _ = writeln!(s, "%%Title: {}", ps::bytes(o.title.as_bytes()));
    if let Some(t) = o.created {
        let [y, mo, d, h, mi, _] = vectorcraft_doc::metadata::civil(t);
        let _ = writeln!(s, "%%CreationDate: ({mo}/{d}/{y:04}) ({h:02}:{mi:02})");
    }
    let _ = writeln!(s, "%%BoundingBox: {} {} {} {}", art.x0.floor() as i64, art.y0.floor() as i64, art.x1.ceil() as i64, art.y1.ceil() as i64);
    s.push_str("%%HiResBoundingBox: ");
    ps::push_nums(&mut s, &[art.x0, art.y0, art.x1, art.y1]);
    s.pop();
    s.push('\n');
    s.push_str("%%DocumentProcessColors: Cyan Magenta Yellow Black\n");
    s.push_str("%AI5_FileFormat 14.0\n%AI3_ColorUsage: Color\n");
    let spots = w.spot_swatches();
    for (i, (name, _)) in spots.iter().enumerate() {
        let _ = writeln!(s, "{} {}", if i == 0 { "%%DocumentCustomColors:" } else { "%%+" }, ps::bytes(name.as_bytes()));
    }
    for (i, (name, cmyk)) in spots.iter().enumerate() {
        s.push_str(if i == 0 { "%%CMYKCustomColor: " } else { "%%+ " });
        ps::push_nums(&mut s, &cmyk.map(f64::from));
        let _ = writeln!(s, "{}", ps::bytes(name.as_bytes()));
    }
    let (w2, h2) = (ps::num(width), ps::num(height));
    let _ = writeln!(s, "%AI3_Cropmarks: 0 0 {w2} {h2}");
    let (cx, cy) = (ps::num(width / 2.0), ps::num(height / 2.0));
    let _ = writeln!(s, "%AI3_TemplateBox: {cx} {cy} {cx} {cy}");
    let _ = writeln!(s, "%AI3_TileBox: 0 0 {w2} {h2}");
    s.push_str("%AI3_DocumentPreview: None\n");
    let _ = writeln!(s, "%AI5_ArtSize: {w2} {h2}");
    let _ = writeln!(s, "%AI5_RulerUnits: {}", ruler_units(doc.units));
    s.push_str("%AI24_LargeCanvasScale: 1\n");
    let model = if doc.color_mode == vectorcraft_doc::ColorMode::Cmyk { 2 } else { 1 };
    let _ = writeln!(s, "%AI9_ColorModel: {model}");
    let _ = writeln!(s, "%AI5_NumLayers: {layers}");
    s.push_str("%%EndComments\n");
    s
}

/// `%AI5_RulerUnits` of `u` (as the reader reads it; units it has no number for are points).
fn ruler_units(u: Unit) -> u8 {
    match u {
        Unit::Inches => 0,
        Unit::Millimeters => 1,
        Unit::Picas => 3,
        Unit::Centimeters => 4,
        Unit::Pixels => 6,
        Unit::Points | Unit::FeetInches | Unit::Meters | Unit::Yards | Unit::Feet => 2,
    }
}

/// The artboards as the `ArtboardArray` of the `/Document` dictionary: each one's top-left and
/// bottom-right corners in art space (`PositionPoint1`, `PositionPoint2`) and its name.
fn artboards(s: &mut String, doc: &Document, to_art: Affine) {
    s.push_str("%AI9_BeginDocumentData\n%_/Document :\n%_/Dictionary :\n%_/Array :\n");
    for ab in &doc.artboards {
        let r = to_art.transform_rect_bbox(ab.rect);
        s.push_str("%_/Dictionary :\n%_");
        ps::push_nums(s, &[r.x0, r.y1]);
        s.push_str("/RealPointRelToROrigin\n%_ (PositionPoint1) ,\n%_");
        ps::push_nums(s, &[r.x1, r.y0]);
        s.push_str("/RealPointRelToROrigin\n%_ (PositionPoint2) ,\n");
        let _ = writeln!(s, "%_{} /UnicodeString (Name) ,\n%_; ,", ps::bytes(ab.name.as_bytes()));
    }
    s.push_str("%_; (ArtboardArray) ,\n%_; /NotRecorded ,\n%_;\n%AI9_EndDocumentData\n");
}

#[cfg(test)]
mod tests;
