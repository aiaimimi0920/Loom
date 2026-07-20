export type PythonArtPortType = "image" | "file" | "int" | "float" | "string" | "boolean";

export interface PythonArtPort {
  name: string;
  label: string;
  type: PythonArtPortType;
  execution_type: string;
  executionType: string;
  default?: string;
}

export interface PythonArtPortInference {
  inputs: PythonArtPort[];
  outputs: PythonArtPort[];
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const asString = (value: unknown) => (typeof value === "string" ? value : undefined);

const inferTypeFromName = (name: string): { uiType: PythonArtPortType; execType: string } => {
  if (/path|image|file|input|output|source|reference|result/i.test(name)) {
    return { uiType: "image", execType: "image_path" };
  }
  if (/factor|ratio|strength|alpha|blend|scale/i.test(name)) {
    return { uiType: "float", execType: "number" };
  }
  if (/count|num|size|clusters|width|height|n_/i.test(name)) {
    return { uiType: "int", execType: "number" };
  }
  return { uiType: "string", execType: "string" };
};

const toPort = (name: string): PythonArtPort => {
  const types = inferTypeFromName(name);
  return {
    name,
    label: name,
    type: types.uiType,
    execution_type: types.execType,
    executionType: types.execType,
  };
};

export function inferPortsFromPythonCode(code: string): PythonArtPortInference {
  const inputs: PythonArtPort[] = [];
  const outputs: PythonArtPort[] = [];
  const seenInputs = new Set<string>();
  const seenOutputs = new Set<string>();

  const inputPatterns = [
    /args\.get\(\s*["'](\w+)["']/g,
    /args\[["'](\w+)["']\]/g,
  ];

  for (const pattern of inputPatterns) {
    let match: RegExpExecArray | null;
    while ((match = pattern.exec(code)) !== null) {
      const name = match[1];
      if (!seenInputs.has(name)) {
        seenInputs.add(name);
        inputs.push(toPort(name));
      }
    }
  }

  const returnMatch = code.match(/return\s*\{([^}]+)\}/);
  if (returnMatch) {
    const keyPattern = /["'](\w+)["']\s*:/g;
    let match: RegExpExecArray | null;
    while ((match = keyPattern.exec(returnMatch[1])) !== null) {
      const name = match[1];
      if (!seenOutputs.has(name)) {
        seenOutputs.add(name);
        outputs.push(toPort(name));
      }
    }
  }

  return { inputs, outputs };
}

const normalizeArtJsonPort = (value: unknown, fallbackType: "input" | "output"): PythonArtPort | null => {
  if (!isRecord(value)) return null;
  const name = asString(value.id) || asString(value.name);
  if (!name) return null;
  const rawType = (asString(value.type) || "").toLowerCase();
  const imageLike = rawType === "image" || rawType === "image_path" || /image|path|file/i.test(name);
  return {
    name,
    label: asString(value.label) || name,
    type: imageLike ? "image" : fallbackType === "input" ? inferTypeFromName(name).uiType : "string",
    execution_type: imageLike ? "image_path" : fallbackType === "input" ? inferTypeFromName(name).execType : "string",
    executionType: imageLike ? "image_path" : fallbackType === "input" ? inferTypeFromName(name).execType : "string",
  };
};

const normalizeArtJsonVariable = (value: unknown): PythonArtPort | null => {
  if (!isRecord(value)) return null;
  const name = asString(value.id) || asString(value.name);
  if (!name) return null;
  const widget = asString(value.widget);
  const inferred = inferTypeFromName(name);
  const numeric = widget === "slider" || widget === "number" || inferred.execType === "number";
  return {
    name,
    label: asString(value.label) || name,
    type: numeric ? inferred.uiType === "int" ? "int" : "float" : inferred.uiType,
    execution_type: numeric ? "number" : inferred.execType,
    executionType: numeric ? "number" : inferred.execType,
    default: value.default === undefined ? undefined : String(value.default),
  };
};

export function mapArtJsonPorts(artJson: unknown): PythonArtPortInference {
  if (!isRecord(artJson)) return { inputs: [], outputs: [] };
  const signature = isRecord(artJson.signature) ? artJson.signature : {};
  const signatureInputs = Array.isArray(signature.inputs) ? signature.inputs : [];
  const signatureOutputs = Array.isArray(signature.outputs) ? signature.outputs : [];
  const variables = Array.isArray(artJson.variables) ? artJson.variables : [];

  return {
    inputs: [
      ...signatureInputs.map((input) => normalizeArtJsonPort(input, "input")).filter((port): port is PythonArtPort => Boolean(port)),
      ...variables.map(normalizeArtJsonVariable).filter((port): port is PythonArtPort => Boolean(port)),
    ],
    outputs: signatureOutputs
      .map((output) => normalizeArtJsonPort(output, "output"))
      .filter((port): port is PythonArtPort => Boolean(port)),
  };
}
