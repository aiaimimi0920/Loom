use serde::Deserialize;
use serde_json::{json, Value};

use super::*;

#[derive(Deserialize)]
struct Fixture {
    layout: WallLayout,
    endpoints: Vec<TileEndpoint>,
    cases: Vec<Case>,
    projections: Vec<ProjectionCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    tile_id: String,
    pixel: TilePixelPoint,
    wall: WallPoint,
    source: WallPoint,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectionCase {
    tile_id: String,
    source_crop: WallRect,
    output_quad: [WallPoint; 4],
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "../../../../protocol/fixtures/wall-geometry.v1.json"
    ))
    .unwrap()
}

fn endpoint<'a>(fixture: &'a Fixture, tile_id: &str) -> &'a TileEndpoint {
    let tile = fixture
        .layout
        .tiles
        .iter()
        .find(|tile| tile.tile_id == tile_id)
        .unwrap();
    fixture
        .endpoints
        .iter()
        .find(|endpoint| endpoint.endpoint_id == tile.endpoint_id)
        .unwrap()
}

fn point_eq(actual: WallPoint, expected: WallPoint) {
    assert!(
        (actual.x - expected.x).abs() < 1e-10,
        "x: {actual:?} != {expected:?}"
    );
    assert!(
        (actual.y - expected.y).abs() < 1e-10,
        "y: {actual:?} != {expected:?}"
    );
}

#[test]
fn cross_language_fixture_maps_pixels_and_source_crops() {
    let data = fixture();
    for case in &data.cases {
        let geometry =
            WallGeometry::new(&data.layout, endpoint(&data, &case.tile_id), &case.tile_id).unwrap();
        let wall = geometry.pixel_to_wall(case.pixel).unwrap();
        point_eq(wall, case.wall);
        assert_eq!(geometry.wall_to_pixel(wall), Some(case.pixel));
        let hit = geometry
            .hit_test(data.layout.revision, case.pixel)
            .unwrap()
            .unwrap();
        assert_eq!(hit.placement_id, "application");
        assert_eq!(hit.source, WallContentSource::Live("live-1".into()));
        point_eq(hit.source_point, case.source);
    }
    for case in &data.projections {
        let geometry =
            WallGeometry::new(&data.layout, endpoint(&data, &case.tile_id), &case.tile_id).unwrap();
        let projections = geometry.projections();
        assert_eq!(projections.len(), 1);
        let actual = &projections[0];
        point_eq(
            WallPoint {
                x: actual.source_crop.x,
                y: actual.source_crop.y,
            },
            WallPoint {
                x: case.source_crop.x,
                y: case.source_crop.y,
            },
        );
        point_eq(
            WallPoint {
                x: actual.source_crop.width,
                y: actual.source_crop.height,
            },
            WallPoint {
                x: case.source_crop.width,
                y: case.source_crop.height,
            },
        );
        for (point, expected) in actual.output_quad.iter().zip(&case.output_quad) {
            point_eq(*point, *expected);
        }
    }
}

#[test]
fn every_rotation_preserves_pixel_identity_including_edge_pixels() {
    let mut data = fixture();
    for rotation in [
        TileRotation::Deg0,
        TileRotation::Deg90,
        TileRotation::Deg180,
        TileRotation::Deg270,
    ] {
        data.layout.tiles[1].rotation = rotation;
        let geometry = WallGeometry::new(&data.layout, &data.endpoints[1], "right").unwrap();
        for x in [0, 1, 49, 50, 98, 99] {
            for y in [0, 1, 99, 100, 198, 199] {
                let pixel = TilePixelPoint { x, y };
                assert_eq!(
                    geometry.wall_to_pixel(geometry.pixel_to_wall(pixel).unwrap()),
                    Some(pixel)
                );
            }
        }
    }
}

#[test]
fn seam_has_one_owner_and_stale_or_outside_input_is_rejected() {
    let data = fixture();
    let left = WallGeometry::new(&data.layout, &data.endpoints[0], "left").unwrap();
    let right = WallGeometry::new(&data.layout, &data.endpoints[1], "right").unwrap();
    assert!(left.wall_to_pixel(WallPoint { x: 0.0, y: 0.0 }).is_none());
    assert!(right.wall_to_pixel(WallPoint { x: 0.0, y: 0.0 }).is_some());
    assert!(left.hit_test(6, TilePixelPoint { x: 0, y: 0 }).is_err());
    assert!(left.pixel_to_wall(TilePixelPoint { x: 100, y: 0 }).is_err());
    assert!(left
        .wall_to_pixel(WallPoint {
            x: f64::NAN,
            y: 0.0
        })
        .is_none());
    assert!(WallGeometry::new(&data.layout, &data.endpoints[1], "left").is_err());
}

