# Interactive wall protocol v1

Status: implementation in progress. The geometry contract below is implemented;
registration and persistence are documented in [WALL_CONTROL_API.md](WALL_CONTROL_API.md).
Presentation and input routing have separate acceptance
tasks in [the implementation plan](../docs/TILE_WALL_IMPLEMENTATION_PLAN.md).
This document does not assert completed multi-terminal runtime support.

## Ownership and compatibility

`loom.wall.v1` describes physical display endpoints and content placement. It is
independent of application sessions. Loom owns layout and input authorization;
Hook implements a terminal today. Future terminals advertise actual display and
input capabilities instead of requiring a general-purpose CPU, GPU or web runtime.
The management UI is optional at runtime and is not part of the output image.

An endpoint binds an opaque `endpointId` to one paired `deviceId` and one stable
`outputId`. A tile places that endpoint in a wall. A placement references an
existing Live session, Surface instance or content-addressed image. Moving,
duplicating or removing a placement does not create or destroy the source.

The wire schema is [wall.v1.schema.json](schemas/wall.v1.schema.json). Wall v1
rejects unknown fields and enum values, including nested objects. This strict
contract overrides the general optional-field rule in the protocol index. New
wire fields require explicit protocol evolution. The separately revisioned
[presentation-control extension](WALL_PRESENTATION_API.md) documents its exact
optional state/heartbeat additions without changing this geometry schema.
The [physical identification extension](WALL_IDENTIFICATION_API.md) adds optional
endpoint `display` metadata and a bounded, lease-owned marker with its own report.
The [timing/media extension](WALL_TIMING_MEDIA.md) adds optional
`scheduledPresentation`, a boot clock and scene receipts, plus an independent
NLWM media subprotocol. These extensions require coordinated upgrades of strict readers.
IDs are opaque ASCII strings
of 1-160 characters; accepting `/` in an identity does not permit using it as a
filesystem path. Images accept only lowercase `sha256:<64 hex digits>` IDs.

## Geometry and limits

All positions use finite IEEE-754 binary64 numbers. Wall coordinates are logical
units, independent of device pixel density, with top-left origin, x right and y
down. Negative origins are allowed. Rectangles have positive dimensions and use
half-open containment: left/top included, right/bottom excluded. Rectangles and
their ends stay within absolute coordinate 1,000,000; each extent is at least
`1 / 65536`. This lower bound preserves pixel-center precision at the supported
coordinate and resolution limits.

An endpoint has native pixel dimensions between 1 and 16,384 per axis, at least
one distinct render mode, and zero or more distinct input capabilities. Render
modes are `raw_bgra`, `h264`, `image`, `surface_v1`; input capabilities are
`pointer`, `wheel`, `keyboard`, `text`, `touch`, `pen`. These values describe
capabilities, not authorization or measured performance. An implementation must
not advertise a mode it cannot consume.

A layout has at most 64 tiles and 256 placements. Tile IDs, endpoint IDs and
placement IDs are unique within their respective collections. Tiles are inside
the wall bounds and cannot overlap with positive area. Gaps and touching seams
are allowed. Cross-wall endpoint membership is a control-plane constraint.
Placements can overlap or extend beyond the wall. `sourceCrop` is a positive
normalized source rectangle fully inside `[0, 1] x [0, 1]`.

`revision` is an integer from 1 through `9007199254740991`, shared by Rust and
JavaScript. A geometry snapshot is immutable while rendering or processing input.
Input using another revision is rejected; the caller must resynchronize, not
retry the old event against a new mapping.

## Projection and input inversion

`tile.rect` is the final oriented footprint on the wall. `rotation` is the
clockwise rotation from native endpoint coordinates to that footprint, one of
`deg0`, `deg90`, `deg180`, `deg270`. A 90-degree rotation does not implicitly
swap the declared wall width and height.

Input adapters convert local pointer coordinates into native physical pixel
indices first (accounting for window origin and DPI exactly once). A native
integer pixel `(x, y)` samples its center:

```text
u = (x + 0.5) / pixelWidth
v = (y + 0.5) / pixelHeight
rotation: 0 -> (u, v); 90 -> (1-v, u); 180 -> (1-u, 1-v); 270 -> (v, 1-u)
wall = tile.origin + rotated * tile.extent
source = sourceCrop.origin + ((wall - placement.origin) / placement.extent) * sourceCrop.extent
```

Pixel-center sampling keeps rotated edge pixels inside the half-open footprint.
Inverse mapping uses inverse rotation followed by floor to a native pixel index.
An exact wall seam has one owning tile. NaN, infinity, negative, fractional or
out-of-bounds native pixel indices are invalid, never silently clamped into an
unrelated target.

For rendering, intersect each placement with the tile and compute the matching
source crop. `outputQuad` contains the wall intersection's top-left, top-right,
bottom-right and bottom-left corners transformed into normalized native output
coordinates. Unlike input samples, projection boundaries can equal 1. Terminal
rendering must preserve this order to reproduce rotation without mirroring.

Painting is ascending `(zIndex, placementId)`; the second key uses ASCII byte
order, not locale collation. Hit testing considers the topmost containing
placement in the same order. A noninteractive foreground blocks click-through.
Images are noninteractive source content in v1; layout editing remains a manager
operation. A successful hit only selects a target: it does not grant Live control
or bypass Surface action/attachment authorization.

## Conformance evidence

[wall-geometry.v1.json](fixtures/wall-geometry.v1.json) supplies independent
expected points and projections for two unequal-density endpoints, a rotated
tile, a negative wall origin, a seam and a cropped source spanning both tiles.
Rust runs it in `loom_protocol::wall::tests`. Hook keeps a byte-identical public
fixture at `__tests__/fixtures/wall/wall-geometry.v1.json` and runs
`__tests__/unit/WallGeometry.test.ts`, including standalone checkouts. Any fixture
change must update both copies and verify their SHA-256 equality before release.

Schema validates structural constraints; runtime validators additionally check
finite extents, containment, overlap, unique identities and normalized crops.
Conformance includes rejected inputs as well as accepted projections. Software
geometry tests do not establish physical display synchronization or latency.
