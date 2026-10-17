//! Artboard Options (double-click with the Artboard tool, double-click the tool, or the Artboards
//! panel's options icon or menu): the artboard's fields, sizes parsed with units.

use serde_json::{Value, json};

use super::{DialogSpec, run_and_close};
use crate::VectorcraftApp;
use crate::state::Dialog;

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |_| tl!("Artboard Options").into(), confirm, ..DialogSpec::FORM };

/// The dialog kind of Artboard Options.
pub const KIND: &str = "artboardOptions";

/// Open Artboard Options for the active artboard: the Artboard tool's while it is in use, else the
/// window's.
pub fn open(app: &mut VectorcraftApp) -> Result<Value, String> {
    let index = if app.session.tool_id() == "artboard" {
        app.session.tool_options()["active"].as_u64().and_then(|i| usize::try_from(i).ok()).unwrap_or(0)
    } else {
        app.view().map_or(0, |v| v.artboard)
    };
    let st = app.session.active().ok_or("no document")?;
    let a = st.doc.artboards.get(index).ok_or("no artboard")?;
    let r = a.rect;
    let fields = json!({"index": index, "name": a.name, "x": r.x0, "y": r.y0, "width": r.width(), "height": r.height()});
    app.ui.dialog = Some(Dialog::new(KIND, fields));
    Ok(json!({ "dialog": KIND }))
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    if d.bool("new") {
        return confirm_new(app, d);
    }
    let mut p = Value::Object(d.fields.clone());
    for k in ["x", "y", "width", "height"] {
        if d.fields.contains_key(k) {
            p[k] = json!(d.f64(k, 0.0));
        }
    }
    p["moveArt"] = json!(crate::panels::artboards::move_art(app));
    run_and_close(app, "artboard.setProps", p)
}

/// `ui.newArtboard` (Alt-click on the Artboards panel's New Artboard): Artboard Options for a new
/// artboard (field `new`), filled in as `artboard.new {}` would make it: named Artboard N (N one
/// more than the number of artboards), as large as the last artboard and 20 pt to the right of it.
pub fn open_new(app: &mut VectorcraftApp) -> Result<Value, String> {
    let st = app.session.active().ok_or("no document")?;
    let abs = &st.doc.artboards;
    let last = abs.last().map_or(vectorcraft_geom::Rect::new(0.0, 0.0, 612.0, 792.0), |a| a.rect);
    let fields = json!({
        "new": true, "name": format!("Artboard {}", abs.len() + 1),
        "x": last.x1 + 20.0, "y": last.y0, "width": last.width(), "height": last.height(),
    });
    app.ui.dialog = Some(Dialog::new(KIND, fields));
    Ok(json!({ "dialog": KIND }))
}

/// OK for a new artboard: `artboard.new` with the fields (an empty name gives the default one),
/// and the new artboard becomes the active one.
fn confirm_new(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    let mut p = json!({});
    for k in ["x", "y", "width", "height"] {
        p[k] = json!(d.f64(k, 0.0));
    }
    let name = d.str("name");
    if !name.trim().is_empty() {
        p["name"] = json!(name);
    }
    let r = run_and_close(app, "artboard.new", p)?;
    if let Some(i) = r["index"].as_u64().and_then(|i| usize::try_from(i).ok()) {
        crate::panels::artboards::select(app, i);
    }
    Ok(r)
}
