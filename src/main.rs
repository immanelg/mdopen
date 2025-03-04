use log::{debug, error, info};
use minijinja::{context, Environment};
use notify::Watcher;
use percent_encoding::percent_decode;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Cursor};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

mod cli;
mod markdown;

pub static STYLE_CSS: &[u8] = include_bytes!("vendor/github.css");

pub static ASSETS_PREFIX: &str = "/__mdopen_assets/";
pub static RELOAD_PREFIX: &str = "/__mdopen_reload/";

fn html_response(
    text: impl Into<Vec<u8>>,
    status: StatusCode,
) -> Response<Cursor<Vec<u8>>> {
    Response::from_data(text.into())
        .with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf8"[..]).unwrap(),
        )
        .with_status_code(status)
}

fn error_response(error_code: StatusCode, jinja_env: &Environment) -> Response<Cursor<Vec<u8>>> {
    let tpl = jinja_env.get_template("error.html").unwrap();
    let html = tpl
        .render(context! {
            title => "Error",
            error_header => error_code.default_reason_phrase(),
        })
        .unwrap();
    html_response(html, StatusCode(404))
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
        "md" => Some("text/markdown"),
        "txt" => Some("text/plain"),
        _ => None,
    }
}

/// Returns response for static content request
fn handle_asset(path: &str, jinja_env: &Environment) -> Response<Cursor<Vec<u8>>> {
    let data = match path {
        "style.css" => STYLE_CSS,
        _ => {
            info!("asset not found: {}", &path);
            return error_response(StatusCode(404), jinja_env);
        }
    };
    

    Response::from_data(data)
        .with_header(Header::from_bytes(&b"Cache-Control"[..], &b"max-age=31536000"[..]).unwrap())
        .with_status_code(200)
}

