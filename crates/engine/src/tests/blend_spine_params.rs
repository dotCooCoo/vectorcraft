use serde_json::json;

use crate::{EngineError, Session};

/// Spine commands report each invalid parameter.
#[test]
fn blend_spine_param_errors_say_what_is_wrong() {
    let mut s = Session::new();
    s.execute("file.new", &json!({})).unwrap();
    let rectangle =
        |s: &mut Session, x: u64, y: u64| s.execute("shape.rectangle", &json!({"x": x, "y": y, "width": 10, "height": 10})).unwrap()["id"].clone();
    let blend = |s: &mut Session, y: u64| {
        let keys = [0, 100].map(|x| rectangle(s, x, y));
        s.execute("object.blend.make", &json!({"ids": keys, "steps": 1})).unwrap()["id"].clone()
    };
    let replaced = |s: &mut Session, y: u64, anchors: serde_json::Value, closed: bool| {
        let b = blend(s, y);
        let path = s.execute("path.create", &json!({"anchors": anchors, "closed": closed})).unwrap()["id"].clone();
        s.execute("select.set", &json!({"ids": [b, path]})).unwrap();
        s.execute("object.blend.replaceSpine", &json!({})).unwrap();
        b
    };
    let ring = replaced(&mut s, 200, json!([{"x": 0, "y": 205}, {"x": 100, "y": 205}]), true);
    let dot = {
        let keys = [0, 100].map(|x| rectangle(&mut s, x, 400));
        s.execute("object.blend.make", &json!({"ids": keys, "steps": 1, "createSpine": "never"})).unwrap()["id"].clone()
    };
    let other = rectangle(&mut s, 0, 600);
    let b = blend(&mut s, 0);
    s.execute("select.set", &json!({"ids": [b]})).unwrap();
    for (cmd, p, want) in [
        ("object.blend.spine.moveAnchor", json!({}), "missing number `anchor`"),
        ("object.blend.spine.moveAnchor", json!({"anchor": -1, "x": 0, "y": 0}), "index must be ≥ 0"),
        ("object.blend.spine.moveAnchor", json!({"anchor": 0, "x": 1e12, "y": 0}), "x and y must be finite"),
        ("object.blend.spine.moveAnchor", json!({"anchor": 0, "x": 0, "y": 0, "handle": "up"}), "handle must be in or out"),
        ("object.blend.spine.moveAnchor", json!({"anchor": 5, "x": 0, "y": 0}), "no such spine point"),
        (
            "object.blend.spine.moveAnchor",
            json!({"anchor": 0, "x": 0, "y": 0, "with": "1"}),
            "`with` must be a list of at most 10000 spine point indexes",
        ),
        ("object.blend.spine.moveAnchor", json!({"anchor": 0, "x": 0, "y": 0, "with": [1], "handle": "in"}), "`with` moves points, not a handle"),
        ("object.blend.spine.moveAnchor", json!({"id": other, "anchor": 0, "x": 0, "y": 0}), "not a blend"),
        ("object.blend.spine.reshapeSegment", json!({"segment": 0, "dx": 1e12}), "t, dx and dy must be finite"),
        ("object.blend.spine.reshapeSegment", json!({"segment": 5, "dy": 10}), "no such spine segment"),
        ("object.blend.spine.removeAnchor", json!({"anchor": 0}), "a key object sits on that point"),
        ("object.blend.spine.removeAnchor", json!({"id": ring, "anchor": 1}), "a spine keeps at least two points"),
        ("object.blend.spine.addAnchor", json!({"id": dot, "x": 0, "y": 0}), "the blend has no spine"),
    ] {
        let e = s.execute(cmd, &p).unwrap_err();
        assert!(matches!(&e, EngineError::BadParams { msg, .. } if msg == want), "{cmd} {p}: {e}");
    }
    s.execute("select.none", &json!({})).unwrap();
    let e = s.execute("object.blend.spine.moveAnchor", &json!({"anchor": 0, "x": 0, "y": 0})).unwrap_err();
    assert!(matches!(&e, EngineError::BadParams { msg, .. } if msg == "select a blend or give `id`"), "{e}");
}
