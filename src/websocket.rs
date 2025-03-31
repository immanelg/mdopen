use std::thread;

use tiny_http::{Header, Request, Response};

use crate::watch;

// TODO: this should be SSE
// TODO: SSE should be connected to /$RELOAD/{path} and only get updated about what they are
// interested in.

/// Turns a Sec-WebSocket-Key into a Sec-WebSocket-Accept.
fn convert_websocket_key(input: &str) -> String {
    use base64::Engine as _;
    use sha1::{Digest, Sha1};
    const MAGIC_STRING: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

    let input = format!("{}{}", input, MAGIC_STRING);
    let output = <Sha1 as Digest>::digest(input);
    base64::engine::general_purpose::STANDARD.encode(output.as_slice())
}

pub(crate) fn accept_websocket(request: Request, watcher_bus: watch::WatcherBus) {
    if request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Upgrade"))
        .and_then(|hdr| {
            if hdr.value == "websocket" {
                Some(hdr)
            } else {
                None
            }
        })
        .is_none()
    {
        log::debug!("websocket accept failed: no 'Upgrade: websocket'");
        let response = tiny_http::Response::from_data("Expected 'Upgrade: websocket' header")
            .with_status_code(400);
        let _ = request.respond(response);
        return;
    };

    let key = match request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Sec-WebSocket-Key"))
        .map(|h| h.value.clone())
    {
        None => {
            log::debug!("websocket accept failed: no 'Sec-WebSocket-Key'");
            let response = tiny_http::Response::from_data("Expected 'Sec-WebSocket-Key' header")
                .with_status_code(400);
            let _ = request.respond(response);
            return;
        }
        Some(k) => k,
    };

    // building the "101 Switching Protocols" response
    let response = Response::new_empty(tiny_http::StatusCode(101))
        .with_header("Upgrade: websocket".parse::<tiny_http::Header>().unwrap())
        .with_header("Connection: Upgrade".parse::<tiny_http::Header>().unwrap())
        .with_header("Sec-WebSocket-Protocol: ping".parse::<Header>().unwrap())
        .with_header(
            format!(
                "Sec-WebSocket-Accept: {}",
                convert_websocket_key(key.as_str())
            )
            .parse::<Header>()
            .unwrap(),
        );

    let mut stream = request.upgrade("websocket", response);
    log::debug!("accepted websocket");
    let mut watcher_rx = watcher_bus.write().unwrap().add_rx();
    thread::spawn(move || match watcher_rx.recv() {
        Ok(event) => {
            log::debug!("subscriber received an event: {:?}", event);
            let msg = match event {
                watch::Event::Reload => "reload",
                watch::Event::Shutdown => "shutdown",
            };
            let frame = encode_frame(msg);
            stream.write_all(&frame).unwrap();
            stream.flush().unwrap();
            log::debug!("sent ws frame: {:?}", frame);
        }
        Err(err) => {
            log::error!("failed to recv event from bus: {}", err);
        }
    });
}

fn encode_frame(msg: &str) -> Vec<u8> {
    const FIRST_BYTE: u8 = 0x81;
    assert!(msg.len() < 126, "only tiny frames supported for now");
    let mut frame = vec![FIRST_BYTE, msg.len() as u8];
    frame.extend(msg.as_bytes());
    frame
}
