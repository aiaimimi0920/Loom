// 真Device/签名插件进程/原任务表；独立PNG字节与RGBA oracle，不读宿主屏幕。
fn output_request(f: &Fixture, id: &str, input: Value) -> Value {
    json!({"protocolVersion":"loom.art.local.service.image-output.v1","call":f.service_submit(id,input,5000),"images":[]})
}
fn output_final(f: &Fixture, call: &Value) -> Value {
    let end=Instant::now()+Duration::from_secs(8);
    loop {
        let (status,response)=f.service("poll-image-output",Fixture::service_control(call));
        assert_eq!(status,200,"{response}");
        if matches!(response["reply"]["status"].as_str(),Some("succeeded"|"failed"|"cancelled")){return response;}
        assert!(Instant::now()<end);thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn local_art_service_image_output_http_returns_exact_png_once_then_releases_terminal_storage() {
    let f=Fixture::with_service();assert_eq!(f.approve(0,true).0,200);
    let request=output_request(&f,"call:output",json!({"returnImage":true}));
    assert_eq!(f.service("submit-image-output",request.clone()).0,200);
    let response=output_final(&f,&request["call"]);
    assert_eq!(response["reply"]["status"],"succeeded");assert_eq!(response["reply"]["output"],json!({"answer":42,"calls":1}));
    let parsed: loom_protocol::local_art::service::image_output::Response=serde_json::from_value(response.clone()).unwrap();
    parsed.validate(&parsed.reply.owner).unwrap();
    let image=parsed.image.unwrap();let bytes=BASE64.decode(image.png).unwrap();assert_eq!(sha256_bytes(&bytes),image.digest);
    let pixels=image::load_from_memory_with_format(&bytes,image::ImageFormat::Png).unwrap().to_rgba8();
    assert_eq!(pixels.dimensions(),(2,2));assert_eq!(pixels.as_raw(),&[255,0,0,255,0,255,0,255,0,0,255,255,255,255,0,128]);
    assert_eq!(f.service("submit-image-output",request.clone()).1,response);
    assert_eq!(f.service("submit",request["call"].clone()).0,409);
    let control=Fixture::service_control(&request["call"]);
    assert_eq!(f.service("cancel",control.clone()).1,response["reply"]);
    assert_eq!(f.service("poll-image-output",control).0,409);
}
#[test]
fn local_art_service_image_output_http_rejects_invalid_output_forgery_cancel_and_revoke() {
    let f=Fixture::with_service();assert_eq!(f.approve(0,true).0,200);
    let request=output_request(&f,"call:bad-output",json!({"null":true}));
    let mut forged=request.clone();forged["call"]["grant"]["signature"]=json!("wrong");assert_eq!(f.service("submit-image-output",forged).0,403);
    assert_eq!(f.service("submit-image-output",request.clone()).0,200);
    let response=output_final(&f,&request["call"]);assert_eq!(response["reply"]["status"],"failed");assert!(response["image"].is_null());
    let request=output_request(&f,"call:cancel-output",json!({"hang":true,"returnImage":true}));
    assert_eq!(f.service("submit-image-output",request.clone()).0,200);
    let control=Fixture::service_control(&request["call"]);assert_eq!(f.service("cancel",control.clone()).1["status"],"cancelled");
    let response=output_final(&f,&request["call"]);assert_eq!(response["reply"]["status"],"cancelled");assert!(response["image"].is_null());
    assert_eq!(f.approve(1,false).0,200);assert_eq!(f.service("poll-image-output",control).0,403);
}
