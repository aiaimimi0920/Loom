// Immutable source-frame PNG representations, independent of endpoint authority and socket state.
const WALL_PNG_FRAME_PROFILES: usize = 4;
const WALL_PNG_CACHE_BYTES: usize = 32 * 1024 * 1024;
static WALL_PNG_RETAINED: std::sync::LazyLock<Arc<AtomicUsize>> =
    std::sync::LazyLock::new(|| Arc::new(AtomicUsize::new(0)));
type WallPngResult = std::result::Result<Arc<WallPngImage>, &'static str>;
type WallPngSlot = Arc<Mutex<Option<WallPngResult>>>;

#[derive(Default)]
struct WallPngFrameCache {
    // A frame owns its source/epoch/ID and immutable pixels. fps does not alter encoding.
    slots: Vec<((u32, u32), WallPngSlot)>,
}

struct WallPngImage {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
    _reservation: Option<WallPngReservation>,
}

struct WallPngReservation {
    retained: Arc<AtomicUsize>,
    bytes: usize,
}

impl WallPngReservation {
    fn acquire(retained: &Arc<AtomicUsize>, bytes: usize, limit: usize) -> Option<Self> {
        retained
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
                used.checked_add(bytes).filter(|total| *total <= limit)
            })
            .ok()?;
        Some(Self {
            retained: Arc::clone(retained),
            bytes,
        })
    }
}

impl Drop for WallPngReservation {
    fn drop(&mut self) {
        self.retained.fetch_sub(self.bytes, Ordering::SeqCst);
    }
}

impl StoredLiveFrame {
    fn wall_png_image(
        &self,
        raw: &[u8],
        width: u32,
        height: u32,
        profile: WallMediaProfile,
    ) -> std::result::Result<Option<Arc<WallPngImage>>, &'static str> {
        let key = (profile.width, profile.height);
        let slot = {
            let mut cache = match self.wall_png.try_lock() {
                Ok(cache) => cache,
                Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
                Err(_) => return Err("wall_png_cache_unavailable"),
            };
            if let Some((_, slot)) = cache.slots.iter().find(|(stored, _)| *stored == key) {
                Some(Arc::clone(slot))
            } else if cache.slots.len() < WALL_PNG_FRAME_PROFILES {
                let slot = Arc::new(Mutex::new(None));
                cache.slots.push((key, Arc::clone(&slot)));
                Some(slot)
            } else {
                None
            }
        };
        // Different profiles never wait behind an encode. Raw/JPEG paths do not take either cache lock.
        let mut cached = if let Some(slot) = slot.as_ref() {
            match slot.try_lock() {
                Ok(cache) => Some(cache),
                Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
                Err(_) => return Err("wall_png_cache_unavailable"),
            }
        } else {
            None
        };
        if let Some(result) = cached.as_ref().and_then(|cache| cache.as_ref()) {
            return result.clone().map(Some);
        }
        if WALL_IMAGE_ENCODERS
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                (count < 2).then_some(count + 1)
            })
            .is_err()
        {
            return Ok(None);
        }
        let _permit = WallImageEncodePermit;
        let result = encode_wall_png(raw, width, height, profile);
        if let Some(cache) = cached.as_mut() {
            match &result {
                Ok(image) if image._reservation.is_none() => {}
                _ => **cache = Some(result.clone()),
            }
        }
        result.map(Some)
    }
}

fn encode_wall_png(
    raw: &[u8],
    width: u32,
    height: u32,
    profile: WallMediaProfile,
) -> WallPngResult {
    let pixels = raw.get(64..).ok_or("wall_source_dimensions_invalid")?;
    let (width, height, pixels) =
        scale_wall_bgra(pixels, width, height, profile.width, profile.height)?;
    use image::{
        codecs::png::{CompressionType, FilterType, PngEncoder},
        ImageEncoder,
    };
    let mut bytes = Vec::new();
    // Encoding settings are fixed for every cache key; any future configurable setting must join the key.
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::NoFilter)
        .write_image(&pixels, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|_| "wall_png_encode_failed")?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("wall_media_frame_limit");
    }
    // No eviction lock or waiting: budget/profile overflow still sends an uncached bounded representation.
    // Vec capacity, not payload length, is the retained allocation budget.
    let reservation =
        WallPngReservation::acquire(&WALL_PNG_RETAINED, bytes.capacity(), WALL_PNG_CACHE_BYTES);
    Ok(Arc::new(WallPngImage {
        width,
        height,
        bytes,
        _reservation: reservation,
    }))
}
