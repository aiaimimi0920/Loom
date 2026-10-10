use super::*;
#[cfg(feature = "server")]
use tungstenite::protocol::frame::{
    coding::{Data, OpCode},
    Frame,
};

#[test]
fn command_lines_are_bounded_strict_utf8_and_require_newline() {
    assert!(read_command(&mut &b""[..]).unwrap().is_none());
    assert!(read_command(&mut &b"{\"op\":\"close\"}\n"[..])
        .unwrap()
        .is_some());
    assert!(read_command(&mut &b"{\"op\":\"close\"}"[..]).is_err());
    assert!(read_command(&mut &b"\xff\n"[..]).is_err());
    assert!(read_command(&mut vec![b' '; MAX_LINE as usize + 1].as_slice()).is_err());
    for value in [0, 150_001] {
        assert!(deadline(&json!({"timeoutMs":value})).is_err());
    }
}

#[cfg(feature = "server")]
fn fixture(
    send: impl FnOnce(&mut loom_local_channel::ServerSocket) + Send + 'static,
) -> (ClientSocket, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let identity =
        loom_local_channel::ServerIdentity::generate(listener.local_addr().unwrap().port())
            .unwrap();
    let discovery = identity.discovery().clone();
    let worker = std::thread::spawn(move || {
        let mut socket = identity.accept(listener.accept().unwrap().0).unwrap();
        send(&mut socket);
    });
    (connect(&discovery, Duration::from_secs(1)).unwrap(), worker)
}

#[cfg(feature = "server")]
#[test]
fn receives_broadcast_and_response_in_real_wire_order() {
    let (mut socket, worker) = fixture(|socket| {
        for payload in [
            json!({"method":"loom.hook.art.progress"}),
            json!({"requestId":"execute", "status":"succeeded"}),
        ] {
            socket.send(Message::Text(payload.to_string())).unwrap();
        }
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    assert_eq!(
        receive(&mut socket, deadline, MAX_BYTES).unwrap()["method"],
        "loom.hook.art.progress"
    );
    assert_eq!(
        receive(&mut socket, deadline, MAX_BYTES).unwrap()["status"],
        "succeeded"
    );
    worker.join().unwrap();
}

#[cfg(feature = "server")]
#[test]
fn oversized_payload_fails_without_returning_partial_json() {
    let (mut socket, worker) = fixture(|socket| {
        socket
            .send(Message::Text(json!({"data":"x".repeat(2000)}).to_string()))
            .unwrap();
    });
    assert!(receive(&mut socket, Instant::now() + Duration::from_secs(1), 100).is_err());
    worker.join().unwrap();
}

#[cfg(feature = "server")]
#[test]
fn control_frames_cannot_extend_receive_deadline() {
    let (mut socket, worker) = fixture(|socket| {
        for _ in 0..30 {
            if socket.send(Message::Pong(vec![])).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
    });
    let start = Instant::now();
    assert!(receive(&mut socket, start + Duration::from_millis(100), MAX_BYTES).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(socket);
    worker.join().unwrap();
}

#[cfg(feature = "server")]
#[test]
fn fragmented_utf8_is_reassembled_before_json_decode() {
    let (mut socket, worker) = fixture(|socket| {
        let payload = json!({"text":"测试"}).to_string().into_bytes();
        let split = payload.iter().position(|byte| *byte > 127).unwrap() + 1;
        socket
            .send(Message::Frame(Frame::message(
                payload[..split].to_vec(),
                OpCode::Data(Data::Text),
                false,
            )))
            .unwrap();
        socket
            .send(Message::Frame(Frame::message(
                payload[split..].to_vec(),
                OpCode::Data(Data::Continue),
                true,
            )))
            .unwrap();
    });
    assert_eq!(
        receive(
            &mut socket,
            Instant::now() + Duration::from_secs(1),
            MAX_BYTES
        )
        .unwrap()["text"],
        "测试"
    );
    worker.join().unwrap();
}

#[cfg(feature = "server")]
#[test]
fn unfinished_message_has_one_total_deadline() {
    let (mut socket, worker) = fixture(|socket| {
        socket
            .send(Message::Frame(Frame::message(
                b"{\"text\":\"".to_vec(),
                OpCode::Data(Data::Text),
                false,
            )))
            .unwrap();
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(15));
            if socket
                .send(Message::Frame(Frame::message(
                    vec![b'x'],
                    OpCode::Data(Data::Continue),
                    false,
                )))
                .is_err()
            {
                break;
            }
        }
    });
    let start = Instant::now();
    assert!(receive(&mut socket, start + Duration::from_millis(100), MAX_BYTES).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(socket);
    worker.join().unwrap();
}

#[cfg(feature = "server")]
#[test]
fn invalid_json_utf8_and_peer_close_fail_closed() {
    for message in [
        Message::Text("not-json".into()),
        Message::Frame(Frame::message(vec![0xff], OpCode::Data(Data::Text), true)),
        Message::Close(None),
    ] {
        let (mut socket, worker) = fixture(move |socket| {
            socket.send(message).unwrap();
        });
        assert!(receive(
            &mut socket,
            Instant::now() + Duration::from_secs(1),
            MAX_BYTES
        )
        .is_err());
        worker.join().unwrap();
    }
}
