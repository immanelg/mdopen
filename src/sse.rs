use crate::watch;

use tiny_http::{Header, Request, Response, StatusCode};

pub(crate) fn sse_emit_watcher_events(request: Request, watcher_bus: watch::WatcherBus) {
    let response = Response::from_data([])
        .with_status_code(StatusCode(200))
        .with_header("Content-Type: text/event-stream".parse::<Header>().unwrap())
        .with_header("Cache-Control: no-cache".parse::<Header>().unwrap())
        .with_header("X-Accel-Buffering: no".parse::<Header>().unwrap())
        .with_header("Connection: keep-alive".parse::<Header>().unwrap())
        .with_header("Content-Length: 64".parse::<Header>().unwrap()) // ?
        ;

    let httpver = request.http_version().clone();
    let mut writer = request.into_writer();
    response.raw_print(&mut writer, httpver, &[], true, None).unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));

    let mut watcher_rx = watcher_bus.write().unwrap().add_rx();

    loop {
        match watcher_rx.recv() {
            Ok(event) => {
                log::debug!("watcher_rx received: {:?} {:?}", event.kind, &event.paths);

                writer.write_all(b"event: update\ndata: {}\n\n").unwrap();
                writer.flush().unwrap();
                log::debug!("flused sse writer");
                break;
            }
            Err(err) => {
                log::error!("failed to recv event from bus: {}", err);
                break;
            }
        }
    }
}
