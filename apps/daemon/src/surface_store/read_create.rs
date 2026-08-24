// Store loading, read projections, instance creation, deletion, and package migration.
impl SurfaceInstanceStore {
    pub(crate) fn new(path: impl AsRef<Path>) -> Result<Self, SurfaceStoreError> {
        let path = path.as_ref().to_path_buf();
        let stored = match read_surface_store_bytes(&path) {
            Ok(bytes) => {
                if bytes.len() > MAX_SURFACE_STORE_BYTES {
                    return Err(invalid_persisted_store(format!(
                        "document exceeds the {MAX_SURFACE_STORE_BYTES} byte limit"
                    )));
                }
                if !json_nesting_is_within_parse_limit(&bytes, MAX_SURFACE_STORE_JSON_DEPTH) {
                    return Err(invalid_persisted_store(format!(
                        "document exceeds the nesting limit of {MAX_SURFACE_STORE_JSON_DEPTH} levels"
                    )));
                }
                let root = serde_json::from_slice::<Value>(&bytes)?;
                if !loom_security::json::value_is_within_depth(&root, MAX_SURFACE_STORE_JSON_DEPTH)
                {
                    return Err(invalid_persisted_store(format!(
                        "document exceeds the nesting limit of {MAX_SURFACE_STORE_JSON_DEPTH} levels"
                    )));
                }
                let document = serde_json::from_value::<SurfaceStoreDocument>(root)?;
                if document.schema_version != SURFACE_STORE_SCHEMA_VERSION {
                    return Err(SurfaceStoreError::UnsupportedSchema(
                        document.schema_version,
                    ));
                }
                let instances = document.instances;
                validate_loaded_instances(&instances)?;
                Some(instances)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        // A store that found no file leaves `persisted` unset, so its first persist writes the file
        // even if the projection happens to be empty. A store that loaded one records what that file
        // is expected to contain, so an opening run of mutations that changes nothing persistent
        // writes nothing.
        let (instances, persisted) = match stored {
            Some(instances) => {
                let bytes = document_bytes(&instances)?;
                (instances, Some(bytes))
            }
            None => (BTreeMap::new(), None),
        };
        Ok(Self {
            path,
            instances,
            persisted,
        })
    }

    pub(crate) fn list(&self) -> Vec<SurfaceInstanceRecord> {
        self.instances.values().cloned().collect()
    }

    pub(crate) fn get(&self, instance_id: &str) -> Option<SurfaceInstanceRecord> {
        self.instances.get(instance_id).cloned()
    }

    /// Returns only the locked package descriptor of an instance.
    ///
    /// `get` clones the whole record, including its pending events, acks and authoritative state. A
    /// caller that only needs to know which Art package an instance is locked to — so that it can
    /// resolve that package with this lock released — should not pay for the rest.
    pub(crate) fn descriptor(&self, instance_id: &str) -> Option<SurfaceInstanceDescriptor> {
        self.instances
            .get(instance_id)
            .map(|instance| instance.descriptor.clone())
    }

    pub(crate) fn event_ack(&self, instance_id: &str, event_id: &str) -> Option<SurfaceActionAck> {
        self.instances
            .get(instance_id)
            .and_then(|instance| instance.event_acks.get(event_id))
            .cloned()
    }

    pub(crate) fn pending_events(&self) -> Vec<SurfaceEvent> {
        self.instances
            .values()
            .flat_map(|instance| instance.pending_events.iter().cloned())
            .collect()
    }

    pub(crate) fn pending_confirmations(&self) -> Vec<SurfaceConfirmationRequest> {
        self.instances
            .values()
            .flat_map(|instance| {
                instance
                    .pending_confirmations
                    .values()
                    .map(|pending| pending.request.clone())
            })
            .collect()
    }

    pub(crate) fn create(
        &mut self,
        art_id: &str,
        art_version: &str,
        package_digest: &str,
        state_schema_version: u32,
        persistence: SurfaceInstancePersistence,
        instance_mode: SurfaceInstanceMode,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_identity(art_id, "Art id")?;
        Version::parse(art_version).map_err(|error| {
            SurfaceStoreError::Invalid(format!("Art version is not valid semver: {error}"))
        })?;
        let package_digest = normalize_package_digest(package_digest)?;
        let now = unix_time_millis();
        let instance_id = format!("instance:{}", Uuid::new_v4());
        let record = SurfaceInstanceRecord {
            descriptor: SurfaceInstanceDescriptor {
                instance_id: instance_id.clone(),
                art_id: art_id.to_owned(),
                art_version: art_version.to_owned(),
                package_digest,
                instance_mode,
                state_schema_version,
                persistence,
                generation: 0,
                surface_revision: 0,
                preview_revision: 0,
                result_revision: 0,
            },
            attachments: BTreeMap::new(),
            authoritative_state: Value::Object(Default::default()),
            latest_preview: None,
            latest_result: None,
            last_failure: None,
            pending_events: Vec::new(),
            event_acks: BTreeMap::new(),
            pending_confirmations: BTreeMap::new(),
            migration_history: Vec::new(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.transaction(|instances| {
            instances.insert(instance_id, record.clone());
            Ok(record)
        })
    }

    pub(crate) fn find_shared(
        &self,
        art_id: &str,
        art_version: &str,
        package_digest: &str,
        persistence: &SurfaceInstancePersistence,
    ) -> Option<SurfaceInstanceRecord> {
        self.instances
            .values()
            .find(|instance| {
                instance.descriptor.instance_mode == SurfaceInstanceMode::Shared
                    && instance.descriptor.art_id == art_id
                    && instance.descriptor.art_version == art_version
                    && instance.descriptor.package_digest == package_digest
                    && &instance.descriptor.persistence == persistence
            })
            .cloned()
    }

    pub(crate) fn delete(&mut self, instance_id: &str) -> Result<(), SurfaceStoreError> {
        self.transaction(|instances| {
            instances
                .remove(instance_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))?;
            Ok(())
        })
    }

    pub(crate) fn migrate_instance(
        &mut self,
        instance_id: &str,
        expected_generation: Option<u64>,
        target_version: &str,
        target_digest: &str,
        target_state_schema_version: u32,
        migrated_state: Value,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        Version::parse(target_version).map_err(|error| {
            SurfaceStoreError::Invalid(format!("target Art version is not valid semver: {error}"))
        })?;
        let target_digest = normalize_package_digest(target_digest)?;
        if target_state_schema_version == 0 {
            return Err(SurfaceStoreError::Invalid(
                "target state schema version must be at least 1".to_owned(),
            ));
        }
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if expected_generation
                .is_some_and(|expected| expected != instance.descriptor.generation)
            {
                return Err(SurfaceStoreError::Conflict(format!(
                    "expected generation {} but current generation is {}",
                    expected_generation.unwrap_or_default(),
                    instance.descriptor.generation
                )));
            }
            if !instance.pending_events.is_empty() {
                return Err(SurfaceStoreError::Conflict(
                    "Surface instance has pending actions and cannot migrate".to_owned(),
                ));
            }
            if instance.descriptor.art_version == target_version
                && instance.descriptor.package_digest == target_digest
                && instance.descriptor.state_schema_version == target_state_schema_version
            {
                return Ok(instance.clone());
            }
            let rollback = instance
                .migration_history
                .iter()
                .rposition(|checkpoint| {
                    checkpoint.art_version == target_version
                        && checkpoint.package_digest == target_digest
                        && checkpoint.state_schema_version == target_state_schema_version
                })
                .map(|index| instance.migration_history.remove(index));
            let current_checkpoint = SurfaceMigrationCheckpoint {
                art_version: instance.descriptor.art_version.clone(),
                package_digest: instance.descriptor.package_digest.clone(),
                state_schema_version: instance.descriptor.state_schema_version,
                authoritative_state: instance.authoritative_state.clone(),
                latest_preview: instance.latest_preview.clone(),
                latest_result: instance.latest_result.clone(),
            };
            instance.migration_history.push(current_checkpoint);
            if instance.migration_history.len() > 8 {
                instance.migration_history.remove(0);
            }
            instance.descriptor.art_version = target_version.to_owned();
            instance.descriptor.package_digest = target_digest;
            instance.descriptor.state_schema_version = target_state_schema_version;
            instance.descriptor.generation = instance.descriptor.generation.saturating_add(1);
            instance.authoritative_state = rollback
                .as_ref()
                .map(|checkpoint| checkpoint.authoritative_state.clone())
                .unwrap_or(migrated_state);
            instance.latest_preview = rollback
                .as_ref()
                .and_then(|checkpoint| checkpoint.latest_preview.clone());
            instance.latest_result = rollback
                .as_ref()
                .and_then(|checkpoint| checkpoint.latest_result.clone());
            if let Some(preview) = instance.latest_preview.as_mut() {
                preview.generation = instance.descriptor.generation;
            }
            if let Some(result) = instance.latest_result.as_mut() {
                result.generation = instance.descriptor.generation;
            }
            instance.last_failure = None;
            instance.pending_events.clear();
            instance.event_acks.clear();
            instance.pending_confirmations.clear();
            for attachment in instance.attachments.values_mut() {
                attachment.snapshot = None;
                if attachment.lifecycle != SurfaceLifecycleState::Disposed {
                    attachment.lifecycle = SurfaceLifecycleState::Created;
                    attachment.lifecycle_revision = attachment.lifecycle_revision.saturating_add(1);
                }
            }
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }
}
