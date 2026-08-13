//! Shared image buffer contracts for Loom Art execution.

use std::collections::BTreeMap;
use std::io::Cursor;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
    MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};

#[derive(Debug, Error)]
pub enum SharedImageError {
    #[error("invalid shared image dimensions {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("shared image `{handle}` size mismatch: expected {expected} bytes but received {actual} bytes")]
    SizeMismatch {
        handle: String,
        expected: usize,
        actual: usize,
    },
    #[error("shared image `{0}` was not found")]
    NotFound(String),
    #[error("shared image data URL is invalid: {0}")]
    InvalidDataUrl(String),
    #[error("shared image decode failed: {0}")]
    Decode(String),
    #[error("shared image encode failed: {0}")]
    Encode(String),
    #[error("shared image platform error: {0}")]
    Platform(String),
}

pub type SharedImageResult<T> = Result<T, SharedImageError>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SharedImageFormat {
    Rgba8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedImageInfo {
    pub handle: String,
    pub size: usize,
    pub width: u32,
    pub height: u32,
    pub format: SharedImageFormat,
}

#[derive(Debug)]
pub struct SharedImageStore {
    prefix: String,
    next_id: u64,
    buffers: BTreeMap<String, SharedImageBuffer>,
}

impl Default for SharedImageStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedImageStore {
    #[must_use]
    pub fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        Self {
            prefix: format!("Loom_Buffer_{}_{unique}", std::process::id()),
            next_id: 0,
            buffers: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn new_for_test(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            next_id: 0,
            buffers: BTreeMap::new(),
        }
    }

    pub fn create_rgba8(
        &mut self,
        width: u32,
        height: u32,
        data: Vec<u8>,
    ) -> SharedImageResult<SharedImageInfo> {
        let expected = rgba8_size(width, height)?;
        let handle = self.next_handle();
        if data.len() != expected {
            return Err(SharedImageError::SizeMismatch {
                handle,
                expected,
                actual: data.len(),
            });
        }

        let mut buffer = SharedImageBuffer::create(handle.clone(), width, height, expected)?;
        buffer.write(&data)?;
        let info = buffer.info.clone();
        self.buffers.insert(handle, buffer);
        Ok(info)
    }

    pub fn create_from_data_url(&mut self, data_url: &str) -> SharedImageResult<SharedImageInfo> {
        let bytes = decode_data_url(data_url)?;
        let image = image::load_from_memory(&bytes)
            .map_err(|error| SharedImageError::Decode(error.to_string()))?
            .to_rgba8();
        self.create_rgba8(image.width(), image.height(), image.into_raw())
    }

    pub fn read_rgba8(&self, handle: &str) -> SharedImageResult<Vec<u8>> {
        self.buffers
            .get(handle)
            .ok_or_else(|| SharedImageError::NotFound(handle.to_owned()))?
            .read()
    }

    pub fn read_rgba8_or_open(&self, handle: &str, size: usize) -> SharedImageResult<Vec<u8>> {
        if let Some(buffer) = self.buffers.get(handle) {
            return buffer.read();
        }
        SharedImageBuffer::open_existing(handle.to_owned(), size)?.read()
    }

    pub fn read_png_data_url(&self, handle: &str) -> SharedImageResult<String> {
        let info = self
            .get(handle)
            .ok_or_else(|| SharedImageError::NotFound(handle.to_owned()))?;
        let data = self.read_rgba8(handle)?;
        rgba8_to_png_data_url(info.width, info.height, data)
    }

    #[must_use]
    pub fn get(&self, handle: &str) -> Option<SharedImageInfo> {
        self.buffers.get(handle).map(|buffer| buffer.info.clone())
    }

    #[must_use]
    pub fn list(&self) -> Vec<SharedImageInfo> {
        self.buffers
            .values()
            .map(|buffer| buffer.info.clone())
            .collect()
    }

    pub fn release(&mut self, handle: &str) -> bool {
        self.buffers.remove(handle).is_some()
    }

    fn next_handle(&mut self) -> String {
        self.next_id += 1;
        format!("{}_{}", self.prefix, self.next_id)
    }
}

#[derive(Debug)]
struct SharedImageBuffer {
    info: SharedImageInfo,
    backend: SharedImageBackend,
}

impl SharedImageBuffer {
    fn create(handle: String, width: u32, height: u32, size: usize) -> SharedImageResult<Self> {
        let backend = SharedImageBackend::create(&handle, size)?;
        Ok(Self {
            info: SharedImageInfo {
                handle,
                size,
                width,
                height,
                format: SharedImageFormat::Rgba8,
            },
            backend,
        })
    }

    fn open_existing(handle: String, size: usize) -> SharedImageResult<Self> {
        let backend = SharedImageBackend::open_existing(&handle, size)?;
        Ok(Self {
            info: SharedImageInfo {
                handle,
                size,
                width: 0,
                height: 0,
                format: SharedImageFormat::Rgba8,
            },
            backend,
        })
    }

    fn write(&mut self, data: &[u8]) -> SharedImageResult<()> {
        if data.len() != self.info.size {
            return Err(SharedImageError::SizeMismatch {
                handle: self.info.handle.clone(),
                expected: self.info.size,
                actual: data.len(),
            });
        }
        self.backend.write(data)
    }

    fn read(&self) -> SharedImageResult<Vec<u8>> {
        self.backend.read(self.info.size)
    }
}

#[cfg(windows)]
struct SharedImageBackend {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
}

#[cfg(windows)]
impl SharedImageBackend {
    fn create(name: &str, size: usize) -> SharedImageResult<Self> {
        let wide_name = wide_name(name);
        let mapping_size = u32::try_from(size)
            .map_err(|_| SharedImageError::Platform(format!("buffer too large: {size}")))?;
        unsafe {
            let handle = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                mapping_size,
                wide_name.as_ptr(),
            );
            if is_invalid_handle(handle) {
                return Err(SharedImageError::Platform(format!(
                    "CreateFileMappingW failed for {name}"
                )));
            }

            let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size);
            if view.Value.is_null() {
                let _ = CloseHandle(handle);
                return Err(SharedImageError::Platform(format!(
                    "MapViewOfFile failed for {name}"
                )));
            }

            Ok(Self { handle, view })
        }
    }

    fn open_existing(name: &str, size: usize) -> SharedImageResult<Self> {
        let wide_name = wide_name(name);
        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, wide_name.as_ptr());
            if is_invalid_handle(handle) {
                return Err(SharedImageError::NotFound(name.to_owned()));
            }

            let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size);
            if view.Value.is_null() {
                let _ = CloseHandle(handle);
                return Err(SharedImageError::Platform(format!(
                    "MapViewOfFile failed for {name}"
                )));
            }

            Ok(Self { handle, view })
        }
    }

    fn write(&mut self, data: &[u8]) -> SharedImageResult<()> {
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), self.view.Value.cast::<u8>(), data.len());
        }
        Ok(())
    }

    fn read(&self, size: usize) -> SharedImageResult<Vec<u8>> {
        let mut data = vec![0_u8; size];
        unsafe {
            std::ptr::copy_nonoverlapping(self.view.Value.cast::<u8>(), data.as_mut_ptr(), size);
        }
        Ok(data)
    }
}

