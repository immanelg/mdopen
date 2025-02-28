use log::{debug, error, info};
use nanotemplate::template as render;
use notify::Watcher;
use percent_encoding::percent_decode;
use std::env;
use std::ffi::OsStr;
use std::fmt::Write;
use std::fs;
use std::io::{self, Cursor};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

mod cli;
mod markdown;

pub static INDEX: &str = include_str!("template/index.html");
pub static GITHUB_STYLE: &[u8] = include_bytes!("vendor/github.css");

pub static STATIC_PREFIX: &str = "/@/";

const TEMPLATE_VAR_WS_URL: &str = "ws_url";

fn html_response(
    text: impl Into<Vec<u8>>,
    status: impl Into<StatusCode>,
) -> Response<Cursor<Vec<u8>>> {
    Response::from_data(text.into())
        .with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf8"[..]).unwrap(),
        )
        .with_status_code(status)
}

fn not_found_response() -> Response<Cursor<Vec<u8>>> {
    let body = "<h1>404 Not Found</h1>";
    let html = render(
        INDEX,
        [
            ("title", "mdopen"),
            ("body", body),
            (TEMPLATE_VAR_WS_URL, "null"),
        ],
    )
    .unwrap();
    html_response(html, 404)
}

fn internal_error_response() -> Response<Cursor<Vec<u8>>> {
    let body = "<h1>500 Internal Server Error</h1>";
    let html = render(
        INDEX,
        [
            ("title", "mdopen"),
            ("body", body),
            (TEMPLATE_VAR_WS_URL, "null"),
        ],
    )
    .unwrap();
    html_response(html, 500)
}

/// Returns response for static content request
fn try_asset_file(request: &Request) -> Option<Response<Cursor<Vec<u8>>>> {
    let asset_url = request.url().strip_prefix(STATIC_PREFIX)?;

    let data = match asset_url {
        "style.css" => GITHUB_STYLE,
        _ => {
            info!("not found: {}", &asset_url);
            return Some(not_found_response());
        }
    };
    let resp = Response::from_data(data)
        .with_header(Header::from_bytes(&b"Cache-Control"[..], &b"max-age=31536000"[..]).unwrap())
        .with_status_code(200);

    Some(resp)
}

/// Get content type from extension.
fn mime_type(ext: &str) -> Option<&'static str> {
    match ext {
        "js" => Some("application/javascript"),
        "css" => Some("text/css"),
        "gif" => Some("image/gif"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "pdf" => Some("application/pdf"),
        "html" => Some("text/html"),
        "txt" => Some("text/plain"),
        _ => None,
    }
}

fn serve_file(
    request: &Request,
    server_addr: &SocketAddr,
) -> io::Result<Response<Cursor<Vec<u8>>>> {
    let cwd = env::current_dir()?;

    let websocket_url = format!("\"ws://{}:{}\"", server_addr.ip(), server_addr.port());

    let url = percent_decode(request.url().as_bytes()).decode_utf8_lossy();
    let relative_path = Path::new(url.as_ref())
        .strip_prefix("/")
        .expect("url should have / prefix");
    let absolute_path = cwd.join(relative_path);

    let title = absolute_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("mdopen");

    if !absolute_path.exists() {
        info!("not found: {}", request.url());
        return Ok(not_found_response());
    }

    if absolute_path.is_dir() {
        let entries = fs::read_dir(&absolute_path)?;

        let mut listing = String::new();

        for entry in entries {
            let Ok(entry) = entry else {
                continue;
            };
            let entry_abs_path = entry.path();
            let entry_name = entry_abs_path
                .file_name()
                .expect("filepath")
                .to_string_lossy()
                .to_string();
            let href = relative_path
                .join(&entry_name)
                .to_string_lossy()
                .to_string();
            _ = write!(listing, "<li><a href='/{}'>{}</a></li>", &href, &entry_name);
        }

        if listing.is_empty() {
            listing.push_str("Nothing to see here");
        }
        let listing = format!("<h1>Directory</h1><ul>{}</ul>", listing);
        let html = render(
            INDEX,
            [
                ("title", title),
                ("body", &listing),
                (TEMPLATE_VAR_WS_URL, &websocket_url),
            ],
        )
        .unwrap();
        return Ok(html_response(html, 200));
    }

    let ext = relative_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default();

    let mut mime = mime_type(ext);

    let data = fs::read(&absolute_path)?;

    let data = match ext {
        "md" | "markdown" => {
            mime = Some("text/html");

            let md = String::from_utf8_lossy(&data).to_string();
            let body = markdown::to_html(&md);

            let html = render(
                INDEX,
                [
                    ("title", title),
                    ("body", &body),
                    (TEMPLATE_VAR_WS_URL, &websocket_url),
                ],
            )
            .unwrap();
            html.into()
        }
        _ => data,
    };

    let resp = Response::from_data(data).with_status_code(200);
    let resp = if let Some(mime) = mime {
        resp.with_header(Header::from_bytes(&b"Content-Type"[..], mime).unwrap())
    } else {
        resp
    };

    Ok(resp)
}

