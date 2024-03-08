use clap::Parser;
use comrak::{markdown_to_html, Options};
use log::{debug, error, info, warn};
use nanotemplate::template;
use simplelog::{Config, TermLogger};
use std::env;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::fs;
use std::io;
use std::io::Cursor;
use std::net::SocketAddr;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::thread;
use tiny_http::Method;
use tiny_http::{Header, Request, Response, Server, StatusCode};

pub static INDEX: &str = include_str!("template/index.html");
pub static GITHUB_STYLE: &[u8] = include_bytes!("vendor/github.css");

pub static STATIC_PREFIX: &str = "/@/";

#[derive(Parser, Debug)]
#[command(name = "MDOpen", version = "1.0", about = "Quickly preview local markdown files", long_about = None)]
struct Args {
    #[clap(num_args = 1.., value_delimiter = ' ', help = "Open files in web browser")]
    files: Vec<String>,

    #[clap(short, long, default_value_t = 5032, help = "port to serve")]
    port: u16,

    // #[clap(short, long, help = "base directory")]
    // directory: String,

    // #[arg(short, long, default_value_t = false)]
    // compile: bool,
}

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
    let html = template(INDEX, &[("title", "mdopen"), ("body", &body)]).unwrap();
    return html_response(html, 404);
}

fn internal_error_response() -> Response<Cursor<Vec<u8>>> {
    let body = "<h1>500 Internal Server Error</h1>";
    let html = template(INDEX, &[("title", "mdopen"), ("body", &body)]).unwrap();
    return html_response(html, 500);
}

fn mime_type(ext: &str) -> &'static str {
    match ext {
        "js" => "application/javascript",
        "css" => "text/css",
        "gif" => "image/gif",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "pdf" => "application/pdf",
        "html" => "text/html",
        "txt" => "text/plain",
        _ => "text/plain",
    }
}

/// If request wants to get a static file (like CSS), constructs response for it (including 404
/// response). Otherwise returns None.
fn maybe_asset_file(request: &Request) -> Option<Response<Cursor<Vec<u8>>>> {

    let Some(asset_url) = request.url().strip_prefix(STATIC_PREFIX) else {
        return None;
    };

    let data = match asset_url {
        "style.css" => STYLE,
        _ => {
            warn!("asset not found: {}", &asset_url);
            return Some(not_found_response());
        }
    };

    let content_type = Path::new(asset_url)
        .extension()
        .and_then(|s| s.to_str())
        .map_or("", mime_type);

    Some(
        Response::from_data(data)
            .with_header(Header::from_bytes(&b"Content-Type"[..], content_type).unwrap())
            .with_header(
                Header::from_bytes(&b"Cache-Control"[..], &b"max-age=31536000"[..]).unwrap(),
            )
            .with_status_code(200),
    )
}

/// Tries to read and compile markdown file and construct a response for it.
fn serve_file(request: &Request) -> io::Result<Response<Cursor<Vec<u8>>>> {
    let cwd = env::current_dir()?;

    let path = cwd.join(request.url().strip_prefix("/").expect("urls start with /"));

    let title = path.file_name().and_then(OsStr::to_str).unwrap_or("mdopen");

    if !path.exists() {
        return Ok(not_found_response());
    }

    if path.is_dir() {
        let entries = fs::read_dir(&path)?;

        let body = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| 
                entry
                    .path()
                    .file_name()
                    .and_then(OsStr::to_str)
                    .map(|s| s.to_owned())
                    .unwrap_or("<file>".to_owned())
            )
            .fold(String::from("<h1>Directory</h1>"), |a, b| format!("<p>{}</p>", a + &b));

        let html = template(INDEX, &[("title", title), ("body", &body)]).unwrap();
        return Ok(html_response(html, 200));
    }

    let ext = path.extension().and_then(OsStr::to_str).unwrap_or("");

    if !(ext == "md" || ext == "markdown") {
        let body = format!("<h1>Not a markdown file</h1>");
        let html = template(INDEX, &[("title", title), ("body", &body)]).unwrap();
        return Ok(html_response(html, 404));
    }

    let md = fs::read_to_string(&path)?; 

    let filename = path
        .file_name()
        .unwrap_or(Default::default())
        .to_str()
        .unwrap_or("");

    let mut md_options = Options::default();
    // allow inline HTML
    md_options.render.unsafe_ = true;

    let body = markdown_to_html(&md, &md_options);

    let html = template(INDEX, &[("title", filename), ("body", &body)]).unwrap();
    return Ok(html_response(html, 200));
}

// fn ensure_loopback(request: &Request) -> Option<Response<Vec<u8>>> {
//     let client_addr = request.remote_addr().expect("tcp listener address");
//     if !client_addr.ip().is_loopback() {
//         warn!(
//             "forbid request to {} from non-localhost address {}",
//             request.url(),
//             client_addr
//         );
//         response_html(request, "<h1>Forbidden</h1>", 403);
//         return;
//     }

/// Construct HTML response for request.
fn handle(request: &Request) -> Response<Cursor<Vec<u8>>> {
    let client_addr = request.remote_addr().expect("tcp listener address");
    if !client_addr.ip().is_loopback() {
        warn!(
            "request to {} from non-loopback address {}",
            request.url(),
            client_addr
        );
        return html_response("<h1>403 Forbidden</h1>", 403);
    }

    if request.method() != &Method::Get {
        return html_response("<h1>405 Method Not Allowed</h1>", 405);
    }

    if let Some(response) = maybe_asset_file(&request) {
        return response;
    };

    match serve_file(&request) {
        Ok(r) => r,
        Err(err) => {
            error!("cannot serve file: {}", err);
            return internal_error_response();
        }
    }
}

fn main() {
    TermLogger::init(
        simplelog::LevelFilter::Debug,
        Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .unwrap();

    let args = Args::parse();

    let port = args.port;
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let server = Server::http(&addr).expect("start server");

    if !args.files.is_empty() {
        thread::spawn(move || {
            for file in args.files.into_iter() {
                let url = format!("http://localhost:{}/{}", &port, &file);
                if let Err(e) = webbrowser::open(&url) {
                    error!("cannot open browser: {:?}", e);
                }
            }
        });
    }
    // debug!("compile? {:?}", args.compile);

    for request in server.incoming_requests() {
        info!("{} {}", request.method(), request.url());
        let resp = handle(&request);
        if let Err(e) = request.respond(resp) {
            error!("cannot send response: {}", e);
        };
    }
    info!("shutting down");
}