#[cfg(windows)]
impl Drop for SharedImageBackend {
    fn drop(&mut self) {
        unsafe {
            if !self.view.Value.is_null() {
                let _ = UnmapViewOfFile(self.view);
            }
            if !is_invalid_handle(self.handle) {
                let _ = CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(windows)]
unsafe impl Send for SharedImageBackend {}
#[cfg(windows)]
unsafe impl Sync for SharedImageBackend {}

#[cfg(windows)]
impl std::fmt::Debug for SharedImageBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedImageBackend")
            .field("handle", &self.handle)
            .field("view", &self.view.Value)
            .finish()
    }
}

#[cfg(not(windows))]
#[derive(Debug)]
struct SharedImageBackend {
    data: Vec<u8>,
}

#[cfg(not(windows))]
impl SharedImageBackend {
    fn create(_name: &str, size: usize) -> SharedImageResult<Self> {
        Ok(Self {
            data: vec![0_u8; size],
        })
    }

    fn open_existing(name: &str, _size: usize) -> SharedImageResult<Self> {
        Err(SharedImageError::NotFound(name.to_owned()))
    }

    fn write(&mut self, data: &[u8]) -> SharedImageResult<()> {
        self.data.copy_from_slice(data);
        Ok(())
    }

    fn read(&self, size: usize) -> SharedImageResult<Vec<u8>> {
        Ok(self.data[..size].to_vec())
    }
}

fn rgba8_size(width: u32, height: u32) -> SharedImageResult<usize> {
    if width == 0 || height == 0 {
        return Err(SharedImageError::InvalidDimensions { width, height });
    }
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .map(|bytes| bytes as usize)
        .ok_or(SharedImageError::InvalidDimensions { width, height })
}

fn decode_data_url(data_url: &str) -> SharedImageResult<Vec<u8>> {
    let payload = data_url
        .split_once(',')
        .map(|(_, payload)| payload)
        .unwrap_or(data_url)
        .trim();
    if payload.is_empty() {
        return Err(SharedImageError::InvalidDataUrl(
            "missing base64 payload".to_owned(),
        ));
    }
    BASE64
        .decode(payload)
        .map_err(|error| SharedImageError::Decode(error.to_string()))
}

pub fn rgba8_to_png_data_url(width: u32, height: u32, data: Vec<u8>) -> SharedImageResult<String> {
    let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, data)
        .ok_or_else(|| SharedImageError::Encode("invalid RGBA8 image buffer".to_owned()))?;
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|error| SharedImageError::Encode(error.to_string()))?;
    Ok(format!(
        "data:image/png;base64,{}",
        BASE64.encode(png.into_inner())
    ))
}

