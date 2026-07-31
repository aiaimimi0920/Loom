# Loom image-compress Art Pingo packaging plan

**Goal:** Make Loom's existing `图片压缩` Art a repo-owned, formally installable
`cli_wrapper` package backed by the official portable Windows x64 Pingo binary,
then verify it through real Loom/Hook execution and produce a new parent-scoped
release.

**Architecture:** Keep the existing production Art id
`custom-1770146354922`, rebuild it as a standard Loom Art ZIP containing
`manifest.json` plus `bin/pingo.exe`, publish that ZIP into the local art-store
root, and install it locally into `%APPDATA%\Loom\control-plane\arts\...` so
Hook/Loom continue using the normal installed-Art resolution path. Prefer
local/store installation over direct daemon ZIP upload because bundled binaries
can exceed the daemon's current safe body limit once base64 encoded.

**Tech Stack:** PowerShell 5.1, local Loom control-plane state, Loom daemon
HTTP compat API, Hook Bridge WebSocket API, official Pingo portable ZIP.

---

## File Map

Create:

```text
docs/progress/phase-55-image-compress-art-pingo.md
docs/superpowers/plans/2026-07-29-loom-image-compress-art-pingo.md
```

Modify:

```text
README.md
docs/progress/MASTER.md
scripts/Install-LoomImageCompressArt.ps1
```

---

## Tasks

- [x] **Task 1: Package the existing `图片压缩` node as a Loom-managed Art**
  - Download the official portable Pingo ZIP.
  - Bundle `pingo.exe` into a formal Loom Art package zip.
  - Preserve the existing Art id so current Hook/Loom nodes keep working.
  - Normalize the params to Pingo's real CLI contract.

- [x] **Task 2: Verify the Art through real runtime paths**
  - Reinstall/update the local Loom tool registry entry.
  - Confirm the daemon resolves the Loom-managed executable path.
  - Execute the Art successfully through:
    - `POST /v1/artloom-compat/ipc/execute-art-node`
    - Hook Bridge WebSocket `art/process`

- [x] **Task 3: Record the work and build a new release**
  - Update `README.md` with the rebuild/install command.
  - Record the phase in project progress docs.
  - Build a new parent-scoped release under
    `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom`.
