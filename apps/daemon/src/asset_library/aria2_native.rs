//! Windows C ABI 适配：唯一工作线程拥有 libaria2 会话，其他线程只操作原子控制状态。
use super::{error, Result};
#[cfg(all(test, windows))]
#[path = "aria2_native_tests.rs"]
mod tests;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicU8, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct State {
    pub(super) control: AtomicU8,
    pub(super) total: AtomicU64,
    pub(super) completed: AtomicU64,
    pub(super) speed: AtomicU64,
}
pub(super) struct Task {
    state: Arc<State>,
    result: mpsc::Receiver<i32>,
    settled: bool,
}
impl Drop for Task {
    fn drop(&mut self) {
        if !self.settled {
            self.state.control.store(1, Ordering::Release);
        }
    }
}
impl Task {
    #[cfg(test)]
    pub fn progress(&self) -> (u64, u64, u64) {
        (
            self.state.total.load(Ordering::Acquire),
            self.state.completed.load(Ordering::Acquire),
            self.state.speed.load(Ordering::Acquire),
        )
    }
    pub fn wait(mut self) -> Result<()> {
        let result = self.result.recv_timeout(Duration::from_secs(90));
        self.settled = result.is_ok();
        match result {
            Ok(0) => Ok(()),
            _ => Err(error(503, "asset_library_download_interrupted")),
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        ffi::{c_char, c_void, CString},
        fs,
        io::Read,
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
        sync::{atomic::AtomicBool, Mutex, OnceLock},
    };
    use windows_sys::Win32::{
        Foundation::{FreeLibrary, HMODULE},
        System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32},
    };

    type Callback = unsafe extern "C" fn(*mut c_void, u64, u64, u64) -> i32;
    type Download =
        unsafe extern "C" fn(*const c_char, *const c_char, Callback, *mut c_void) -> i32;
    struct Module(HMODULE);
    impl Drop for Module {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }
    struct Engine {
        download: Download,
        close: unsafe extern "C" fn(),
        _module: Module,
        _file: fs::File,
    }
    impl Drop for Engine {
        fn drop(&mut self) {
            unsafe {
                (self.close)();
            }
        }
    }
    struct Job {
        url: String,
        directory: PathBuf,
        expected: u64,
        state: Arc<State>,
        result: mpsc::Sender<i32>,
    }
    struct Context {
        state: Arc<State>,
        expected: u64,
        started: Instant,
    }
    struct Worker {
        sender: mpsc::SyncSender<Job>,
        thread: std::thread::JoinHandle<()>,
    }
    static WORKER: OnceLock<Mutex<Option<Worker>>> = OnceLock::new();
    static STOPPING: AtomicBool = AtomicBool::new(false);

    unsafe extern "C" fn progress(
        pointer: *mut c_void,
        total: u64,
        completed: u64,
        speed: u64,
    ) -> i32 {
        // context 在同步 FFI 调用返回前一直由工作线程持有，回调不存储该指针。
        let context = unsafe { &*(pointer as *const Context) };
        context.state.total.store(total, Ordering::Release);
        context.state.completed.store(completed, Ordering::Release);
        context.state.speed.store(speed, Ordering::Release);
        if STOPPING.load(Ordering::Acquire)
            || context.started.elapsed() > Duration::from_secs(75)
            || total > context.expected
            || completed > context.expected
        {
            return 1;
        }
        context.state.control.load(Ordering::Acquire) as i32
    }

    fn library_path() -> std::result::Result<PathBuf, ()> {
        #[cfg(test)]
        if let Some(path) = std::env::var_os("LOOM_TEST_LIBARIA2") {
            return Ok(path.into());
        }
        Ok(std::env::current_exe()
            .map_err(|_| ())?
            .parent()
            .ok_or(())?
            .join("download-engine/loom_aria2.dll"))
    }

