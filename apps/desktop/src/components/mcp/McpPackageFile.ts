// Bounds and encodes browser-selected MCP archives before they enter the daemon transport.

export const MAX_MCP_PACKAGE_FILE_BYTES = 64 * 1024 * 1024;

// A multiple of three keeps independently encoded chunks directly concatenable.
const BASE64_CHUNK_BYTES = 0x6000;

export function assertMcpPackageFileSize(size: number): void {
  if (!Number.isSafeInteger(size) || size < 0) {
    throw new Error("MCP 服务包大小无效。");
  }
  if (size > MAX_MCP_PACKAGE_FILE_BYTES) {
    throw new Error("MCP 服务包不能超过 64 MiB。");
  }
}

export function encodeMcpPackageBytes(bytes: Uint8Array): string {
  assertMcpPackageFileSize(bytes.byteLength);
  let encoded = "";
  for (let offset = 0; offset < bytes.length; offset += BASE64_CHUNK_BYTES) {
    const chunk = bytes.subarray(offset, offset + BASE64_CHUNK_BYTES);
    encoded += btoa(String.fromCharCode(...chunk));
  }
  return encoded;
}
