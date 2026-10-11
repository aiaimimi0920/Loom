//! Validate supervisors once at the outer boundary; pin files across nested mounts.

use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::OnceLock;

use crate::linux_bootstrap;

pub(crate) struct Supervisors {
    pub(crate) bubblewrap: File,
    pub(crate) perl: File,
}

impl Supervisors {
    pub(crate) fn get() -> io::Result<&'static Self> {
        static FILES: OnceLock<Result<Supervisors, String>> = OnceLock::new();
        FILES
            .get_or_init(|| Self::load().map_err(|error| error.to_string()))
            .as_ref()
            .map_err(|error| io::Error::other(error.clone()))
    }

    fn load() -> io::Result<Self> {
        // Only the initial identity mapping distinguishes host root ownership.
        // overflowuid and a nested namespace's virtual UID 0 prove no such thing.
        let mapping = std::fs::read_to_string("/proc/self/uid_map")?;
        if mapping.split_whitespace().collect::<Vec<_>>() == ["0", "0", "4294967295"] {
            return Ok(Self {
                bubblewrap: trusted_file(Path::new("/usr/bin/bwrap"))?,
                perl: trusted_file(Path::new("/usr/bin/perl"))?,
            });
        }
        let fd = std::env::var(linux_bootstrap::ENV_KEY)
            .ok()
            .and_then(|value| value.parse::<RawFd>().ok())
            .ok_or_else(|| {
                io::Error::other("unmapped root requires inherited supervisor descriptors")
            })?;
        let [bubblewrap, perl] = linux_bootstrap::receive(fd)?;
        let files = Self {
            bubblewrap: File::from(bubblewrap),
            perl: File::from(perl),
        };
        if !files.bubblewrap.metadata()?.is_file() || !files.perl.metadata()?.is_file() {
            return Err(io::Error::other(
                "supervisor descriptors must pin regular files",
            ));
        }
        Ok(files)
    }

    pub(crate) fn descriptors(&self) -> [RawFd; 2] {
        [self.bubblewrap.as_raw_fd(), self.perl.as_raw_fd()]
    }
}

fn trusted_file(path: &Path) -> io::Result<File> {
    let resolved = path.canonicalize()?;
    for component in resolved.ancestors() {
        let metadata = component.metadata()?;
        if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err(io::Error::other("supervisor executable and ancestors must be root-owned and not group/world-writable"));
        }
    }
    let file = File::open(&resolved)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err(io::Error::other("opened supervisor file is not trusted"));
    }
    Ok(file)
}
