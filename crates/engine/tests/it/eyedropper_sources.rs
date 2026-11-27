//! Visible art is readable by the Eyedropper without making it editable (#1251).

use serde_json::json;
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_engine::{Session, UiRequest, ViewInfo};
use vectorcraft_tools::{Mods, PointerEvent, PointerKind};

fn fixture() -> (Session, NodeId, NodeId, NodeId) {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    let layer = s.doc().unwrap().doc.layers[0].id;
    let source = NodeId(s.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": 60, "height": 60})).unwrap()["id"].as_u64().unwrap());
    s.execute("paint.setFill", &json!({"color": "#e23355"})).unwrap();
    s.execute("stroke.set", &json!({"weight": 7})).unwrap();
    s.execute("layer.new", &json!({"name": "Target"})).unwrap();
    let target = NodeId(s.execute("shape.rectangle", &json!({"x": 140, "y": 100, "width": 120, "height": 80})).unwrap()["id"].as_u64().unwrap());
    s.execute("paint.setFill", &json!({"color": "#2255ee"})).unwrap();
    s.execute("stroke.set", &json!({"weight": 1})).unwrap();
    (s, source, target, layer)
}

fn click(s: &mut Session, mods: Mods) {
    s.select_tool("eyedropper", ViewInfo::default()).unwrap();
    for kind in [PointerKind::Down, PointerKind::Up] {
        s.pointer(&PointerEvent::new(kind, 40.0, 40.0).with_mods(mods), ViewInfo::default()).unwrap();
    }
}

fn assert_sample(s: &mut Session, source: NodeId, target: NodeId, mods: Mods) {
    let st = s.doc().unwrap();
    let before = st.doc.clone();
    let selection = st.selection.clone();
    let history = st.history.undo.len();
    click(s, mods);
    let st = s.doc().unwrap();
    let source_before = before.node(source).unwrap();
    let target_after = st.doc.node(target).unwrap();
    assert_eq!(json!(st.doc.node(source).unwrap()), json!(source_before), "sampling must not edit its source");
    assert_eq!(json!(st.selection), json!(selection));
    assert_eq!(target_after.geometric_bounds(), before.node(target).unwrap().geometric_bounds());
    if mods.alt {
        assert_eq!(target_after.appearance.items.len(), before.node(target).unwrap().appearance.items.len() + source_before.appearance.items.len());
    } else {
        assert_eq!(target_after.appearance.fill_paint(), source_before.appearance.fill_paint());
        assert_eq!(target_after.appearance.stroke().unwrap().width, if mods.shift { 1.0 } else { 7.0 });
    }
    assert_eq!(st.history.undo.len(), history + 1);
    let after = st.doc.clone();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(json!(s.doc().unwrap().doc), json!(before));
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(json!(s.doc().unwrap().doc), json!(after));
}

fn read_modes() -> [Mods; 3] {
    [Mods::default(), Mods { shift: true, ..Default::default() }, Mods { shift: true, alt: true, ..Default::default() }]
}

#[test]
fn locked_objects_and_layers_are_sources_in_all_read_modes() {
    for lock_layer in [false, true] {
        for mods in read_modes() {
            let (mut s, source, target, layer) = fixture();
            s.execute("layer.setProps", &json!({"id": if lock_layer { layer } else { source }, "locked": true})).unwrap();
            assert_sample(&mut s, source, target, mods);
        }
    }
}

#[test]
fn isolation_allows_reading_surrounding_art_without_leaving_the_group() {
    for mods in read_modes() {
        let (mut s, source, target, _) = fixture();
        let group = NodeId(s.execute("object.group", &json!({})).unwrap()["id"].as_u64().unwrap());
        s.execute("object.isolate", &json!({"id": group})).unwrap();
        s.execute("select.set", &json!({"ids": [target]})).unwrap();
        assert_sample(&mut s, source, target, mods);
        assert_eq!(s.doc().unwrap().isolation, Some(group));
    }
}

#[test]
fn alt_reverse_still_respects_locks_and_isolation_and_hidden_sources_are_skipped() {
    for restriction in ["object", "layer", "isolation", "hidden"] {
        let (mut s, source, target, layer) = fixture();
        match restriction {
            "isolation" => {
                let group = s.execute("object.group", &json!({})).unwrap()["id"].clone();
                s.execute("object.isolate", &json!({"id": group})).unwrap();
                s.execute("select.set", &json!({"ids": [target]})).unwrap();
            }
            "hidden" => {
                s.execute("layer.setProps", &json!({"id": layer, "visible": false})).unwrap();
            }
            _ => {
                s.execute("layer.setProps", &json!({"id": if restriction == "layer" { layer } else { source }, "locked": true})).unwrap();
            }
        }
        let before_doc = s.doc().unwrap().doc.clone();
        let before = json!(before_doc);
        let history = s.doc().unwrap().history.undo.len();
        click(&mut s, Mods { alt: restriction != "hidden", ..Default::default() });
        if restriction == "hidden" {
            let mut expected = (*before_doc).clone();
            expected.node_mut(target).unwrap().appearance.set_fill(Paint::solid(Color::WHITE));
            assert_eq!(json!(s.doc().unwrap().doc), json!(expected));
            assert_eq!(s.doc().unwrap().history.undo.len(), history + 1);
            s.execute("edit.undo", &json!({})).unwrap();
            assert_eq!(json!(s.doc().unwrap().doc), before);
            s.execute("edit.redo", &json!({})).unwrap();
            assert_eq!(json!(s.doc().unwrap().doc), json!(expected));
        } else {
            assert_eq!(json!(s.doc().unwrap().doc), before);
            assert_eq!(s.doc().unwrap().history.undo.len(), history);
        }
        s.select_tool("selection", ViewInfo::default()).unwrap();
        s.pointer(&PointerEvent::new(PointerKind::Down, 40.0, 40.0), ViewInfo::default()).unwrap();
        assert!(!s.doc().unwrap().selection.objects.contains(&source));
    }
}

#[test]
fn a_gradient_stop_can_read_a_locked_source_and_return_to_the_gradient_tool() {
    let (mut s, source, target, layer) = fixture();
    s.execute("paint.editGradient", &json!({})).unwrap();
    s.execute("gradient.selectStop", &json!({"index": 1})).unwrap();
    s.execute("layer.setProps", &json!({"id": layer, "locked": true})).unwrap();
    s.select_tool("eyedropper", ViewInfo::default()).unwrap();
    s.set_tool_option("stop", &json!("gradient"));
    let before = json!(s.doc().unwrap().doc.node(source).unwrap());
    let requests = s
        .pointer(&PointerEvent::new(PointerKind::Down, 40.0, 40.0).with_mods(Mods { alt: true, ..Default::default() }), ViewInfo::default())
        .unwrap();
    assert!(matches!(requests.as_slice(), [UiRequest::SwitchTool(tool)] if tool == "gradient"));
    assert_eq!(s.tool_options()["stop"], json!(null));
    assert_eq!(json!(s.doc().unwrap().doc.node(source).unwrap()), before);
    let paint = s.doc().unwrap().doc.node(target).unwrap().appearance.fill_paint();
    let vectorcraft_color::Paint::Gradient(g) = paint else { panic!("gradient retained") };
    assert_eq!(g.gradient.stops[1].color.to_hex(), "#e23355");
}

#[test]
fn paper_samples_recolor_only_picked_mesh_points_with_either_active_proxy() {
    for fill_active in [true, false] {
        let (mut s, mesh, target, _) = fixture();
        s.execute("object.mesh.create", &json!({"ids": [mesh], "rows": 1, "cols": 1})).unwrap();
        s.execute("select.set", &json!({"ids": [mesh, target]})).unwrap();
        s.select_tool("eyedropper", ViewInfo::default()).unwrap();
        s.set_tool_option("meshPoints", &json!([{"id": mesh, "index": 0}]));
        s.execute("paint.toggleActive", &json!({"fill": fill_active})).unwrap();
        let before = s.doc().unwrap().doc.clone();
        let history = s.doc().unwrap().history.undo.len();
        let mut expected = (*before).clone();
        let NodeKind::Mesh(points) = &mut expected.node_mut(mesh).unwrap().kind else { panic!("mesh retained") };
        assert_ne!(points.points[0].color, Color::WHITE);
        points.points[0].color = Color::WHITE;
        for kind in [PointerKind::Down, PointerKind::Up] {
            s.pointer(&PointerEvent::new(kind, 350.0, 250.0), ViewInfo::default()).unwrap();
        }
        assert_eq!(json!(s.doc().unwrap().doc), json!(expected), "fill proxy active: {fill_active}");
        assert_eq!(s.doc().unwrap().history.undo.len(), history + 1);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(json!(s.doc().unwrap().doc), json!(before));
        s.execute("edit.redo", &json!({})).unwrap();
        assert_eq!(json!(s.doc().unwrap().doc), json!(expected));
    }
}
