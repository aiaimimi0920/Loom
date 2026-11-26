# Neuro UI Integration (`@neuro/ui`)

Loom's React presentation layer (`apps/desktop`) consumes the shared `@neuro/ui` component library from the `NeuroDesign` repository without copying design-system component source into Loom. Rust crates, `loom-daemon`, Tauri IPC, API clients, and task lifecycles remain owned by Loom.

## Local layout and synchronization

In a local multi-repo workspace where Loom is checked out at `C:\Users\Public\Project\Loom` and `NeuroDesign` is checked out at `C:\Users\Public\nas_home\NeuroDesign`, `apps/desktop/package.json` references:

```text
file:../../../../nas_home/NeuroDesign/packages/react/neuro-ui
```

When setting up a new machine or updating the relative checkout path, build `@neuro/ui` in `NeuroDesign` first, then link and synchronize inside `apps/desktop`:

```powershell
npx --yes npm@11 install --install-links --ignore-scripts --prefer-offline --no-audit --no-fund @neuro/ui@file:../../../../nas_home/NeuroDesign/packages/react/neuro-ui
npx --yes npm@11 run ui:sync
npm run typecheck
npm test
```

`.npmrc` keeps `install-links=true` so npm copies the built package artifacts rather than creating cross-drive symlinks. `npm run ui:sync` rebuilds only `@neuro/ui`, refreshes the installed package copy when build outputs or metadata change, and verifies single-instance `react` / `react-dom` resolution. Do not run concurrent `npm install`, `npm test`, or `npm run build` commands in `apps/desktop` at the same time. Compiled Loom desktop and web bundles are self-contained and do not read `NeuroDesign` at runtime.

## Clean CI checkouts

The four desktop UI workflow jobs (`ci.yml/windows`, `build-windows.yml/build`, `release-tag.yml/release`, and `compile-windows.yml/compile`) check out Loom at `Project/Loom` and `aiaimimi0920/NeuroDesign` at `nas_home/NeuroDesign` (pinned to an exact reviewed commit SHA with `persist-credentials: false`).

Using Node.js `22.22.2` and npm `11.21.1`, each UI job runs `npm ci --no-audit --no-fund` and `node scripts/build.mjs --library --theme neuro` inside `NeuroDesign` before installing `apps/desktop`. Non-UI jobs keep standalone root checkouts. `scripts/tests/neuro-ui-ci-layout.test.cjs` (invoked by `Test-GitHubActionsContract.ps1`) verifies the commit pin, `file:` resolution, build ordering, and workflow artifact paths.

## Ownership boundaries between Loom and `@neuro/ui`

- **Theme and tokens**: `LoomUiProvider` tracks `data-loom-theme` and OS color-scheme changes while `.neuro-root` maps application semantic variables onto `--n-*` tokens (`--n-mono` for monospace, standard focus rings, `prefers-reduced-motion`, and `forced-colors` handled by `@neuro/ui`). The Otter canvas retains its dedicated dark-surface and high-contrast edge rules.
- **Display language and localization**: `apps/desktop/src/i18n/` (`DisplayLanguage`, `Catalogs`, `uiText`, `uiFormat`, and `useUiText`) owns compile-time domain-sharded UI copy under `apps/desktop/src/locales/en/` (source of truth and fallback) and `apps/desktop/src/locales/zh-Hans/`. Target language switching (`en` / `zh-Hans`, default `en`) updates active components synchronously without restarting the desktop shell or daemon.
- **Dialogs, feedback, and focus**: `AppFeedback` owns notification queuing and Promise-based confirmation lifecycles; `@neuro/ui` `Dialog` and `ConfirmDialog` own focus trapping, `Escape` handling, scroll locking, and focus restoration. Busy dialogs block dismissal until in-flight mutations settle, and stale async responses from closed or switched sessions are invalidated before mutating UI state.
- **Domain controllers stay in Loom**:
  - **MCP Hub**: `@neuro/ui` `Tabs`, `Panel`, `Badge`, `Dialog`, `Field`, `Notice`, and `EmptyState` render service and store views, while `McpHub` owns server enable/disable/test/install state, URL and argument parsing, and write-only `CredentialStore` bindings.
  - **Frameworks, Capabilities, and Arts**: `DataTable`, `Dialog`, `MultiSelect` (inline up to six options, searchable dropdown above six), and form primitives render package catalogs, permission-approval dialogs, the four-mode Art creation wizard, and Art parameter/binding editors. Package validation, digest-bound permission grants, `valueBindings` / `credentialBindings`, and sequential save-then-upgrade ordering remain in Loom hooks (`useArtScriptSource`, `useArtMcpDiscovery`, `useArtWorkflowInterface`, `useArtCreationSubmission`, `artEditModel`).
  - **Devices, Connections, and Tile Walls**: standard inputs, switches, segmented radios, disclosures, and tables render paired devices, LAN/peer/Cloud Relay settings, projection rules, and Tile Wall editors, while `useManagedDevicesController` and wall controllers own polling, CAS revisions, connection-code omission/clearing, and scene presentation commands.
  - **Workflow Canvas and App Shell**: `NavigationItem` and `IconButton` drive sidebar navigation, responsive collapse state, and window controls; native key-chord capture (`ShortcutSlotsEditor`), hidden `<input type="file">` pickers, and interactive `OtterCanvasSurface` / `OtterCanvasNode` viewport gestures remain custom Loom components.
