// Real signed Capability Plugin for local Art service HTTP tests; installed/approved/enabled normally.
const SERVICE_ID: &str = "capability:publisher.example:local-service.run";
fn install_local_service_fixture(root: &Path, runtime: &SharedCapabilityRuntime) {
    install_local_service_fixture_scoped(root, runtime, false)
}
fn install_local_service_fixture_scoped(
    root: &Path,
    runtime: &SharedCapabilityRuntime,
    images: bool,
) {
    install_local_service_fixture_options(root,runtime,images,false)
}
fn install_local_service_fixture_options(root: &Path, runtime: &SharedCapabilityRuntime, images: bool, gesture: bool) {
    let key = loom_plugin_security::generate_signing_key("local-service-test");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".into(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    trust.write_atomic(&root.join("plugin-trust.json")).unwrap();
    let package = root.join("service-fixture");
    fs::create_dir_all(package.join("runtime")).unwrap();
    let executable = if cfg!(windows) {
        "runtime/service.exe"
    } else {
        "runtime/service"
    };
    let source = package.join("runtime/service.rs");
    fs::write(&source, LOCAL_SERVICE_SOURCE).unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    assert!(Command::new(rustc)
        .arg(&source)
        .arg("-O")
        .arg("-o")
        .arg(package.join(executable))
        .status()
        .unwrap()
        .success());
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("linux", "x86_64") => "linux-x64",
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x64",
        other => panic!("unsupported {other:?}"),
    };
    let permissions = if images {
        vec!["hook.unit.image.read"]
    } else {
        vec![]
    };
    let manifest = json!({"schemaVersion":1,"kind":"capability","id":"local-service","name":"Local service fixture","description":"HTTP service regression","version":"1.0.0",
        "publisher":{"id":"publisher.example","keyId":key.key_id},
        "hostCompatibility":{"loomCapabilityApi":{"minimum":"1.0"},"hookExtensionApi":{"minimum":"1.0"}},
        "entrypoints":{"service":{"targets":{platform:{"command":executable}},"processModel":"persistent"}},
        "contributes":{"commands":[{"id":"publisher.example/local-service.run","title":"Run","permissions":permissions,"requiresUserGesture":gesture}]},
        "permissions":permissions,"resources":{"memoryMiB":64,"maxProcesses":1,"timeoutSeconds":10},"dependencies":[],
        "signature":{"algorithm":"ed25519","keyId":key.key_id,"file":"signature.json"}});
    fs::write(
        package.join("capability.manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    // Only package members enter the archive/signature; compiler source and pdb stay outside.
    fs::remove_file(&source).unwrap();
    if package.join("runtime/service.pdb").exists() {
        fs::remove_file(package.join("runtime/service.pdb")).unwrap();
    }
    loom_plugin_security::sign_package(&package, "signature.json", &key).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for path in ["capability.manifest.json", executable, "signature.json"] {
        let mut options = zip::write::SimpleFileOptions::default();
        if path == executable {
            options = options.unix_permissions(0o755);
        }
        writer.start_file(path, options).unwrap();
        writer
            .write_all(&fs::read(package.join(path)).unwrap())
            .unwrap();
    }
    let archive = writer.finish().unwrap().into_inner();
    let (status, body) = install_capability_plugin(
        &json!({"zipBase64":BASE64.encode(archive)}).to_string(),
        root,
    )
    .unwrap();
    assert_eq!(status, 200, "{body}");
    let installed: Value = serde_json::from_str(&body).unwrap();
    let digest = &installed["package"]["digest"];
    let (status, body) = approve_capability_plugin(
        "publisher.example/local-service",
        &json!({"digest":digest,"permissions":permissions}).to_string(),
        root,
    )
    .unwrap();
    assert_eq!(status, 200, "{body}");
    let (status, body) = enable_capability_plugin(
        "publisher.example/local-service",
        &json!({"digest":digest}).to_string(),
        root,
        runtime,
    )
    .unwrap();
    assert_eq!(status, 200, "{body}");
}
const LOCAL_SERVICE_SOURCE: &str = r###"
use std::io::{Read,Write};
fn field(input:&str,key:&str)->String {
 let marker=format!("\"{}\":\"",key); let rest=&input[input.find(&marker).unwrap()+marker.len()..]; rest[..rest.find('"').unwrap()].to_owned()
}
fn send(out:&mut impl Write,id:&str,status:&str,payload:&str) {
 let msg=format!("{{\"type\":\"response\",\"protocol\":\"loom.capability.runtime.v1\",\"apiVersion\":\"1.0\",\"requestId\":\"{}\",\"status\":\"{}\",\"payload\":{}}}",id,status,payload);
 out.write_all(&(msg.len() as u32).to_be_bytes()).unwrap();out.write_all(msg.as_bytes()).unwrap();out.flush().unwrap();
}
fn main(){
 let mut input=std::io::stdin();let mut out=std::io::stdout();let mut calls=0;
 loop {
  let mut size=[0;4];if input.read_exact(&mut size).is_err(){break}
  let mut bytes=vec![0;u32::from_be_bytes(size) as usize];input.read_exact(&mut bytes).unwrap();let req=String::from_utf8(bytes).unwrap();
  let id=field(&req,"requestId");let method=field(&req,"method");
  if method=="command" {
   calls+=1;send(&mut out,&id,"progress",r#"{"step":1}"#);
   if req.contains("\"hang\":true"){std::thread::sleep(std::time::Duration::from_secs(30));}
   let payload=if req.contains("\"large\":true"){format!("{{\"output\":\"{}\"}}","x".repeat(65537))}
    else if req.contains("\"null\":true"){r#"{"output":null}"#.to_owned()}
    else if req.contains("\"returnImage\":true"){format!(r#"{{"output":{{"calls":{},"answer":42,"content":[{{"type":"image","mimeType":"image/png","data":"iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFElEQVR4nGP4z8DwHwyBNBAwNAAAR0sIeRPxYNAAAAAASUVORK5CYII="}}]}}}}"#,calls)}
    else if req.contains("\"readImage\":true") {
      let path=field(&req,"stagedPath").replace("\\\\","\\");
      let data=std::fs::read(path).unwrap();
      let hex=data.iter().map(|v|format!("{:02x}",v)).collect::<String>();
      format!("{{\"output\":{{\"calls\":{},\"pngHex\":\"{}\"}}}}",calls,hex)
    } else{format!("{{\"output\":{{\"calls\":{}}}}}",calls)};
   send(&mut out,&id,"succeeded",&payload);
  }else{send(&mut out,&id,"succeeded","{}");}
  if method=="deactivate"{break}
 }
}
"###;
