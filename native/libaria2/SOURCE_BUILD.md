# libaria2 Engine Source Bundle Build Instructions

The materials in this bundle correspond specifically to Loom's `libaria2` download-engine library (`loom_aria2.dll`), not to a complete combined Loom application distribution (`combinedApplicationSourceComplete` in `source-manifest.json` remains `false` unless accompanied by the full application source archive).

In an environment providing `bash`, `tar`, `make`, `pkg-config`, and the `x86_64-w64-mingw32` cross-compilation toolchain, run from the extracted bundle root:

```bash
bash ./build.sh /absolute/path/to/new-build /absolute/path/to/aria2-1.37.0.tar.xz /absolute/path/to/bridge.cpp
```

- The output directory must be a dedicated empty/new path outside user data directories.
- `build.sh` does not install system packages automatically; exact compiler versions are recorded in `compiler.txt`, input SHA-256 digests in `source-manifest.json`, and the resulting DLL digest and link map in `build-receipt.json`.
- Native compilation links static `libstdc++` / MinGW runtime components whose upstream source and license materials are tracked via `toolchain-sources.json` (`scripts/Get-LoomNativeSourceMaterials.ps1`).
- The upstream `aria2-1.37.0.tar.xz` archive is unmodified; configure flags and final shared-library link flags live in `build.sh`. This library performs bounded HTTP/HTTPS transfers only and never executes post-download scripts or package payloads.
