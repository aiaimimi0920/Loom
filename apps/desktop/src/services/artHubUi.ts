export type ArtWorkspaceId = "registry" | "frameworks" | "store" | "security";

export interface ArtWorkspaceItem {
  id: ArtWorkspaceId;
  label: string;
  eyebrow: string;
}

export const artWorkspaceItems: ArtWorkspaceItem[] = [
  { id: "registry", label: "注册表", eyebrow: "LIBRARY / CREATE" },
  { id: "frameworks", label: "执行框架", eyebrow: "RUNTIME" },
  { id: "store", label: "商店", eyebrow: "INSTALL" },
  { id: "security", label: "信任与凭据", eyebrow: "SECURITY" },
];

export function nextArtWorkspaceIndex(
  key: string,
  currentIndex: number,
  workspaceCount: number,
): number | null {
  if (workspaceCount <= 0) return null;
  if (key === "ArrowRight") return (currentIndex + 1) % workspaceCount;
  if (key === "ArrowLeft") return (currentIndex - 1 + workspaceCount) % workspaceCount;
  if (key === "Home") return 0;
  if (key === "End") return workspaceCount - 1;
  return null;
}
