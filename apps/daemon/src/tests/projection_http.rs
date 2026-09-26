mod projection_http {
    include!("projection_fixture.rs");
    include!("projection_revocation.rs");
    include!("projection_delivery_tests.rs");
    include!("projection_delivery_state_tests.rs");
    include!("offline_peers_http.rs");
    include!("offline_catalog_http.rs");
    include!("offline_raster_http.rs");
    include!("offline_transfer_http.rs");
    include!("account_login_scope.rs");
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn projection_http_lifecycle_retries_and_restart_use_real_device_sessions() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let root = ProjectionRoot::new();
        let (port, mut server) = start(&root.0);
        let mut a = Identity::pair(port, "Projection A");
        let mut b = Identity::pair(port, "Projection B");
        let outsider = Identity::pair(port, "Projection outsider");
        let snapshot = png(10);
        let envelope = a.invitation(&snapshot, unix_time_millis() + 300_000);
        let create = json!({"envelope": envelope, "snapshot": snapshot});
        assert_eq!(
            public(port, "/v1/projections/create", create.clone()).0,
            401
        );
        assert_eq!(admin(port, "/v1/projections/create", create.clone()).0, 403);
        assert_eq!(a.post(port, "create", create.clone()).0, 200);
        assert_eq!(
            error_code(&a.post(port, "create", create)),
            "projection_invitation_replayed"
        );
        let read = json!({"projectionId": envelope.projection_id, "knownRevision": 1});
        assert_eq!(b.post(port, "read", read.clone()).0, 403);
        let (status, inspected) = b.post(port, "inspect", json!({"envelope": envelope}));
        assert_eq!(status, 200);
        assert_eq!(inspected["snapshot"]["imageBase64"], snapshot.image_base64);
        assert_eq!(inspected["sourceName"], "Projection A");
        let accept = acceptance(&envelope, "receiver");
        assert_eq!(b.post(port, "accept", accept.clone()).0, 200);
        assert_eq!(b.post(port, "accept", accept.clone()).0, 200);
        assert_eq!(
            b.post(port, "accept", acceptance(&envelope, "different-unit"))
                .0,
            409
        );
        assert_eq!(outsider.post(port, "accept", accept).0, 409);
        assert_eq!(outsider.post(port, "read", read.clone()).0, 403);
        for revision in 1..=2 {
            thread::sleep(Duration::from_millis(550));
            let update = update(&envelope, revision, &png(10 + revision as u8));
            assert_eq!(a.post(port, "update", update.clone()).0, 200);
            assert_eq!(a.post(port, "update", update.clone()).0, 200);
            assert_eq!(outsider.post(port, "update", update).0, 403);
        }
        let (status, received) = b.post(port, "read", read.clone());
        assert_eq!(status, 200);
        assert_eq!(received["revision"], 3);
        assert_eq!(received["snapshot"]["imageBase64"], png(12).image_base64);
        let current = json!({"projectionId": envelope.projection_id, "knownRevision": 3});
        assert!(b.post(port, "read", current).1["snapshot"].is_null());
        assert_eq!(
            a.post(port, "update", update(&envelope, 1, &png(99))).0,
            409
        );
        server.finish().unwrap();
        let (port, mut server) = start(&root.0);
        a.session(port);
        b.session(port);
        assert_eq!(b.post(port, "read", read.clone()).1["revision"], 3);
        let unlink = json!({"projectionId": envelope.projection_id});
        assert_eq!(b.post(port, "unlink", unlink.clone()).0, 200);
        assert_eq!(b.post(port, "unlink", unlink).0, 200);
        assert_eq!(
            error_code(&a.post(port, "read", read)),
            "projection_unlinked"
        );
        assert_eq!(
            a.post(port, "update", update(&envelope, 3, &png(99))).0,
            410
        );
        server.finish().unwrap();
    }

    #[test]
    fn projection_http_rejects_tampered_expired_and_invalid_image_invitations() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let root = ProjectionRoot::new();
        let (port, mut server) = start(&root.0);
        let a = Identity::pair(port, "Validation A");
        let snapshot = png(10);
        let envelope = a.invitation(&snapshot, unix_time_millis() + 300_000);
        let mut tampered = envelope.clone();
        tampered.server_origin = "https://other.example.test".to_owned();
        assert_eq!(
            error_code(&a.post(
                port,
                "create",
                json!({"envelope": tampered, "snapshot": snapshot})
            )),
            "projection_signature_invalid"
        );
        let expired = a.invitation(&snapshot, unix_time_millis() - 1);
        assert_eq!(
            error_code(&a.post(
                port,
                "create",
                json!({"envelope": expired, "snapshot": snapshot})
            )),
            "projection_invitation_expired"
        );
        assert_eq!(
            error_code(&a.post(
                port,
                "create",
                json!({"envelope": envelope, "snapshot": png(99)})
            )),
            "projection_digest_mismatch"
        );
        let mut wrong_size = snapshot.clone();
        wrong_size.width = 3;
        assert_eq!(
            error_code(&a.post(
                port,
                "create",
                json!({"envelope": envelope, "snapshot": wrong_size})
            )),
            "projection_dimensions_mismatch"
        );
        wrong_size.width = 8193;
        assert_eq!(
            error_code(&a.post(
                port,
                "create",
                json!({"envelope": envelope, "snapshot": wrong_size})
            )),
            "projection_image_budget"
        );
        assert_eq!(
            a.post(
                port,
                "create",
                json!({"envelope": envelope, "snapshot": snapshot, "extra": true})
            )
            .0,
            400
        );
        assert_eq!(
            a.post(
                port,
                "create",
                json!({"envelope": envelope, "snapshot": snapshot})
            )
            .0,
            200
        );
        assert_eq!(
            error_code(&a.post(port, "accept", acceptance(&envelope, "receiver"))),
            "projection_same_device"
        );
        server.finish().unwrap();
    }
}
