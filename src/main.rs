use clap::Parser;
use comrak::{markdown_to_html, Options};
use log::{error, info, warn};
use nanotemplate::template as render;
use std::env;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::fmt::Write;
use std::fs;
use std::io::{self, Cursor};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

pub static INDEX: &str = include_str!("template/index.html");
pub static GITHUB_STYLE: &[u8] = include_bytes!("vendor/github.css");

pub static STATIC_PREFIX: &str = "/@/";

pub static MD_EXTENSIONS: &[&str] = &["md", "markdown"];

#[derive(Parser, Debug)]
#[command(name = "MDOpen", version = "1.0", about = "Quickly preview local markdown files", long_about = None)]
struct Cli {
    #[clap(num_args = 1.., value_delimiter = ' ', help = "Files to open")]
    files: Vec<String>,

    #[clap(short, long, default_value_t = 5032, help = "Port to serve")]
    port: u16,

    #[arg(short, long)]
    browser: Option<String>,
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
    let html = render(INDEX, [("title", "mdopen"), ("body", &body)]).unwrap();
    html_response(html, 404)
}

fn internal_error_response() -> Response<Cursor<Vec<u8>>> {
    let body = "<h1>500 Internal Server Error</h1>";
    let html = render(INDEX, [("title", "mdopen"), ("body", &body)]).unwrap();
    html_response(html, 500)
}

fn matches_ext(ext: &OsStr, extensions: &[&str]) -> bool {
    let ext = ext.to_string_lossy();
    let ext = ext.as_ref();
    extensions.iter().any(|&want| ext == want)
}

/// Get content type from extension.
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
    let asset_url = request.url().strip_prefix(STATIC_PREFIX)?;

    let data = match asset_url {
        "style.css" => GITHUB_STYLE,
        _ => {
            info!("not found: {}", &asset_url);
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

    let relative_path = Path::new(request.url().strip_prefix('/').expect("urls start with /"));
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
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let entry_abs_path = entry.path();
            if !metadata.is_dir()
                && !matches_ext(
                    entry_abs_path.extension().unwrap_or_default(),
                    MD_EXTENSIONS,
                )
            {
                continue;
            }
            let entry_name = entry_abs_path
                .file_name()
                .expect("filepath")
                .to_string_lossy()
                .to_string();
            let href = relative_path
                .join(&entry_name)
                .to_string_lossy()
                .to_string();
            _ = write!(listing, "<li><a href='{}'>{}</a></li>", &href, &entry_name);
        }

        if listing.is_empty() {
            listing.push_str("Nothing to see here");
        }
        let listing = format!("<h1>Directory</h1><ul>{}</ul>", listing);
        let html = render(INDEX, [("title", title), ("body", &listing)]).unwrap();
        return Ok(html_response(html, 200));
    }

    if matches_ext(
        relative_path.extension().unwrap_or_default(),
        MD_EXTENSIONS,
    ) {
        let md = fs::read_to_string(&absolute_path)?;

        let mut md_options = Options::default();
        // allow inline HTML
        md_options.render.unsafe_ = true;
        md_options.extension.strikethrough = true;
        md_options.extension.table = true;
        md_options.extension.autolink = true;
        // md_options.extension.front_matter_delimiter = true;
        md_options.extension.tasklist = true;

        let body = markdown_to_html(&md, &md_options);

        let html = render(INDEX, [("title", title), ("body", &body)]).unwrap();
        return Ok(html_response(html, 200));
    }

    // if matches(
    //     relative_path.extension().unwrap_or(Default::default()),
    //     TEXT_EXTENSIONS,
    // ) {
    //     let file = fs::read(&absolute_path)?;
    //     return Ok(html_response(file, 200));
    // }

    let body = "<h1>Bad file!</h1>";
    let html = render(INDEX, [("title", title), ("body", &body)]).unwrap();
    Ok(html_response(html, 404))
}

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
        info!("method not allowed: {} {}", request.method(), request.url());
        return html_response("<h1>405 Method Not Allowed</h1>", 405);
    }

    if let Some(response) = maybe_asset_file(request) {
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

fn open_browser(browser: &Option<String>, url: &String) -> io::Result<()> {
    match browser {
        Some(ref browser) => open::with(&url, browser),
        None => open::that(&url),
    }
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    let port = cli.port;
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let server = Server::http(addr).expect("start server");

    if !cli.files.is_empty() {
        thread::spawn(move || {
            for file in cli.files.into_iter() {
                let url = format!("http://127.0.0.1:{}/{}", &port, &file);
                if let Err(e) = open_browser(&cli.browser, &url) {
                    error!("cannot open browser: {:?}", e);
                }
            }
        });
    }

    for request in server.incoming_requests() {
        info!("{} {}", request.method(), request.url());
        let resp = handle(&request);
        if let Err(e) = request.respond(resp) {
            error!("cannot send response: {}", e);
        };
    }
}
