use log::{debug, error, info};
use nanotemplate::template as render;
use percent_encoding::percent_decode;
use std::env;
use std::ffi::OsStr;
use std::fmt::Write;
use std::fs;
use std::io::{self, Cursor};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

mod markdown;
mod cli;

pub static INDEX: &str = include_str!("template/index.html");
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

fn not_found_response() -> Response<Cursor<Vec<u8>>> {
    let body = "<h1>404 Not Found</h1>";
    let html = render(INDEX, [("title", "mdopen"), ("body", body)]).unwrap();
    html_response(html, 404)
}

fn internal_error_response() -> Response<Cursor<Vec<u8>>> {
    let body = "<h1>500 Internal Server Error</h1>";
    let html = render(INDEX, [("title", "mdopen"), ("body", body)]).unwrap();
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

fn serve_file(request: &Request) -> io::Result<Response<Cursor<Vec<u8>>>> {
    let cwd = env::current_dir()?;

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
        let html = render(INDEX, [("title", title), ("body", &listing)]).unwrap();
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

            let html = render(INDEX, [("title", title), ("body", &body)]).unwrap();
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

/// Construct HTML response for request.
fn handle(request: &Request) -> Response<Cursor<Vec<u8>>> {
    if request.method() != &Method::Get {
        info!("method not allowed: {} {}", request.method(), request.url());
        return html_response("<h1>405 Method Not Allowed</h1>", 405);
    }

    if let Some(response) = try_asset_file(request) {
        return response;
    };

    match serve_file(request) {
        Ok(r) => r,
        Err(err) => {
            error!("cannot serve file: {}", err);
            internal_error_response()
        }
    }
}

fn open_browser(browser: &Option<String>, url: &str) -> io::Result<()> {
    match browser {
        Some(ref browser) => open::with(url, browser),
        None => open::that(url),
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = cli::Args::parse();

    let port = args.port;
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let server = match Server::http(addr) {
        Ok(s) => s,
        Err(e) => {
            error!("cannot start server: {}", e);
            return;
        }
    };

    info!("serving at http://{}", addr);

    if !args.files.is_empty() {
        thread::spawn(move || {
            for file in args.files.into_iter() {
                let url = format!("http://localhost:{}/{}", &port, &file);
                info!("opening {}", &url);
                if let Err(e) = open_browser(&args.browser, &url) {
                    error!("cannot open browser: {}", e);
                }
            }
        });
    }

    for request in server.incoming_requests() {
        debug!("{} {}", request.method(), request.url());
        let resp = handle(&request);
        if let Err(e) = request.respond(resp) {
            error!("cannot send response: {}", e);
        };
    }
}
