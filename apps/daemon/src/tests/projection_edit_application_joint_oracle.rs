// Fixed wire geometry and independent RGBA construction; never call Otter's edit merge or renderer for expectations.
use super::*;
pub(super) fn document(
    local: (&Https, &Identity),
    remote: (&Https, &Identity),
    envelope: &ProjectionEnvelope,
    expected: &Value,
    revision: u64,
    x: f64,
) {
    let read = json!({"operation":"read","projectionId":envelope.projection_id});
    let local = local.0.offline(local.1, "edit", read.clone());
    let remote = remote.0.offline(remote.1, "edit", read);
    assert_eq!(local.0, 200, "{local:?}");
    assert_eq!(remote.0, 200, "{remote:?}");
    assert_eq!(local.1, remote.1);
    assert_eq!(&local.1, expected);
    let value = &local.1;
    assert_eq!(value["revision"], revision);
    assert_eq!(value["modeRevision"], if revision < 5 { 2 } else { 5 });
    assert_eq!(value["checkpointRevision"], 1);
    assert_eq!(value["receiptCount"], revision - 1);
    assert_eq!(
        value["mode"],
        if revision < 5 { "two_way" } else { "one_way" }
    );
    assert_eq!(value["basis"]["width"], 32);
    assert_eq!(value["basis"]["height"], 32);
    let objects = value["objects"].as_object().unwrap();
    assert_eq!(objects.len(), 1);
    let (id, object) = objects.iter().next().unwrap();
    assert_eq!(
        object["revision"],
        if revision < 3 { 1 } else { revision.min(4) }
    );
    assert_eq!(object["value"]["id"].as_str(), Some(id.as_str()));
    assert_eq!(object["value"]["type"], "rect");
    for (key, coordinate) in [("x", x), ("y", 4.0), ("w", 8.0), ("h", 8.0)] {
        assert_eq!(object["value"][key].as_f64(), Some(coordinate), "{key}");
    }
    assert!(value.get("bindings").is_none() && value.get("receipts").is_none());
}
pub(super) fn basis(root: &Path, value: &Value) {
    let bytes = BASE64
        .decode(value["snapshot"]["imageBase64"].as_str().unwrap())
        .unwrap();
    assert_eq!(sha256_bytes(&bytes), value["digest"]);
    assert_eq!(value["snapshot"]["width"], 32);
    assert_eq!(value["snapshot"]["height"], 32);
    let raster = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert_eq!(raster.dimensions(), (32, 32));
    assert!(
        raster.pixels().all(|p| p.0 == [0, 0, 255, 255]),
        "basis must not bake the red annotation twice"
    );
    write_once(&root.join("basis.png"), &bytes);
}
pub(super) fn frame(path: &Path) {
    let bytes = fs::read(path).unwrap();
    let raster = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert_eq!(raster.dimensions(), (32, 32));
    for (x, y, pixel) in raster.enumerate_pixels() {
        // Rectangle (9,4)-(17,12), 2px red centered stroke, opaque blue base.
        let outer = (8..18).contains(&x) && (3..13).contains(&y);
        let inner = (10..16).contains(&x) && (5..11).contains(&y);
        let expected = if outer && !inner {
            [255, 0, 0, 255]
        } else {
            [0, 0, 255, 255]
        };
        assert_eq!(
            pixel.0,
            expected,
            "independent pixel ({x},{y}) {}",
            path.display()
        );
    }
}
