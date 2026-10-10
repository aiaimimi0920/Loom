// Public pairing owns a small expiring pending set, not an unbounded device archive.
const MAX_PENDING_DEVICE_PAIRINGS: usize = 128;
const DEVICE_PAIRING_TTL_MILLIS: u64 = 10 * 60_000;

impl DeviceRegistryStore {
    fn expire_pending_pairings(&mut self, now: u64) -> Result<()> {
        // Never erase an administrator's disablement or historical revocation:
        // public re-registration must not manufacture a fresh enabled identity.
        let expired: Vec<_> = self
            .devices
            .values()
            .filter(|device| {
                !device.is_local
                    && device.approval == "pending"
                    && device.enabled
                    && device.session_epoch == 0
                    && device.created_at.saturating_add(DEVICE_PAIRING_TTL_MILLIS) <= now
            })
            .map(|device| device.id.clone())
            .collect();
        if expired.is_empty() {
            return Ok(());
        }
        let removed: Vec<_> = expired
            .into_iter()
            .filter_map(|id| self.devices.remove(&id))
            .collect();
        if let Err(error) = self.persist() {
            for device in removed {
                self.devices.insert(device.id.clone(), device);
            }
            return Err(error);
        }
        Ok(())
    }

    fn pending_pairing_capacity_available(&self) -> bool {
        self.devices
            .values()
            .filter(|device| device.approval == "pending")
            .take(MAX_PENDING_DEVICE_PAIRINGS)
            .count()
            < MAX_PENDING_DEVICE_PAIRINGS
    }
}
