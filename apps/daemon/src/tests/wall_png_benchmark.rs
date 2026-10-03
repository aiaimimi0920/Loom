// Explicit local encode probe, not a physical-display or cross-device performance claim.
mod wall_png_benchmark {
    use super::*;

    #[test]
    #[ignore = "explicit synthetic 1/2/4-viewer encode comparison"]
    fn wall_png_same_profile_encode_cost() {
        let root = unique_temp_dir("wall-png-encode-cost");
        let walls = Arc::new(WallStore::open(&root).unwrap());
        let store = LiveSessionStore::new();
        store
            .create("device-source", live_start_envelope("live:png-bench"))
            .unwrap();
        let profile = WallMediaProfile {
            codec: WallMediaCodec::Png,
            width: 640,
            height: 360,
            fps: 10,
        };
        let mut frame_id = 0;
        for viewers in [1, 2, 4] {
            let mut samples = Vec::new();
            let mut output_bytes = 0;
            for phase in 0..12 {
                frame_id += 1;
                let mut pixels = vec![0_u8; 1280 * 720 * 4];
                for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
                    let x = i % 1280;
                    let y = i / 1280;
                    pixel.copy_from_slice(&[
                        (x / 8 + phase) as u8,
                        (y / 8) as u8,
                        ((x + y) / 16) as u8,
                        255,
                    ]);
                }
                let wire = LiveBinaryFrame {
                    epoch: 1,
                    metadata: loom_protocol::LiveFrameMetadata {
                        frame_id,
                        capture_timestamp_ms: frame_id,
                        encode_timestamp_ms: frame_id,
                        width: 1280,
                        height: 720,
                        keyframe: true,
                        dropped_frames: 0,
                        color_space: loom_protocol::LiveColorSpace::Srgb,
                        codec: loom_protocol::LiveCodec::RawBgra,
                    },
                    payload: pixels,
                }
                .encode()
                .unwrap();
                store
                    .publish_frame("live:png-bench", "device-source", wire)
                    .unwrap();
                let frame = store
                    .wait_for_frame("live:png-bench", 0, 0, Duration::ZERO)
                    .unwrap()
                    .unwrap();
                let started = Instant::now();
                for _ in 0..viewers {
                    let bytes = encode_wall_media_frame(&frame, &walls, profile)
                        .unwrap()
                        .unwrap();
                    let decoded = WallMediaFrame::decode(&bytes).unwrap();
                    assert_eq!(
                        (decoded.metadata.width, decoded.metadata.height),
                        (640, 360)
                    );
                    output_bytes += bytes.len();
                    std::hint::black_box(bytes);
                }
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "WALL_PNG_BENCHMARK {}",
                json!({
                    "scope": "synthetic-local-encode", "viewers": viewers, "frames": samples.len(),
                    "sourceSize": [1280, 720], "outputSize": [640, 360],
                    "meanMsPerSourceFrame": samples.iter().sum::<f64>() / samples.len() as f64,
                    "p50Ms": samples[samples.len() / 2], "maxMs": samples.last(), "outputBytes": output_bytes,
                    "physicalDisplayTested": false, "crossDeviceTested": false,
                })
            );
        }
        drop(walls);
        fs::remove_dir_all(root).unwrap();
    }
}
