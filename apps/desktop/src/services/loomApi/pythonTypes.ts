// Python Art discovery, source, metadata, and inferred-port contracts.

export interface LoomPythonArt {
  path: string;
  art_json_path?: string;
  art_id: string;
  label: string;
  description?: string;
  version?: string;
  definition?: unknown;
}

export interface LoomPythonArtsResponse {
  arts?: LoomPythonArt[];
}

export interface LoomPythonSourceReadResponse {
  path: string;
  content: string;
  bytes?: number;
}

export interface LoomPythonArtJsonResponse {
  artJsonPath?: string;
  artJson?: unknown;
}

export interface LoomPythonNearbyArtJsonResponse extends LoomPythonArtJsonResponse {
  found: boolean;
  pythonPath?: string;
}

export interface LoomPythonPortDefinition {
  name: string;
  label?: string;
  type?: string;
  execution_type?: string;
  executionType?: string;
  default?: string;
}

export interface LoomPythonPortInferenceResponse {
  path?: string | null;
  inputs?: LoomPythonPortDefinition[];
  outputs?: LoomPythonPortDefinition[];
}
