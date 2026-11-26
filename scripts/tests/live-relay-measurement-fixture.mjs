// Shared minimal live-session response fixture; private fields verify that sampled reports never copy raw responses.
export function snapshot(overrides = {}) {
  return {
    session: { sessionId: 'live:test', sourceDeviceId: 'source:a', sourceWindowIdentity: { title: 'PRIVATE_TITLE' } },
    epoch: 1, lastFrameId: 1, publishedFrames: 1, bufferedFrames: 1,
    sourceConnected: true, closed: false, viewerConnections: { 'viewer:a': 1 },
    observations: [{ value: 'PRIVATE_OCR' }],
    mediaDiagnostics: {
      receivedBinaryBytes: 100, sourceSequenceGaps: 0, bufferEvictions: 0,
      forwardedFrames: 1, forwardedBinaryBytes: 100, viewerSkippedFrames: 0, failedWrites: 0,
      lastForward: { viewerDeviceId: 'viewer:a', epoch: 1, frameId: 1, binaryBytes: 100,
        skippedFrames: 0, queueAgeMs: 12, adaptationMs: 3, socketWriteMs: 2,
        wireCodec: 'jpeg', writeSucceeded: true },
    }, ...overrides,
  };
}
