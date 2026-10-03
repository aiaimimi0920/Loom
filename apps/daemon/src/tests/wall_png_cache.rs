mod wall_png_cache_tests {
    use super::*;

    fn frame(id: u64) -> StoredLiveFrame {
        StoredLiveFrame {
            received_at: Instant::now(),
            epoch: 1,
            frame_id: id,
            bytes: Arc::new(encoded_live_frame(id)),
            legacy: Arc::new(Mutex::new(None)),
            wall_png: Arc::new(Mutex::new(WallPngFrameCache::default())),
        }
    }

    fn profile(width: u32) -> WallMediaProfile {
        WallMediaProfile {
            codec: WallMediaCodec::Png,
            width,
            height: 2,
            fps: 10,
        }
    }

    fn image(frame: &StoredLiveFrame, profile: WallMediaProfile) -> Arc<WallPngImage> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(image) = frame.wall_png_image(&frame.bytes, 2, 2, profile).unwrap() {
                return image;
            }
            assert!(
                Instant::now() < deadline,
                "bounded encode admission timed out"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn wall_png_same_profile_shares_pixels_not_per_viewer_timestamps() {
        let frame = frame(1);
        let first = image(&frame, profile(2));
        let clone = frame.clone();
        for fps in [1, 2, 4, 10] {
            let mut requested = profile(2);
            requested.fps = fps;
            assert!(Arc::ptr_eq(&first, &image(&clone, requested)));
        }
        let reserved = first._reservation.as_ref().unwrap();
        assert_eq!(reserved.bytes, first.bytes.capacity());
        let root = unique_temp_dir("wall-png-shared-times");
        let walls = Arc::new(WallStore::open(&root).unwrap());
        let a = encode_wall_media_frame(&frame, &walls, profile(2))
            .unwrap()
            .unwrap();
        thread::sleep(Duration::from_millis(3));
        let b = encode_wall_media_frame(&frame, &walls, profile(2))
            .unwrap()
            .unwrap();
        let a = WallMediaFrame::decode(&a).unwrap();
        let b = WallMediaFrame::decode(&b).unwrap();
        assert_eq!(a.payload, b.payload);
        assert_eq!(
            a.metadata.received_timestamp_ms,
            b.metadata.received_timestamp_ms
        );
        assert!(b.metadata.sent_timestamp_ms > a.metadata.sent_timestamp_ms);
        let pixels = image::load_from_memory_with_format(a.payload, image::ImageFormat::Png)
            .unwrap()
            .to_rgb8();
        assert_eq!((pixels.width(), pixels.height()), (2, 2));
        assert_eq!(pixels.as_raw(), &[1; 12]);
        drop(walls);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wall_png_cache_is_frame_owned_profile_bounded_and_releases_with_inflight_references() {
        let frame = frame(1);
        let first = image(&frame, profile(2));
        for width in 3..6 {
            let alternate = image(&frame, profile(width));
            assert!(!Arc::ptr_eq(&first, &alternate));
        }
        assert_eq!(
            frame.wall_png.lock().unwrap().slots.len(),
            WALL_PNG_FRAME_PROFILES
        );
        let overflow_a = image(&frame, profile(6));
        let overflow_b = image(&frame, profile(6));
        assert!(!Arc::ptr_eq(&overflow_a, &overflow_b));
        assert_eq!(
            frame.wall_png.lock().unwrap().slots.len(),
            WALL_PNG_FRAME_PROFILES
        );
        let mut other = frame.clone();
        other.wall_png = Arc::new(Mutex::new(WallPngFrameCache::default()));
        other.epoch = 2;
        assert!(!Arc::ptr_eq(&first, &image(&other, profile(2))));
        let weak_cache = Arc::downgrade(&frame.wall_png);
        let weak_pixels = Arc::downgrade(&first);
        drop(frame);
        assert!(weak_cache.upgrade().is_none());
        assert!(weak_pixels.upgrade().is_some());
        drop(first);
        assert!(weak_pixels.upgrade().is_none());
    }

    #[test]
    fn wall_png_busy_slot_does_not_block_other_profiles_or_direct_raw_jpeg() {
        let frame = frame(1);
        image(&frame, profile(2));
        let slot = Arc::clone(&frame.wall_png.lock().unwrap().slots[0].1);
        let _guard = slot.lock().unwrap();
        assert!(frame
            .wall_png_image(&frame.bytes, 2, 2, profile(2))
            .unwrap()
            .is_none());
        assert_eq!(
            (
                image(&frame, profile(1)).width,
                image(&frame, profile(1)).height
            ),
            (1, 1)
        );
        assert!(Arc::ptr_eq(
            &frame.bytes,
            &frame
                .representation(LiveMediaProfile::Jpeg)
                .unwrap()
                .unwrap()
        ));
        let root = unique_temp_dir("wall-png-busy-raw");
        let walls = Arc::new(WallStore::open(&root).unwrap());
        let raw = WallMediaProfile {
            codec: WallMediaCodec::RawBgra,
            ..profile(2)
        };
        let bytes = encode_wall_media_frame(&frame, &walls, raw)
            .unwrap()
            .unwrap();
        assert_eq!(
            WallMediaFrame::decode(&bytes).unwrap().payload,
            &frame.bytes[64..]
        );
        let cache_guard = frame.wall_png.lock().unwrap();
        assert!(frame
            .wall_png_image(&frame.bytes, 2, 2, profile(1))
            .unwrap()
            .is_none());
        drop(cache_guard);
        drop(walls);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wall_png_reservation_bounds_overflow_and_releases_only_when_owned_value_drops() {
        let retained = Arc::new(AtomicUsize::new(0));
        let first = WallPngReservation::acquire(&retained, 5, 8).unwrap();
        assert!(WallPngReservation::acquire(&retained, 4, 8).is_none());
        assert!(WallPngReservation::acquire(&retained, usize::MAX, 8).is_none());
        let second = WallPngReservation::acquire(&retained, 3, 8).unwrap();
        assert_eq!(retained.load(Ordering::SeqCst), 8);
        let owned = Arc::new(first);
        let in_flight = Arc::clone(&owned);
        drop(owned);
        assert_eq!(retained.load(Ordering::SeqCst), 8);
        drop(in_flight);
        assert_eq!(retained.load(Ordering::SeqCst), 3);
        drop(second);
        assert_eq!(retained.load(Ordering::SeqCst), 0);
        assert_eq!(
            encode_wall_png(&[], 2, 2, profile(2)).err(),
            Some("wall_source_dimensions_invalid")
        );
    }

    #[test]
    fn wall_png_cached_frame_cannot_extend_old_epoch_or_closed_source() {
        let store = LiveSessionStore::new();
        store
            .create("device-source", live_start_envelope("live:png-auth"))
            .unwrap();
        store
            .set_media_connected(
                "live:png-auth",
                "device-source",
                LiveDeviceRole::Source,
                true,
            )
            .unwrap();
        store
            .publish_frame("live:png-auth", "device-source", encoded_live_frame(1))
            .unwrap();
        let frame = store
            .wait_for_frame("live:png-auth", 0, 0, Duration::ZERO)
            .unwrap()
            .unwrap();
        image(&frame, profile(2));
        assert!(store.wall_source_active_at_epoch("live:png-auth", Some(1)));
        store
            .state
            .lock()
            .unwrap()
            .get_mut("live:png-auth")
            .unwrap()
            .epoch = 2;
        assert!(!store.wall_source_active_at_epoch("live:png-auth", Some(frame.epoch)));
        store
            .state
            .lock()
            .unwrap()
            .get_mut("live:png-auth")
            .unwrap()
            .closed = true;
        assert!(!store.wall_source_active("live:png-auth"));
        assert!(image(&frame, profile(2)).bytes.len() > 0);
    }
}
