//! 显式运行的跨仓库测试；仅连接独占回环 Worker，不改变正式服务地址或准入。
use super::*;

#[test]
#[ignore = "requires an isolated AssetLibrary trial fixture"]
fn asset_library_worker_download_contract() {
    let fixture = std::env::var("LOOM_ASSETLIBRARY_TEST_FIXTURE").expect("fixture path required");
    let config: Value = serde_json::from_slice(&fs::read(fixture).unwrap()).unwrap();
    assert_eq!(config["mode"], "local-trial-only");
    let origin = config["origin"].as_str().unwrap();
    let url = reqwest::Url::parse(origin).unwrap();
    assert_eq!(url.scheme(), "http");
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    let root = PathBuf::from(config["downloadRoot"].as_str().unwrap());
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap();
    let result = download(
        &root,
        config["publicationId"].as_str().unwrap(),
        client,
        config["token"].as_str().unwrap().into(),
        origin,
    );
    if config["expected"] == "saved" {
        let value = result.unwrap();
        let bytes = fs::read(value["path"].as_str().unwrap()).unwrap();
        assert_eq!(bytes.len() as u64, config["size"].as_u64().unwrap());
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), config["sha256"]);
        assert_eq!(value["publicationId"], config["publicationId"]);
        assert!(!value
            .to_string()
            .contains(config["token"].as_str().unwrap()));
        assert_eq!(
            fs::read_dir(root.join("asset-library-downloads"))
                .unwrap()
                .count(),
            1
        );
    } else {
        assert_eq!(config["expected"], "denied");
        let failure = result.expect_err("revoked or unlisted publication must not download");
        assert_eq!(failure.status, 404);
        assert!(!root.join("asset-library-downloads").exists());
    }
}
