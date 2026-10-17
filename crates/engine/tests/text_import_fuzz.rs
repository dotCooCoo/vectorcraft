//! Untrusted text files placed with File → Place never crash: any bytes, with any Text Import
//! Options, place as area type or fail with an error, and so do hostile RTF files.
//!
//! `PROPTEST_CASES=20000 cargo test -p vectorcraft-engine --test text_import_fuzz` runs a deeper search.
// Integration tests: unwrapping and panicking on failure is fine here, unlike in shipped code (AGENTS.md › Robustness).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use serde_json::json;
use vectorcraft_testkit::catch_quiet;

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn any_bytes_place_as_text_without_panics(
        bytes in prop::collection::vec(prop_oneof![any::<u8>(), Just(b'\r'), Just(b'\n'), Just(b' ')], 0..400),
        bom in prop::sample::select(vec![&b""[..], b"\xEF\xBB\xBF", b"\xFF\xFE", b"\xFE\xFF"]),
        ansi in any::<bool>(),
        mac in any::<bool>(),
        lines in any::<bool>(),
        paras in any::<bool>(),
        spaces in prop::option::of(0u64..8),
    ) {
        // Exercise import/options/thumbnails with bundled fonts, independently of host font collections.
        // Font lookup and fallback have dedicated tests in vectorcraft-text.
        vectorcraft_text::FontDb::global().set_system_fallback(false);
        let data = [bom, &bytes[..]].concat();
        let r = catch_quiet(|| {
            let mut s = vectorcraft_engine::Session::new();
            s.execute("file.new", &json!({"width": 100, "height": 100})).unwrap();
            let text = json!({
                "characterSet": if ansi { "ansi" } else { "unicode" },
                "platform": if mac { "mac" } else { "windows" },
                "removeLineReturns": lines,
                "removeParagraphReturns": paras,
                "replaceSpaces": spaces,
            });
            let p = json!({"name": "fuzz.txt", "dataBase64": vectorcraft_format::base64_encode(&data), "text": text, "thumbnail": 8});
            let _ = s.execute("file.place.info", &p);
            let _ = s.execute("file.place", &p);
        });
        prop_assert!(r.is_ok(), "panicked: {:?}", r.err());
    }

    #[test]
    fn any_rtf_places_without_panics(
        tokens in prop::collection::vec(arb_rtf_token(), 0..200),
        name in prop::sample::select(vec!["fuzz.rtf", "fuzz.doc"]),
    ) {
        vectorcraft_text::FontDb::global().set_system_fallback(false);
        let data = [&b"{\\rtf1"[..], &tokens.concat()].concat();
        let r = catch_quiet(|| {
            let mut s = vectorcraft_engine::Session::new();
            s.execute("file.new", &json!({"width": 100, "height": 100})).unwrap();
            let p = json!({"name": name, "dataBase64": vectorcraft_format::base64_encode(&data), "thumbnail": 8});
            let _ = s.execute("file.place.info", &p);
            let _ = s.execute("file.place", &p);
        });
        prop_assert!(r.is_ok(), "panicked: {:?}", r.err());
    }
}

/// Control words the RTF reader knows, and one it doesn't.
const RTF_WORDS: &str = "par line pard plain b i ul ulnone fs f cf u uc qc qj li fi sb sa rtlpar cell row tab fonttbl colortbl red blue \
                         pict bin deff super sub up dn scaps caps ansicpg mac stylesheet upr ud zzunknown";

/// A piece of an RTF file: a brace, a control word the reader knows or doesn't (with a parameter
/// in or out of range, or none), a control symbol, text or raw bytes.
fn arb_rtf_token() -> impl Strategy<Value = Vec<u8>> {
    let words = prop::sample::select(RTF_WORDS.split_whitespace().collect::<Vec<_>>());
    let param = prop::option::of(prop_oneof![-70_000i64..70_000, Just(i64::from(i32::MAX)), Just(-99_999_999_999)]);
    let word = |(w, p, space): (&str, Option<i64>, bool)| {
        let p = p.map(|v| v.to_string()).unwrap_or_default();
        format!("\\{w}{p}{}", if space { " " } else { "" }).into_bytes()
    };
    prop_oneof![
        Just(b"{".to_vec()),
        Just(b"}".to_vec()),
        Just(b"\\*".to_vec()),
        Just(b";".to_vec()),
        (words, param, any::<bool>()).prop_map(word),
        prop::sample::select(vec!["\\'e9", "\\'9", "\\~", "\\-", "\\_", "\\\\", "\\{", "\\}", "\\\r\n", "\\|"]).prop_map(|s| s.as_bytes().to_vec()),
        "[a-z ]{0,6}".prop_map(String::into_bytes),
        prop::collection::vec(any::<u8>(), 0..4),
    ]
}
