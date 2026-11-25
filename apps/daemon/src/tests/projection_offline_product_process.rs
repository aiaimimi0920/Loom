// Parent-owned Job contains only the explicitly spawned integration test and its future products.
use std::{
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    process::Child,
};
use windows_sys::Win32::System::JobObjects::*;

pub(super) struct Job {
    _handle: OwnedHandle,
}
impl Job {
    pub(super) fn attach(child: &Child) -> Self {
        unsafe {
            let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            assert!(!raw.is_null(), "{}", std::io::Error::last_os_error());
            let handle = OwnedHandle::from_raw_handle(raw);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            assert_ne!(
                SetInformationJobObject(
                    handle.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as _,
                    std::mem::size_of_val(&limits) as u32
                ),
                0,
                "{}",
                std::io::Error::last_os_error()
            );
            assert_ne!(
                AssignProcessToJobObject(handle.as_raw_handle(), child.as_raw_handle()),
                0,
                "{}",
                std::io::Error::last_os_error()
            );
            Self { _handle: handle }
        }
    }
}

#[test]
fn product_job_closure_terminates_only_assigned_test_child() {
    use super::*;
    let root = std::env::temp_dir().join(format!("otter-product-job-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let spawn = |label: &str| {
        Otter(Command::new(std::env::current_exe().unwrap())
        .args(["tests::projection_http::offline_native_joint::product::process::product_job_wait_child",
            "--exact","--ignored","--test-threads=1"])
        .env("OTTER_PRODUCT_JOB_CHILD",root.join(label)).stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null()).spawn().unwrap())
    };
    let mut assigned = spawn("assigned");
    let mut unrelated = spawn("unrelated");
    let job = Job::attach(&assigned.0);
    assert_eq!(wait(&root, "assigned", &mut assigned)["waiting"], true);
    assert_eq!(wait(&root, "unrelated", &mut unrelated)["waiting"], true);
    assert!(unrelated.0.try_wait().unwrap().is_none());
    drop(job);
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        if assigned.0.try_wait().unwrap().is_some() {
            break;
        }
        assert!(Instant::now() < end, "assigned child survived Job close");
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        unrelated.0.try_wait().unwrap().is_none(),
        "Job affected unassigned child"
    );
    // Windows kill-on-close不保证非零退出码；核实际终止且未走release后的正常完成路径。
    assert!(!root.join("assigned.finished").exists());
    write_once(&root.join("release"), b"normal-exit");
    super::fixture::finish(&mut unrelated);
    assert!(root.join("unrelated.finished").exists());
}
#[test]
#[ignore = "only spawned explicitly by owned Job cleanup test"]
fn product_job_wait_child() {
    use super::*;
    let path = PathBuf::from(
        std::env::var_os("OTTER_PRODUCT_JOB_CHILD").expect("explicit isolated Job input"),
    );
    assert!(path.is_absolute() && path.parent().unwrap().starts_with(std::env::temp_dir()));
    write_once(&path, br#"{"waiting":true}"#);
    let end = Instant::now() + Duration::from_secs(15);
    while !path.parent().unwrap().join("release").exists() {
        assert!(Instant::now() < end, "owned Job test release deadline");
        thread::sleep(Duration::from_millis(10));
    }
    write_once(
        &path.with_extension("finished"),
        b"normal-exit-after-release",
    );
}
