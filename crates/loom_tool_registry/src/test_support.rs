use std::sync::{Mutex, MutexGuard, PoisonError};

// Real PowerShell fixtures contend heavily with executable copying and endpoint protection on a
// clean Windows runner. Serialize only those tests; production framework execution stays parallel.
static WINDOWS_POWERSHELL_FIXTURE_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn lock_windows_powershell_fixture() -> MutexGuard<'static, ()> {
    WINDOWS_POWERSHELL_FIXTURE_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}
