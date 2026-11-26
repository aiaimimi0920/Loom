// Real Device, issuer, approval, Task, and signed plugin process; test approval comes from the trusted test host without spoofing live user UI acceptance.
const CONFIRMATION: &str = "loom.art.local.service.confirmation.v1";
impl Fixture {
    fn sensitive() -> Self {
        let archive = package("local.js",&sha256_bytes(SOURCE),&[SERVICE_ID],SOURCE);
        let fixture = Self::build_package_service(&archive,true,true,true);
        assert_eq!(fixture.approve(0,true).0,200);
        fixture
    }
    fn confirmable(&self, id: &str, input: Value, timeout: u64) -> Value {
        json!({"protocolVersion":CONFIRMATION,"call":self.service_submit(id,input,timeout),"images":[],"imageOutput":false})
    }
    fn confirm_control(submit: &Value, challenge: Option<Value>, approve: bool) -> Value {
        json!({"protocolVersion":CONFIRMATION,"control":Self::service_control(&submit["call"]),
            "decision":challenge.map(|challenge| json!({"challenge":challenge,"approve":approve}))})
    }
    fn confirmed_final(&self, submit: &Value) -> Value {
        let due = Instant::now()+Duration::from_secs(8);
        loop {
            let (status,value) = self.service("poll-confirmable",Self::confirm_control(submit,None,false));
            assert_eq!(status,200,"{value}");
            if matches!(value["reply"]["status"].as_str(),Some("succeeded"|"failed"|"cancelled")) { return value; }
            assert!(Instant::now()<due,"confirmation did not settle: {value}");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
#[test]
fn local_art_confirmation_waits_and_dispatches_exactly_once_after_bound_approval() {
    let f=Fixture::sensitive();
    let old = f.service_submit("call:legacy-sensitive",json!({}),2000);
    assert_eq!(f.service("submit",old.clone()).0,200);
    assert_eq!(f.service_final(&old)["status"],"failed");
    let submit=f.confirmable("call:confirmed",json!({}),3000);
    let (status,pending)=f.service("submit-confirmable",submit.clone()); assert_eq!(status,200,"{pending}");
    let challenge=pending["confirmation"].clone(); assert!(challenge["nonce"].is_string());
    let (_,waiting)=f.service("poll-confirmable",Fixture::confirm_control(&submit,None,false));
    assert_eq!(waiting["confirmation"],challenge); assert!(waiting["reply"]["output"].is_null());
    let mut forged=challenge.clone(); forged["fingerprint"]=json!("f".repeat(64));
    assert_eq!(f.service("confirm",Fixture::confirm_control(&submit,Some(forged),true)).0,409);
    assert_eq!(f.service("confirm",Fixture::confirm_control(&submit,Some(challenge.clone()),true)).0,200);
    let final_reply=f.confirmed_final(&submit); assert_eq!(final_reply["reply"]["output"]["calls"],1,"{final_reply}");
    assert!(final_reply["confirmation"].is_null());
    assert_eq!(f.service("submit-confirmable",submit.clone()).1,final_reply);
    assert_eq!(f.service("confirm",Fixture::confirm_control(&submit,Some(challenge),true)).0,409);
    let mut conflict=submit; conflict["call"]["input"]=json!({"changed":true});
    assert_eq!(f.service("submit-confirmable",conflict).0,409);
}
#[test]
fn local_art_confirmation_denial_timeout_and_cancel_never_execute_or_extend_deadline() {
    let f=Fixture::sensitive();
    for (id,mode) in [("call:deny",0),("call:timeout",1),("call:cancel",2)] {
        let submit=f.confirmable(id,json!({}),if mode==1 {120} else {2000});
        let (_,pending)=f.service("submit-confirmable",submit.clone());
        if mode==0 { assert_eq!(f.service("confirm",Fixture::confirm_control(&submit,Some(pending["confirmation"].clone()),false)).0,200); }
        if mode==2 { assert_eq!(f.service("cancel",Fixture::service_control(&submit["call"])).1["status"],"cancelled"); }
        let result=f.confirmed_final(&submit);
        if mode==2 { assert_eq!(result["reply"]["status"],"cancelled"); }
        else { assert_eq!(result["reply"]["error"],if mode==0 {"local_art_confirmation_denied"} else {"local_art_service_timeout"}); }
        assert_eq!(f.service("confirm",Fixture::confirm_control(&submit,Some(pending["confirmation"].clone()),true)).0,409);
    }
    let submit=f.confirmable("call:after-denial",json!({}),2000);
    let (_,pending)=f.service("submit-confirmable",submit.clone());
    f.service("confirm",Fixture::confirm_control(&submit,Some(pending["confirmation"].clone()),true));
    assert_eq!(f.confirmed_final(&submit)["reply"]["output"]["calls"],1);
}
#[test]
fn local_art_confirmation_registration_change_and_revocation_reject_old_challenge() {
    let f=Fixture::sensitive();
    let submit=f.confirmable("call:scope",json!({}),3000);
    let (_,pending)=f.service("submit-confirmable",submit.clone());
    f.runtime.deactivate("publisher.example/local-service").unwrap();
    let registry=loom_tool_registry::capability::CapabilityPluginRegistry::new(&f.root);
    let record=registry.get("publisher.example/local-service").unwrap().unwrap();
    let package=runtime_package(&registry,&record.qualified_id,record.active_digest.as_deref().unwrap()).unwrap();
    f.runtime.activate(package).unwrap();
    assert_eq!(f.service("confirm",Fixture::confirm_control(&submit,Some(pending["confirmation"].clone()),true)).0,409);
    assert_eq!(f.confirmed_final(&submit)["reply"]["error"],"local_art_command_changed");
    let revoked=f.confirmable("call:revoked-confirmation",json!({}),3000);
    let (_,pending)=f.service("submit-confirmable",revoked.clone());
    assert_eq!(f.approve(1,false).0,200);
    assert_eq!(f.service("confirm",Fixture::confirm_control(&revoked,Some(pending["confirmation"].clone()),true)).0,403);
}
#[test]
fn local_art_confirmation_reuses_images_output_and_cleanup_without_second_resource_framework() {
    let f=Fixture::sensitive();
    let mut submit=f.confirmable("call:confirmed-image",json!({"readImage":true}),4000);
    submit["images"]=json!([{"png":BASE64.encode(image_bytes())}]);
    let (_,pending)=f.service("submit-confirmable",submit.clone());
    assert_eq!(f.resources.lock().unwrap().active_leases_for_test(),0);
    assert_eq!(f.service("confirm",Fixture::confirm_control(&submit,Some(pending["confirmation"].clone()),true)).0,200);
    let result=f.confirmed_final(&submit);
    let hex=image_bytes().iter().map(|byte|format!("{byte:02x}")).collect::<String>();
    assert_eq!(result["reply"]["output"]["pngHex"],hex,"{result}"); wait_image_cleanup(&f);
    let mut output=f.confirmable("call:confirmed-output",json!({"returnImage":true}),4000); output["imageOutput"]=json!(true);
    let (_,pending)=f.service("submit-confirmable",output.clone());
    assert_eq!(f.service("confirm",Fixture::confirm_control(&output,Some(pending["confirmation"].clone()),true)).0,200);
    let result=f.confirmed_final(&output); assert_eq!(result["reply"]["status"],"succeeded","{result}");
    assert_eq!(result["image"]["width"],2); assert_eq!(result["image"]["height"],2);
    assert_eq!(result["image"]["digest"],sha256_bytes(&BASE64.decode(result["image"]["png"].as_str().unwrap()).unwrap()));
    f.service("cancel",Fixture::service_control(&output["call"]));
}

#[test]
fn local_art_confirmation_waiting_uses_original_device_capacity_and_shutdown_drains() {
    let mut f = Fixture::sensitive();
    for index in 0..4 {
        let input = f.confirmable(&format!("call:waiting-{index}"), json!({}), 5000);
        let (status, pending) = f.service("submit-confirmable", input);
        assert_eq!(status, 200, "{pending}");
        assert!(pending["confirmation"].is_object());
    }
    let fifth = f.confirmable("call:waiting-overflow", json!({}), 5000);
    assert_eq!(f.service("submit-confirmable", fifth).0, 429);
    assert_eq!(f.resources.lock().unwrap().active_leases_for_test(), 0);
    let start = Instant::now();
    f.server.finish().unwrap();
    assert!(start.elapsed() < Duration::from_secs(3), "waiting confirmation blocked shutdown");
    assert_eq!(f.resources.lock().unwrap().active_leases_for_test(), 0);
}

#[test]
fn local_art_confirmation_approved_running_call_still_cancels_and_rejects_late_success() {
    let f = Fixture::sensitive();
    let input = f.confirmable("call:approved-cancel", json!({"hang":true}), 4000);
    let (_, pending) = f.service("submit-confirmable", input.clone());
    assert_eq!(f.service("confirm", Fixture::confirm_control(&input, Some(pending["confirmation"].clone()), true)).0, 200);
    let due = Instant::now() + Duration::from_secs(3);
    loop {
        let (_, state) = f.service("poll-confirmable", Fixture::confirm_control(&input, None, false));
        if state["reply"]["progress"]["step"] == 1 { break; }
        assert!(Instant::now() < due, "service never dispatched: {state}");
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(f.service("cancel", Fixture::service_control(&input["call"])).1["status"], "cancelled");
    let final_reply = f.confirmed_final(&input);
    assert_eq!(final_reply["reply"]["status"], "cancelled");
    assert!(final_reply["reply"]["output"].is_null());
    assert!(final_reply["image"].is_null());
    assert_eq!(f.service("submit-confirmable", input).1, final_reply);
}
