use log::{debug, error, info};
use minijinja::{context, Environment};
use notify::Watcher;
use percent_encoding::percent_decode;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Cursor};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

mod cli;
mod markdown;

pub static GITHUB_STYLE: &[u8] = include_bytes!("vendor/github.css");

pub static STATIC_PREFIX: &str = "/@/";

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

fn not_found_response(jinja_env: &Environment) -> Response<Cursor<Vec<u8>>> {
    let tpl = jinja_env.get_template("error.html").unwrap();
    let html = tpl
        .render(context! {
            title => "Not Found",
            error_header => "404 File Not Found",
        })
        .unwrap();
    html_response(html, 404)
}

fn internal_error_response(jinja_env: &Environment) -> Response<Cursor<Vec<u8>>> {
    let tpl = jinja_env.get_template("error.html").unwrap();
    let html = tpl
        .render(context! {
            title => "Error!",
            error_header => "500 Internal Server Error",
        })
        .unwrap();
    html_response(html, 500)
}

/// Returns response for static content request
fn try_asset_file(request: &Request, jinja_env: &Environment) -> Option<Response<Cursor<Vec<u8>>>> {
    let asset_url = request.url().strip_prefix(STATIC_PREFIX)?;

    let data = match asset_url {
        "style.css" => GITHUB_STYLE,
        _ => {
            info!("not found: {}", &asset_url);
            return Some(not_found_response(jinja_env));
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

fn serve_file(request: &Request, config: &AppConfig, jinja_env: &Environment) -> io::Result<Response<Cursor<Vec<u8>>>> {
    let cwd = env::current_dir()?;

    let url = percent_decode(request.url().as_bytes()).decode_utf8_lossy();
    let relative_path = Path::new(url.as_ref())
        .strip_prefix("/")
        .expect("url should have / prefix");
    let absolute_path = cwd.join(relative_path);

    let file_path = absolute_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("mdopen");

    if !absolute_path.exists() {
        info!("not found: {}", request.url());
        return Ok(not_found_response(jinja_env));
    }

    if absolute_path.is_dir() {
        let entries = fs::read_dir(&absolute_path)?;

        #[derive(serde::Serialize)]
        struct DirItem {
            pub name: String,
            pub path: String,
            // metadata? dont care
        }
        let files: Vec<DirItem> = entries
            .filter_map(|e| e.ok())
            .map(|e| {
                let path = e.path();
                let name = path
                    .file_name()
                    .expect("filename")
                    .to_string_lossy()
                    .into_owned();
                let path = relative_path.join(&name).to_string_lossy().to_string();
                DirItem { name, path }
            })
            .collect();
        let tpl = jinja_env.get_template("dir.html").unwrap();
        let html = tpl
            .render(context! {
                dir_path => relative_path,
                files => files,
            })
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

            let data = String::from_utf8_lossy(&data).to_string();
            let body = markdown::to_html(&data);

            let tpl = jinja_env.get_template("page.html").unwrap();
            let html = tpl
                .render(context! {
                    websocket_url => config.addr,
                    title => file_path,
                    markdown_body => body,
                    enable_syntax_highlight => config.enable_syntax_highlight,
                    enable_latex => config.enable_latex,
                    enable_reload => config.enable_reload,
                })
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

fn accept_websocket_or_continue(request: Request, mut reader: BusReader) -> AcceptWebsocketResult {
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
        // Not websocket
        return AcceptWebsocketResult::Continue(request);
    };

    let key = match request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Sec-WebSocket-Key"))
        .map(|h| h.value.clone())
    {
        None => {
            let response = tiny_http::Response::from_data([]).with_status_code(400);
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
    debug!("connected to websocket");
    thread::spawn(move || loop {
        let hello_frame = &[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
        use notify::EventKind as K;
        match reader.recv() {
            Ok(event) => match event.kind {
                K::Remove(_) | K::Create(_) | K::Modify(_) => {
                    debug!("modification event: {:?}", event);
                    stream.write_all(hello_frame).unwrap();
                    stream.flush().unwrap();
                    return;
                }
                _ => {}
            },
            Err(err) => {
                error!("failed to recv event from bus: {}", err);
            }
        }
    });
    AcceptWebsocketResult::Accepted
}

/// Route a request and respond to it.
fn handle(request: Request, config: &AppConfig, reader: BusReader, jinja_env: &Environment) {
    if request.method() != &Method::Get {
        info!("method not allowed: {} {}", request.method(), request.url());
        let _ = request.respond(html_response("<h1>405 Method Not Allowed</h1>", 405));
        return;
    }

    if let Some(response) = try_asset_file(&request, jinja_env) {
        let _ = request.respond(response);
        return;
    };

    let request = match accept_websocket_or_continue(request, reader) {
        AcceptWebsocketResult::Accepted => return,
        AcceptWebsocketResult::Continue(request) => request,
    };

    match serve_file(&request, &config, jinja_env) {
        Ok(r) => {
            let _ = request.respond(r);
        }
        Err(err) => {
            error!("cannot serve file: {}", err);
            let _ = request.respond(internal_error_response(jinja_env));
        }
    }
}

fn open_browser(browser: &Option<String>, url: &str) -> io::Result<()> {
    match browser {
        Some(ref browser) => open::with(url, browser),
        None => open::that(url),
    }
}

type BusReader = bus::BusReader<notify::Event>;

fn broadcaster<T>(len: usize) -> Arc<Mutex<bus::Bus<T>>> {
    let bus = bus::Bus::new(len);
    Arc::new(Mutex::new(bus))
}

struct AppConfig {
    //jinja_env: Environment<'_>,
    addr: SocketAddr,
    enable_reload: bool,
    enable_latex: bool,
    enable_syntax_highlight: bool,
}

struct AppContext<'c> {
    jinja: minijinja::Environment<'c>,
    //bus: bus::Bus,
    config: AppConfig,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = cli::CommandArgs::parse();
    let config = AppConfig {
        addr: SocketAddr::new(args.host, args.port), 
        enable_reload: args.enable_reload,
        enable_latex: args.enable_latex,
        enable_syntax_highlight: args.enable_syntax_highlight,
    };

    let server = match Server::http(config.addr) {
        Ok(s) => s,
        Err(e) => {
            error!("cannot start server: {}", e);
            return;
        }
    };

    info!("serving at http://{}", config.addr);

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
            .watch(path, notify::RecursiveMode::Recursive)
            .unwrap();
        // FIXME: opening a directory is watching the whole dir and spams messages
        // FIXME: unwraps File Not Found
        // FIXME: https://github.com/notify-rs/notify/issues/247
    }

    if !args.files.is_empty() {
        thread::spawn(move || {
            for file in args.files.into_iter() {
                let url = format!("http://{}/{}", &config.addr, &file);
                info!("opening {}", &url);
                if let Err(e) = open_browser(&args.browser, &url) {
                    error!("cannot open browser: {}", e);
                }
            }
        });
    }

    let mut jinja_env = Environment::new();
    jinja_env.set_auto_escape_callback(|_filename| minijinja::AutoEscape::None);
    jinja_env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    jinja_env
        .add_template("base.html", include_str!("template/base.html"))
        .unwrap();
    jinja_env
        .add_template("page.html", include_str!("template/page.html"))
        .unwrap();
    jinja_env
        .add_template("dir.html", include_str!("template/dir.html"))
        .unwrap();
    jinja_env
        .add_template("error.html", include_str!("template/error.html"))
        .unwrap();

    for request in server.incoming_requests() {
        debug!("request {} {}", request.method(), request.url());
        let reader = {
            let mut bus = incoming_bus.lock().unwrap();
            bus.add_rx()
        };
        handle(request, &config, reader, &jinja_env);
    }
}
