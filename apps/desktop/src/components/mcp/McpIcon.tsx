// Renders the small icon vocabulary shared by MCP cards and dialogs.

export type McpIconKind = "plug" | "edit" | "power" | "trash" | "test" | "close" | "external";

export function McpIcon({ kind }: { kind: McpIconKind }) {
  const props = {
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };

  switch (kind) {
    case "edit":
      return <svg {...props}><path d="M4 20h4l11-11a2.8 2.8 0 0 0-4-4L4 16v4Z" /><path d="m13.5 6.5 4 4" /></svg>;
    case "power":
      return <svg {...props}><path d="M12 3v9" /><path d="M7.1 5.8a8 8 0 1 0 9.8 0" /></svg>;
    case "trash":
      return <svg {...props}><path d="M4 7h16M9 7V4h6v3M7 7l1 13h8l1-13M10 11v5M14 11v5" /></svg>;
    case "test":
      return <svg {...props}><path d="M5 12h14" /><path d="m13 6 6 6-6 6" /><circle cx="6" cy="12" r="2" /></svg>;
    case "close":
      return <svg {...props}><path d="m6 6 12 12M18 6 6 18" /></svg>;
    case "external":
      return <svg {...props}><path d="M14 4h6v6M20 4l-9 9" /><path d="M18 13v6a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h6" /></svg>;
    default:
      return <svg {...props}><path d="M8 3v5M16 3v5M6 8h12v2a6 6 0 0 1-6 6v5M9 21h6" /></svg>;
  }
}
