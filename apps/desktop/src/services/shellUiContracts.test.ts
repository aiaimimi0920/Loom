import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const app = read("../App.tsx");
const shell = read("../components/app/appShell.tsx");
const shellStyles = read("../styles/shell.css");
const responsive = read("../styles/responsive.css");
const uiStyles = readFileSync(new URL(import.meta.resolve("@neuro/ui/styles.css")), "utf8");

test("shell navigation uses the standard current-page contract without decorative eyebrows", () => {
  assert.match(app, /NavigationItem as NeuroNavigationItem/);
  assert.match(app, /<NeuroNavigationItem\s+className="rail-item"\s+active=\{activeSection === item.id\}/);
  assert.match(
    app,
    /<NeuroNavigationItem\s+className="rail-item rail-device-button"\s+active=\{activeSection === "devices"\}/,
  );
  assert.match(app, /onClick=\{\(\) => setActiveSection\(item.id\)\}/);
  assert.doesNotMatch(app, /<button\b|rail-item--active|eyebrow|section-kicker/);
  assert.doesNotMatch(shell, /eyebrow/);
  assert.match(uiStyles, /\.n-navigation-item\[aria-current="page"\][^{]*\{[^}]*var\(--n-signal-soft\)/);
});

test("shell refresh keeps a stable name and delegates loading to the standard button", () => {
  assert.match(
    app,
    /<IconButton\s+className="loom-window-control loom-window-control--refresh"[\s\S]*?label=\{t\("common\.refresh"\)\}[\s\S]*?loading=\{loading\}[\s\S]*?onClick=\{\(\) => void refresh\(\)\}/,
  );
  assert.doesNotMatch(app, /window-control--loading/);
  assert.doesNotMatch(
    shellStyles,
    /shell-refresh-spin|\.window-control(?::|\s*\{)|\.rail-item--active|\.rail-item:focus/,
  );
  assert.match(shellStyles, /\.rail-item__icon\s*\{[^}]*color: inherit;/);
});

test("native window commands and the settings return still belong to Loom", () => {
  for (const command of ["minimize", "toggle-maximize", "close"]) {
    assert.match(app, new RegExp(`onClick=\\{\\(\\) => void runWindowCommand\\("${command}"\\)\\}`));
  }
  assert.match(app, /data-tauri-drag-region\s+onDoubleClick=\{\(\) => void runWindowCommand\("toggle-maximize"\)\}/);
  assert.match(
    app,
    /label=\{t\("shell\.titlebar\.backToMcp"\)\}[\s\S]*?onDoubleClick=\{\(event\) => event.stopPropagation\(\)\}[\s\S]*?onClick=\{\(\) => setActiveSection\("mcp"\)\}/,
  );
});

test("responsive collapse has one state owner, permits manual expansion and releases its listener", () => {
  assert.match(app, /\(\) => window.matchMedia\("\(max-width: 820px\)"\).matches/);
  assert.match(app, /query.addEventListener\("change", updateRail\)/);
  assert.match(app, /return \(\) => query.removeEventListener\("change", updateRail\)/);
  assert.match(app, /aria-expanded=\{!railCollapsed\}/);
  assert.match(app, /setRailCollapsed\(\(collapsed\) => !collapsed\)/);
  assert.doesNotMatch(responsive, /--shell-rail-width|rail-item__label|shell-rail-toggle__mark/);
});
