use clap::Parser;
use log::{error, info};
use nanotemplate::template as render;
use percent_encoding::percent_decode;
use pulldown_cmark::{CowStr, Event, Tag, TagEnd};
use std::env;
use std::ffi::OsStr;
use std::fmt::{Debug, Write};
use std::fs;
use std::io::{self, Cursor};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

pub static INDEX: &str = include_str!("template/index.html");
pub static GITHUB_STYLE: &[u8] = include_bytes!("vendor/github.css");

pub static STATIC_PREFIX: &str = "/@/";

#[derive(Parser, Debug)]
#[command(name = "MDOpen", version = env!("CARGO_PKG_VERSION"), about = "Quickly preview local markdown files", long_about = None)]
struct Cli {
    #[arg(num_args = 0.., help = "Files to open")]
    files: Vec<String>,

    #[arg(short, long, default_value_t = 5032, help = "Port to serve")]
    port: u16,

    #[arg(short, long, help = "Browser to use for opening files")]
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

fn to_tag_anchor(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

fn to_html(md: &str) -> String {
    use pulldown_cmark::{Options, Parser};

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_MATH);
    options.insert(Options::ENABLE_GFM);

    let parser = Parser::new_ext(md, options);

    let mut inside_heading_level = None;

    let parser = parser.map(|event| match event {
        Event::Start(Tag::Heading { level, id, classes, attrs }) => {
            inside_heading_level = Some(level);
            Event::Start(Tag::Heading { level, id, classes, attrs })
        }
        Event::End(TagEnd::Heading(level)) => {
            inside_heading_level = None;
            Event::End(TagEnd::Heading(level))
        }
        Event::Text(text) => {
            if inside_heading_level.is_some() {
                let anchor = to_tag_anchor(&text);
                Event::Html(CowStr::from(format!(r##"<a id="{anchor}" class="anchor" href="#{anchor}"><svg class="octicon octicon-link" viewBox="0 0 16 16" version="1.1" width="16" height="16" aria-hidden="true"><path d="m7.775 3.275 1.25-1.25a3.5 3.5 0 1 1 4.95 4.95l-2.5 2.5a3.5 3.5 0 0 1-4.95 0 .751.751 0 0 1 .018-1.042.751.751 0 0 1 1.042-.018 1.998 1.998 0 0 0 2.83 0l2.5-2.5a2.002 2.002 0 0 0-2.83-2.83l-1.25 1.25a.751.751 0 0 1-1.042-.018.751.751 0 0 1-.018-1.042Zm-4.69 9.64a1.998 1.998 0 0 0 2.83 0l1.25-1.25a.751.751 0 0 1 1.042.018.751.751 0 0 1 .018 1.042l-1.25 1.25a3.5 3.5 0 1 1-4.95-4.95l2.5-2.5a3.5 3.5 0 0 1 4.95 0 .751.751 0 0 1-.018 1.042.751.751 0 0 1-1.042.018 1.998 1.998 0 0 0-2.83 0l-2.5 2.5a1.998 1.998 0 0 0 0 2.83Z"></path></svg></a>{text}"##)))
            } else {
                Event::Text(text)
            }
        }
        _ => event,
    });

    let mut html_output = String::new();
    pulldown_cmark::html::push_html(&mut html_output, parser);

    return html_output;
}
fn serve_file(request: &Request) -> io::Result<Response<Cursor<Vec<u8>>>> {
    let cwd = env::current_dir()?;

    let url = percent_decode(request.url().as_bytes()).decode_utf8_lossy();
    let relative_path = Path::new(url.strip_prefix('/').expect("urls start with /"));
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

            let body = to_html(&md);

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
    env_logger::init();

    let cli = Cli::parse();

    let port = cli.port;
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let server = Server::http(addr).expect("start server");

    if !cli.files.is_empty() {
        thread::spawn(move || {
            for file in cli.files.into_iter() {
                let url = format!("http://localhost:{}/{}", &port, &file);
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
