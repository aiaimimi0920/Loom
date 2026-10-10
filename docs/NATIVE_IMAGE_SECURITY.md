# Native image workflow resource boundaries

`loom_native_image` owns the six `core.image.*` workflow operators. The daemon
reaches them through `loom_workflow_runtime`; Hook supplies inputs but does not
choose resource limits. These operators accept PNG input and emit PNG data URLs.

Each job enforces these limits independently:

| Resource | Limit |
| --- | ---: |
| Compressed input | 16 MiB |
| Base64 input including optional data URL prefix | ceil(16 MiB / 3) * 4 + 128 bytes |
| Width or height | 8192 pixels |
| Total decoded pixels | 8,388,608 |
| Decoder allocation budget and decoded pixel bytes | 64 MiB |
| Encoded output before base64 | 64 MiB |
| Blur radius | integer 0 through 128 (zero retains the historical radius-one behavior) |
| Pixel block size | integer 0 through 8192 (zero retains the historical block-one behavior) |
| Cooperative processing deadline | earlier of caller deadline and 15 seconds |

Dimension and decoder allocation limits are installed before constructing the
PNG decoder. The pixel product and decoded byte count are checked before pixel
decode/allocation. `image::Limits::max_alloc` is a decoder budget, not a promise
about total process RSS: compressed input, source/output surfaces, codec scratch
storage, and base64 output are separate bounded allocations. Limits are per job,
not a global concurrent-work quota.

Blur uses sliding column sums and an unrounded box average, preserving edge and
alpha behavior while reducing work to O(pixels), independent of radius. Scratch
storage is O(width). Pixelation and blur check cancellation inside their loops;
codec reads/writes and stage boundaries also check cancellation/deadline. Other
linear transformations are bounded by the pixel limit and checked at stage
boundaries. These checks are cooperative, not OS-preemptive deadlines inside
third-party codec computations. No detached worker continues a timed-out job.

Invalid sizes and malformed or oversized blur/pixelation parameters fail with
the existing `ProcessResult` failure shape; numeric values are not truncated.
Workflow cancellation/timeout retain their existing typed errors. Direct
in-memory `process_image` calls enforce surface/parameter budgets too; the
low-level `apply_*` image helpers are trusted caller primitives, not the
untrusted encoded-input boundary.

Regression evidence lives in `crates/loom_native_image/tests/resource_limits.rs`
and the codec/blur unit tests, including oversized PNG headers, pixel-product
limits, bounded output writes, overflow values, cancellation during blur, and
pixel-exact comparisons against the original box-average definition.