#[cfg(windows)]
fn wide_name(name: &str) -> Vec<u16> {
    OsStr::new(name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn is_invalid_handle(handle: HANDLE) -> bool {
    handle.is_null() || handle == INVALID_HANDLE_VALUE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_image_store_create_write_read_list_release_roundtrip() {
        let mut store = SharedImageStore::new_for_test("Loom_Test_Buffer");

        let info = store
            .create_rgba8(1, 1, vec![10, 20, 30, 255])
            .expect("create shared image");

        assert_eq!(info.width, 1);
        assert_eq!(info.height, 1);
        assert_eq!(info.size, 4);
        assert_eq!(info.format, SharedImageFormat::Rgba8);
        assert_eq!(
            store.read_rgba8(&info.handle).expect("read"),
            vec![10, 20, 30, 255]
        );
        assert_eq!(store.get(&info.handle), Some(info.clone()));
        assert_eq!(store.list(), vec![info.clone()]);
        assert!(store.release(&info.handle));
        assert!(!store.release(&info.handle));
        assert!(store.list().is_empty());
    }

    #[test]
    fn shared_image_store_creates_rgba8_from_png_data_url() {
        let mut store = SharedImageStore::new_for_test("Loom_Test_Png");
        let data_url = encode_test_png_data_url([10, 20, 30, 255]);

        let info = store
            .create_from_data_url(&data_url)
            .expect("create from png");

        assert_eq!(info.width, 1);
        assert_eq!(info.height, 1);
        assert_eq!(
            store.read_rgba8(&info.handle).expect("read"),
            vec![10, 20, 30, 255]
        );
        assert!(store
            .read_png_data_url(&info.handle)
            .expect("read png")
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn shared_image_store_rejects_rgba_size_mismatch() {
        let mut store = SharedImageStore::new_for_test("Loom_Test_Invalid");

        let error = store
            .create_rgba8(2, 1, vec![10, 20, 30, 255])
            .expect_err("size mismatch fails");

        assert!(error.to_string().contains("size"));
    }

    #[test]
    fn shared_image_store_can_open_own_named_buffer_by_descriptor() {
        let mut store = SharedImageStore::new_for_test("Loom_Test_Open");
        let info = store
            .create_rgba8(1, 1, vec![1, 2, 3, 4])
            .expect("create shared image");

        assert_eq!(
            store
                .read_rgba8_or_open(&info.handle, info.size)
                .expect("open/read by descriptor"),
            vec![1, 2, 3, 4]
        );
    }

    fn encode_test_png_data_url(pixel: [u8; 4]) -> String {
        rgba8_to_png_data_url(1, 1, pixel.to_vec()).expect("encode png")
    }
}
