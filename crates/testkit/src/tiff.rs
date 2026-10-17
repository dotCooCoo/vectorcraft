//! Hand-written TIFF files for import tests: little-endian, uncompressed, 8 or 16 bits a sample,
//! with each pixel's samples stored together (chunky) or one plane per sample (planar,
//! PlanarConfiguration 2) (TIFF 6.0, sections 2 and 8).

/// One image: its channels in the order the file stores them, color first, extra samples after.
#[derive(Clone, Debug, Default)]
pub struct Tiff<'a> {
    pub width: u32,
    pub height: u32,
    /// PhotometricInterpretation: 0 white is zero, 1 black is zero, 2 RGB, 5 separated (CMYK).
    pub photometric: u16,
    /// Bits a sample: 8 or 16.
    pub bits: u16,
    /// The ExtraSamples values, one for each channel after the color (0 not alpha, 1 associated
    /// alpha, 2 unassociated alpha); none writes no ExtraSamples tag.
    pub extra: &'a [u16],
    /// One plane per channel (PlanarConfiguration 2) instead of each pixel's samples together.
    pub planar: bool,
    /// Pixels per inch, written as XResolution and YResolution with ResolutionUnit 2.
    pub ppi: Option<u32>,
    /// The channels: `width` × `height` samples each, row by row.
    pub channels: &'a [Vec<u16>],
}

/// Pads `out` to an even length: TIFF offsets point at word boundaries.
fn align(out: &mut Vec<u8>) {
    if out.len() % 2 == 1 {
        out.push(0);
    }
}

/// A directory entry: its tag, field type, count and value bytes.
type Entry = (u16, u16, usize, Vec<u8>);

/// The TIFF file of `t`: the header, the directory at offset 8, the values longer than four
/// bytes, then the image in one strip a plane.
pub fn tiff(t: &Tiff) -> Vec<u8> {
    let pixels = usize::try_from(t.width * t.height).unwrap();
    assert!(t.channels.iter().all(|c| c.len() == pixels), "every channel has a sample a pixel");
    assert!(matches!(t.bits, 8 | 16), "8 or 16 bits a sample");
    let mut strips: Vec<Vec<u8>> = vec![];
    let samples_of = |i: usize| t.channels.iter().map(move |c| c[i]);
    let order: Vec<Vec<u16>> = if t.planar { t.channels.to_vec() } else { vec![(0..pixels).flat_map(samples_of).collect()] };
    for values in order {
        strips.push(match t.bits {
            8 => values.iter().map(|&v| u8::try_from(v).unwrap()).collect(),
            _ => values.iter().flat_map(|v| v.to_le_bytes()).collect(),
        });
    }
    let samples = u16::try_from(t.channels.len()).unwrap();
    let counts: Vec<u32> = strips.iter().map(|s| u32::try_from(s.len()).unwrap()).collect();
    let entries = |offsets: &[u32]| -> Vec<Entry> {
        let shorts = |tag, v: &[u16]| (tag, 3, v.len(), v.iter().flat_map(|x| x.to_le_bytes()).collect());
        let longs = |tag, v: &[u32]| (tag, 4, v.len(), v.iter().flat_map(|x| x.to_le_bytes()).collect());
        let rational = |tag, n: u32| (tag, 5, 1, [n.to_le_bytes(), 1u32.to_le_bytes()].concat());
        // In ascending tag order.
        let mut e = vec![
            longs(256, &[t.width]),
            longs(257, &[t.height]),
            shorts(258, &vec![t.bits; usize::from(samples)]),
            shorts(259, &[1]),
            shorts(262, &[t.photometric]),
            longs(273, offsets),
            shorts(277, &[samples]),
            longs(278, &[t.height]),
            longs(279, &counts),
        ];
        if let Some(ppi) = t.ppi {
            e.extend([rational(282, ppi), rational(283, ppi)]);
        }
        e.push(shorts(284, &[if t.planar { 2 } else { 1 }]));
        if t.ppi.is_some() {
            e.push(shorts(296, &[2]));
        }
        if !t.extra.is_empty() {
            e.push(shorts(338, t.extra));
        }
        e
    };
    // The strips' offsets follow from the sizes of the rest, which don't depend on them.
    let padded = |n: usize| n + n % 2;
    let sized = entries(&vec![0; strips.len()]);
    let ifd_len = 2 + 12 * sized.len() + 4;
    let values_len: usize = sized.iter().filter(|e| e.3.len() > 4).map(|e| padded(e.3.len())).sum();
    let mut at = 8 + ifd_len + values_len;
    let mut offsets = vec![];
    for s in &strips {
        offsets.push(u32::try_from(at).unwrap());
        at += padded(s.len());
    }
    let mut out = b"II*\0\x08\0\0\0".to_vec();
    let entries = entries(&offsets);
    out.extend(u16::try_from(entries.len()).unwrap().to_le_bytes());
    let mut values = vec![];
    for (tag, ty, count, bytes) in entries {
        out.extend(tag.to_le_bytes());
        out.extend(ty.to_le_bytes());
        out.extend(u32::try_from(count).unwrap().to_le_bytes());
        if bytes.len() <= 4 {
            out.extend(&bytes);
            out.extend(vec![0; 4 - bytes.len()]);
        } else {
            out.extend(u32::try_from(8 + ifd_len + values.len()).unwrap().to_le_bytes());
            values.extend(bytes);
            align(&mut values);
        }
    }
    out.extend([0; 4]);
    out.extend(values);
    for s in strips {
        out.extend(s);
        align(&mut out);
    }
    assert_eq!(out.len(), at, "the strips sit where the directory says");
    out
}