// Get file contents for server response
// For directory, create listing in HTML
// For markdown, create generate HTML
// For other files, get its content
fn get_contents(
    path: &Path,
    config: &AppConfig,
    jinja_env: &Environment,
) -> io::Result<Vec<u8>> {
    let cwd = env::current_dir()?;

    let absolute_path = cwd.join(path);

    let file_path = absolute_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("mdopen");

    let Ok(metadata) = absolute_path.metadata() else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "not found"));
    };

    if metadata.is_dir() {
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
                let file_name = e.path()
                    .file_name()
                    .expect("filename")
                    .to_string_lossy()
                    .into_owned();
                let file_path = path.join(&file_name).to_string_lossy().to_string();
                DirItem { name: file_name, path: file_path }
            })
            .collect();
        let tpl = jinja_env.get_template("dir.html").unwrap();
        let html = tpl
            .render(context! {
                dir_path => path,
                files => files,
            })
            .unwrap();

        return Ok(html.into_bytes());
    }

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default();

    let data = fs::read(&absolute_path)?;

    let data = match ext {
        "md" | "markdown" => {
            let data = String::from_utf8_lossy(&data).to_string();
            let body = markdown::to_html(&data);

            let tpl = jinja_env.get_template("page.html").unwrap();
            let html = tpl
                .render(context! {
                    websocket_url => format!("ws://{}{}", config.addr, RELOAD_PREFIX), // FIXME: add file path
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
    Ok(data)

}
fn serve_file(
    url: &str,
    config: &AppConfig,
    jinja_env: &Environment,
) -> Response<Cursor<Vec<u8>>> {
    let path = PathBuf::from(percent_decode(url.as_bytes()).decode_utf8_lossy().into_owned());
    let path_rel = path.strip_prefix("/").expect("url should have / prefix");
    let contents = get_contents(path_rel, config, jinja_env);
    match contents {
        Ok(contents) => {
            let mut response = Response::from_data(contents).with_status_code(200);

            let ext = path_rel
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();

            // FIXME: should this be in get_contents()?
            let mime = match mime_type(ext) {
                Some("text/markdown") => Some("text/html"),
                m => m,
            };
            if let Some(mime) = mime {
                response = response.with_header(Header::from_bytes(&b"Content-Type"[..], mime).unwrap());
            }

            response
        },
        Err(err) => {
            if err.kind() == io::ErrorKind::NotFound {
                error_response(StatusCode(404), jinja_env)
            } else {
                error_response(StatusCode(500), jinja_env)
            }
        }
    }
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

fn accept_websocket(request: Request, mut watcher_rx: WatcherBusReader)  {
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
            debug!("websocket accept failed: no 'Upgrade: websocket'");
            let response = tiny_http::Response::from_data("Expected 'Upgrade: websocket' header").with_status_code(400);
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
            debug!("websocket accept failed: no 'Sec-WebSocket-Key'");
            let response = tiny_http::Response::from_data("Expected 'Sec-WebSocket-Key' header").with_status_code(400);
            let _ = request.respond(response);
            return;
        }
        Some(k) => k,
    };

    // building the "101 Switching Protocols" response
    let response = Response::new_empty(tiny_http::StatusCode(101))
        .with_header("Upgrade: websocket".parse::<tiny_http::Header>().unwrap())
        .with_header("Connection: Upgrade".parse::<tiny_http::Header>().unwrap())
        .with_header(
            "Sec-WebSocket-Protocol: ping"
                .parse::<Header>()
                .unwrap(),
        )
        .with_header(
            format!(
                "Sec-WebSocket-Accept: {}",
                convert_websocket_key(key.as_str())
            )
            .parse::<Header>()
            .unwrap(),
        );

    let mut stream = request.upgrade("websocket", response);
    debug!("accepted websocket");
    thread::spawn(move || loop {
        let hello_frame = &[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]; // TODO: uhhhhhhh
        use notify::EventKind as Kind;
        match watcher_rx.recv() {
            Ok(event) =>  {
                match event.kind {
                    Kind::Remove(_) | Kind::Create(_) | Kind::Modify(_) => {
                        debug!("watcher change: {:?} {:?}", event.kind, &event.paths);
                        stream.write_all(hello_frame).unwrap();
                        stream.flush().unwrap();
                        return;
                    }
                    Kind::Access(_) | Kind::Other | Kind::Any => {}
                }
            }
            Err(err) => {
                error!("failed to recv event from bus: {}", err);
                return;
            }
        }
    });
}

/// Route a request and respond to it.
fn handle(request: Request, config: &AppConfig, watcher_rx: WatcherBusReader, jinja_env: &Environment) {
    if request.method() != &Method::Get {
        let response = error_response(StatusCode(405), jinja_env);
        let _ = request.respond(response);
        return;
    }
    let url = request.url().to_owned();

    if let Some(_path) = url.strip_prefix(RELOAD_PREFIX) {
        accept_websocket(request, watcher_rx);
        return;
    } 
    let response = if let Some(path) = url.strip_prefix(ASSETS_PREFIX) {
        handle_asset(path, jinja_env)
    } else {
        serve_file(&url, config, jinja_env)
    };
    if let Err(err) = request.respond(response) {
        error!("cannot respond: {}", err);
    };
}

fn open_browser(browser: &Option<String>, url: &str) -> io::Result<()> {
    match browser {
        Some(ref browser) => open::with(url, browser),
        None => open::that(url),
    }
}

type WatcherBusReader = bus::BusReader<notify::Event>;

struct AppConfig {
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

    let watcher_bus = Arc::new(RwLock::new(bus::Bus::new(8)));

    let watcher_bus_notify = watcher_bus.clone();
    let mut watcher = notify::RecommendedWatcher::new(
        move |event| {
            if let Ok(event) = event {
                let mut watcher_bus = watcher_bus_notify.write().unwrap();
                watcher_bus.broadcast(event);
            }
        },
        notify::Config::default(),
    )
    .unwrap();

    if config.enable_reload {
        watcher.watch(".".as_ref(), notify::RecursiveMode::Recursive).unwrap();
        debug!("watching directory: .");
        // NOTE: https://github.com/notify-rs/notify/issues/247
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
        debug!("{} {}", request.method(), request.url());
        let watcher_rx = watcher_bus.write().unwrap().add_rx();
        handle(request, &config, watcher_rx, &jinja_env);
    }
}