/// Turns a Sec-WebSocket-Key into a Sec-WebSocket-Accept.
fn convert_websocket_key(input: &str) -> String {
    use base64::Engine as _;
    use sha1::{Digest, Sha1};
    const MAGIC_STRING: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

    let input = format!("{}{}", input, MAGIC_STRING);
    let output = <Sha1 as Digest>::digest(input);
    base64::engine::general_purpose::STANDARD.encode(output.as_slice())
}

enum AcceptWebsocketResult {
    Continue(Request),
    Accepted,
}

fn accept_websocket_or_continue(request: Request, mut reader: Reader) -> AcceptWebsocketResult {
    match request
        .headers()
        .iter()
        .find(|h| h.field.equiv(&"Upgrade"))
        .and_then(|hdr| {
            if hdr.value == "websocket" {
                Some(hdr)
            } else {
                None
            }
        }) {
        None => {
            // Not websocket
            return AcceptWebsocketResult::Continue(request);
        }
        _ => (),
    };

    let key = match request
        .headers()
        .iter()
        .find(|h| h.field.equiv(&"Sec-WebSocket-Key"))
        .map(|h| h.value.clone())
    {
        None => {
            let response = tiny_http::Response::from_data(&[]).with_status_code(400);
            let _ = request.respond(response);
            return AcceptWebsocketResult::Accepted;
        }
        Some(k) => k,
    };

    // building the "101 Switching Protocols" response
    let response = tiny_http::Response::new_empty(tiny_http::StatusCode(101))
        .with_header("Upgrade: websocket".parse::<tiny_http::Header>().unwrap())
        .with_header("Connection: Upgrade".parse::<tiny_http::Header>().unwrap())
        .with_header(
            "Sec-WebSocket-Protocol: ping"
                .parse::<tiny_http::Header>()
                .unwrap(),
        )
        .with_header(
            format!(
                "Sec-WebSocket-Accept: {}",
                convert_websocket_key(key.as_str())
            )
            .parse::<tiny_http::Header>()
            .unwrap(),
        );

    let mut stream = request.upgrade("websocket", response);
    info!("connected to websocket");
    thread::spawn(move || loop {
        let hello_frame = &[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
        match reader.recv() {
            Ok(event) => {
                debug!("event: {:?}", event);
                assert!(hello_frame.len() > 0);
                stream.write_all(hello_frame).unwrap();
                stream.flush().unwrap();
            }
            Err(err) => {
                eprintln!("failed to recv event from bus: {}", err);
            }
        }
    });
    AcceptWebsocketResult::Accepted
}

/// Route a request and respond to it.
fn handle(request: Request, server_addr: &SocketAddr, reader: Reader) {
    if request.method() != &Method::Get {
        info!("method not allowed: {} {}", request.method(), request.url());
        let _ = request.respond(html_response("<h1>405 Method Not Allowed</h1>", 405));
        return;
    }

    if let Some(response) = try_asset_file(&request) {
        let _ = request.respond(response);
        return;
    };

    let request = match accept_websocket_or_continue(request, reader) {
        AcceptWebsocketResult::Accepted => return,
        AcceptWebsocketResult::Continue(request) => request,
    };

    match serve_file(&request, server_addr) {
        Ok(r) => {
            let _ = request.respond(r);
        }
        Err(err) => {
            error!("cannot serve file: {}", err);
            let _ = request.respond(internal_error_response());
        }
    }
}

fn open_browser(browser: &Option<String>, url: &str) -> io::Result<()> {
    match browser {
        Some(ref browser) => open::with(url, browser),
        None => open::that(url),
    }
}

type Event = notify::Event;
type Reader = bus::BusReader<Event>;

fn broadcaster<T>(len: usize) -> Arc<Mutex<bus::Bus<T>>> {
    let bus = bus::Bus::new(len);
    Arc::new(Mutex::new(bus))
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = cli::Args::parse();

    let host = args.host;
    let port = args.port;
    let addr = SocketAddr::new(host, port);

    let server = match Server::http(addr) {
        Ok(s) => s,
        Err(e) => {
            error!("cannot start server: {}", e);
            return;
        }
    };

    info!("serving at http://{}", addr);

    let bus = broadcaster(100);
    let incoming_bus = bus.clone();

    let mut watcher = notify::RecommendedWatcher::new(
        move |result: Result<notify::Event, notify::Error>| {
            if let Ok(event) = result {
                let mut bus = bus.lock().unwrap();
                debug!("broadcasting event: {:?}", event);
                bus.try_broadcast(event).unwrap();
            }
        },
        notify::Config::default(),
    )
    .unwrap();

    for file in args.files.iter() {
        let path = Path::new(file);
        watcher
            .watch(&path, notify::RecursiveMode::Recursive)
            .unwrap();
    }

    if !args.files.is_empty() {
        thread::spawn(move || {
            for file in args.files.into_iter() {
                let url = format!("http://{}:{}/{}", &host, &port, &file);
                info!("opening {}", &url);
                if let Err(e) = open_browser(&args.browser, &url) {
                    error!("cannot open browser: {}", e);
                }
            }
        });
    }

    for request in server.incoming_requests() {
        debug!("{} {}", request.method(), request.url());
        let reader = {
            let mut bus = incoming_bus.lock().unwrap();
            bus.add_rx()
        };
        handle(request, &addr, reader);
    }
}
