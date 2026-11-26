# Otter LAN Connection Protocol

Loom's **Otter Connection** settings expose an optional, dedicated LAN HTTPS listener (`apps/daemon/src/lan_listener.rs` and `apps/daemon/src/lan_http*.rs`). It is disabled by default (default port `8766`) and never exposes the local loopback management HTTP listener (`127.0.0.1:8765`) to the network.

## Operator workflow and approval policies

1. On the Loom host, enable **Allow LAN connections**, select a reachable local RFC1918 IPv4 address and TCP port, and click **Apply**. Enabling the listener also controls LAN UDP discovery broadcasts on that interface.
2. Open Otter's Loom Connection dialog, select the discovered Loom hub, enter the connection password if required (minimum 8 bytes when enabled), and submit the request.
3. Passing the password gate submits the request to Loom's configured approval policy: **Auto-approve on LAN**, **Auto-approve previously connected devices**, or **Manual approval**. Requests that do not qualify for automatic approval wait for manual administrator approval rather than failing immediately.
4. Both hosts must have private-network reachability: allow inbound TCP on the chosen LAN port (e.g., `8766`) and UDP discovery broadcasts (`49779`, 1-hop broadcast every 2 seconds) on private/LocalSubnet firewall profiles. Loom never modifies OS firewall rules automatically.

## Trust on first use (TOFU) and listener lifecycle

- UDP discovery packets carry Loom's public identity and public TLS certificate—never connection passwords, password hashes, admin bearer tokens, or private keys. Otter requires the broadcast source IP to match the advertised host and treats discovery strictly as an unauthenticated hint.
- Pinning occurs only when the user explicitly selects a target (TOFU): Otter validates certificate subject/SAN, validity window, signature, and Loom ID before pinning that certificate to the origin without installing a system root CA. Subsequent broadcasts cannot overwrite a pinned identity or certificate.
- Listener configuration and private keys persist atomically in `<control-plane>/settings/loom-lan.json`. Changing the bound IP regenerates the leaf certificate. Disabling the LAN listener closes active sockets, clears pending LAN connection requests, and revokes LAN-admitted sessions; Otter observes disconnection on its next lease renewal (5-second interval, 8-second HTTP timeout).

## Exposed route allowlists and resource bounds

The LAN listener binds strictly to RFC1918 IPv4 addresses, accepts only private-network peers over TLS 1.3, and proxies an exact route allowlist (never open path prefixes, administrator APIs, browser `Origin`/`Cookie` requests, `Bearer` admin tokens, `GET` bodies, or redirects):

- **`lan_http` (Core, Local Art, & Projection)**: public identity (`/v1/otter/identity`), connection request/status/cancel/activate, device pairing/challenges/sessions, Local Art issuer/describe/prepare-package v3/resource/service/live-viewer/live-trigger routes, and shared/offline/QR Projection v2 + signed peer transfer endpoints.
- **`lan_http_live` (Live Screenshot)**: session status/directory/create, single-session read/events, and exact viewer/resume/control/input/observation/trigger/close routes. WebSocket upgrades allow only `GET /v1/live/media` with canonical `sessionId`/`role`/cursor parameters verified via `Device` session + nonce before handing the TLS stream to the dedicated media worker pool.
- **`lan_http_surface` (Remote Surface)**: allows only `POST /v1/surfaces/attach`, `POST /v1/surfaces/instances/{raw-id}/lifecycle`, and `GET /v1/surfaces/stream` (`after` and `timeoutMs <= 5000`, 16 KiB JSON request cap) with `Device` + nonce authentication. Surface CRUD administration, arbitrary mounts, and loopback shared memory are never exposed on LAN.
- **`lan_http_wall` (Tile Wall)**: allows only exact paired-device Wall state, endpoint register/connect/heartbeat/disconnect/identify-report, image read, `GET /v1/walls/live/media` (`loom.wall.media.v1`), and ephemeral Wall Surface open/state/image/close/event/confirmation/cancel routes.

Short HTTP connections process one request per connection across at most **32** concurrent workers with no unbounded backlog: headers are capped at 16 KiB / 48 entries, responses at 64 MiB, default bodies at 1 MiB (or the target route's explicit cap). TLS handshake and request reads share a 5-second deadline; loopback forwarding uses 8 seconds by default and 65 seconds for `/v1/ai/chat/completions`, `/v1/surfaces/stream`, and routed paths ending in `/events`. Response writes have a 3-second deadline. Upgraded media WebSockets use a separate bounded worker pool.

Run the LAN regression suite with:

```powershell
cargo test --locked -p loom-daemon lan_ --lib
```