#[test]
fn paint_order_and_hit_order_agree_and_foreground_blocks_click_through() {
    let mut data = fixture();
    let mut foreground = data.layout.placements[0].clone();
    foreground.placement_id = "foreground".into();
    foreground.source = WallContentSource::Surface("surface-1".into());
    data.layout.placements.push(foreground);
    // Equal z-index is broken by the same ASCII ID order on both runtimes.
    let geometry = WallGeometry::new(&data.layout, &data.endpoints[0], "left").unwrap();
    let pixel = TilePixelPoint { x: 50, y: 50 };
    assert_eq!(
        geometry.projections().last().unwrap().placement_id,
        "foreground"
    );
    assert_eq!(
        geometry.hit_test(7, pixel).unwrap().unwrap().placement_id,
        "foreground"
    );
    data.layout.placements[1].interactive = false;
    let geometry = WallGeometry::new(&data.layout, &data.endpoints[0], "left").unwrap();
    assert!(geometry.hit_test(7, pixel).unwrap().is_none());
    data.layout.placements[1].rect.x = 500.0;
    let geometry = WallGeometry::new(&data.layout, &data.endpoints[0], "left").unwrap();
    assert_eq!(geometry.projections().len(), 1);
    assert_eq!(
        geometry.hit_test(7, pixel).unwrap().unwrap().placement_id,
        "application"
    );
}

#[test]
fn rejects_nonfinite_tiny_overflowing_and_conflicting_layouts() {
    let original = fixture().layout;
    let mut invalid = Vec::new();
    for width in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e-300, 1_000_001.0] {
        let mut layout = original.clone();
        layout.bounds.width = width;
        invalid.push(layout);
    }
    let mut layout = original.clone();
    layout.tiles[1].rect.x = -1.0;
    invalid.push(layout);
    let mut layout = original.clone();
    layout.tiles[1].endpoint_id = layout.tiles[0].endpoint_id.clone();
    invalid.push(layout);
    let mut layout = original.clone();
    layout.placements[0].source_crop.width = 1.0;
    invalid.push(layout);
    let mut layout = original.clone();
    layout.placements.push(layout.placements[0].clone());
    invalid.push(layout);
    let mut layout = original.clone();
    layout.revision = WALL_MAX_REVISION + 1;
    invalid.push(layout);
    let mut layout = original;
    layout.tiles[1].rect.x = 200.0;
    invalid.push(layout);
    for layout in invalid {
        assert!(validate_wall_layout(&layout).is_err());
    }
}

#[test]
fn endpoints_advertise_bounded_distinct_capabilities_without_requiring_inputs() {
    let mut endpoint = fixture().endpoints.remove(0);
    endpoint.input_capabilities.clear();
    assert!(validate_tile_endpoint(&endpoint).is_ok());
    endpoint.render_modes.push(endpoint.render_modes[0]);
    assert!(validate_tile_endpoint(&endpoint).is_err());
    endpoint.render_modes.clear();
    assert!(validate_tile_endpoint(&endpoint).is_err());
    endpoint.render_modes.push(TileRenderMode::RawBgra);
    endpoint.pixel_size.width = 16_385;
    assert!(validate_tile_endpoint(&endpoint).is_err());
}

#[test]
fn display_metadata_is_optional_bounded_and_strict() {
    let mut endpoint = fixture().endpoints.remove(0);
    assert!(serde_json::to_value(&endpoint)
        .unwrap()
        .get("display")
        .is_none());
    endpoint.display = Some(TileDisplayInfo {
        name: "Screen 1".into(),
        can_identify: true,
    });
    assert!(validate_tile_endpoint(&endpoint).is_ok());
    let schema: Value = serde_json::from_str(crate::schemas::WALL_V1).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&serde_json::to_value(&endpoint).unwrap()));
    for invalid in [
        json!(null),
        json!({"name":"Screen 1","canIdentify":"yes"}),
        json!({"name":"Screen 1","canIdentify":true,"extra":0}),
    ] {
        let mut value = serde_json::to_value(&endpoint).unwrap();
        value["display"] = invalid;
        assert!(serde_json::from_value::<TileEndpoint>(value.clone()).is_err());
        assert!(!validator.is_valid(&value));
    }
    for name in [
        String::new(),
        "   ".into(),
        "Screen\n1".into(),
        "x".repeat(257),
    ] {
        endpoint.display.as_mut().unwrap().name = name;
        assert!(validate_tile_endpoint(&endpoint).is_err());
    }
}

#[test]
fn schema_and_serde_reject_unknown_fields_and_invalid_wire_shapes() {
    let schema: Value = serde_json::from_str(crate::schemas::WALL_V1).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let data = fixture();
    let valid = serde_json::to_value(&data.layout).unwrap();
    assert!(validator.is_valid(&valid));
    for endpoint in &data.endpoints {
        assert!(validator.is_valid(&serde_json::to_value(endpoint).unwrap()));
    }
    for (pointer, replacement) in [
        ("/protocolVersion", json!("loom.wall.v0")),
        ("/tiles/0/rotation", json!("deg45")),
        ("/revision", json!(9007199254740992_u64)),
        (
            "/placements/0/source",
            json!({"kind":"image", "id":"file:///private/image.png"}),
        ),
        ("/bounds/width", json!(0)),
    ] {
        let mut value = valid.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(!validator.is_valid(&value), "{pointer}");
        assert!(serde_json::from_value::<WallLayout>(value)
            .map(|layout| validate_wall_layout(&layout).is_err())
            .unwrap_or(true));
    }
    for pointer in [
        "",
        "/tiles/0",
        "/placements/0",
        "/placements/0/source",
        "/bounds",
    ] {
        let mut value = valid.clone();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert!(!validator.is_valid(&value), "{pointer}");
        assert!(
            serde_json::from_value::<WallLayout>(value).is_err(),
            "{pointer}"
        );
    }
}