    fn load() -> std::result::Result<Engine, ()> {
        let path = library_path()?;
        if !path.is_absolute() {
            return Err(());
        }
        let metadata = fs::symlink_metadata(&path).map_err(|_| ())?;
        if !metadata.is_file()
            || metadata.len() > 16 * 1024 * 1024
            || loom_security::metadata_has_link_semantics(&metadata)
        {
            return Err(());
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .map_err(|_| ())?;
        let mut bytes = Vec::new();
        (&file)
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ())?;
        let pin: serde_json::Value =
            serde_json::from_str(include_str!("../../../../security/libaria2-runtime.json"))
                .map_err(|_| ())?;
        if pin["librarySha256"].as_str() != Some(format!("{:x}", Sha256::digest(&bytes)).as_str()) {
            return Err(());
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // 只加载绝对路径 DLL；依赖仅从系统目录解析，不搜索 PATH 或当前目录。
        let module = Module(unsafe {
            LoadLibraryExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        });
        if module.0.is_null() {
            return Err(());
        }
        let abi =
            unsafe { GetProcAddress(module.0, c"loom_aria2_abi".as_ptr().cast()) }.ok_or(())?;
        let abi: unsafe extern "C" fn() -> u32 = unsafe { std::mem::transmute(abi) };
        if unsafe { abi() } != 1 {
            return Err(());
        }
        let function =
            unsafe { GetProcAddress(module.0, c"loom_aria2_download".as_ptr().cast()) }.ok_or(())?;
        let download: Download = unsafe { std::mem::transmute(function) };
        let close =
            unsafe { GetProcAddress(module.0, c"loom_aria2_close".as_ptr().cast()) }.ok_or(())?;
        let close: unsafe extern "C" fn() = unsafe { std::mem::transmute(close) };
        Ok(Engine {
            download,
            close,
            _module: module,
            _file: file,
        })
    }

    fn execute(engine: &Engine, job: &Job) -> std::result::Result<i32, ()> {
        if STOPPING.load(Ordering::Acquire) || job.state.control.load(Ordering::Acquire) == 1 {
            return Ok(1);
        }
        let url = CString::new(job.url.as_str()).map_err(|_| ())?;
        let directory = CString::new(job.directory.to_str().ok_or(())?).map_err(|_| ())?;
        let mut context = Context {
            state: job.state.clone(),
            expected: job.expected,
            started: Instant::now(),
        };
        let result = unsafe {
            (engine.download)(
                url.as_ptr(),
                directory.as_ptr(),
                progress,
                (&mut context as *mut Context).cast(),
            )
        };
        Ok(result)
    }

    pub(super) fn start(
        url: String,
        directory: PathBuf,
        expected: u64,
        state: Arc<State>,
    ) -> Result<Task> {
        if STOPPING.load(Ordering::Acquire) {
            return Err(error(503, "asset_library_engine_stopping"));
        }
        let mut worker = WORKER
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| error(503, "asset_library_engine_start_failed"))?;
        if STOPPING.load(Ordering::Acquire) {
            return Err(error(503, "asset_library_engine_stopping"));
        }
        if worker.is_none() {
            let (sender, receiver) = mpsc::sync_channel::<Job>(8);
            let thread = std::thread::Builder::new()
                .name("loom-libaria2".into())
                .spawn(move || {
                    let engine = load();
                    for job in receiver {
                        let result = engine
                            .as_ref()
                            .map_err(|_| ())
                            .and_then(|engine| execute(engine, &job))
                            .unwrap_or(-100);
                        let detached = job.result.send(result).is_err();
                        // 成功送达结果后，目录所有权移交调用线程，不能再因迟到取消删除它。
                        if result != 0 || detached {
                            super::super::aria2::cleanup(&job.directory);
                        }
                    }
                    // 最后一个 Session 已清理；显式关闭工作线程后卸载 DLL 和全局库状态。
                })
                .map_err(|_| error(503, "asset_library_engine_start_failed"))?;
            *worker = Some(Worker { sender, thread });
        }
        let (result, receiver) = mpsc::channel();
        worker
            .as_ref()
            .unwrap()
            .sender
            .try_send(Job {
                url,
                directory,
                expected,
                state: state.clone(),
                result,
            })
            .map_err(|_| error(503, "asset_library_engine_busy"))?;
        Ok(Task {
            state,
            result: receiver,
            settled: false,
        })
    }

    pub(super) fn shutdown() {
        STOPPING.store(true, Ordering::Release);
        if let Some(worker) = WORKER.get().and_then(|worker| worker.lock().ok()?.take()) {
            drop(worker.sender);
            let _ = worker.thread.join();
        }
    }
}

pub(super) fn shutdown() {
    #[cfg(windows)]
    windows::shutdown();
}

pub(super) fn start(url: String, directory: PathBuf, expected: u64) -> Result<Task> {
    start_controlled(url, directory, expected, Arc::new(State::default()))
}

pub(super) fn start_controlled(
    url: String,
    directory: PathBuf,
    expected: u64,
    state: Arc<State>,
) -> Result<Task> {
    #[cfg(windows)]
    {
        windows::start(url, directory, expected, state)
    }
    #[cfg(not(windows))]
    {
        let _ = (url, directory, expected, state);
        Err(error(503, "asset_library_engine_platform_unsupported"))
    }
}
