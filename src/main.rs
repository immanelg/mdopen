use clap::Parser;
use comrak::{markdown_to_html, Options};
use log::{debug, error, info, warn};
use nanotemplate::template;
use simplelog::{Config, TermLogger};
use std::env;
use std::fmt::Debug;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::net::{IpAddr, Ipv4Addr};
use std::thread;
use tiny_http::{Header, Request, Response, Server, StatusCode};

pub static INDEX: &str = include_str!("template/index.html");
// pub static STYLE: &[u8] = include_bytes!("template/style.css");

// pub static STATIC_PREFIX: &str = "/@/";

#[derive(Parser, Debug)]
#[command(name = "MDOpen", version = "1.0", about = "quickly preview local markdown files", long_about = None)]
struct Args {
    #[clap(num_args = 1.., value_delimiter = ' ', help = "open files in web browser")]
    files: Vec<String>,

    #[clap(short, long, default_value_t = 5032, help = "port to serve")]
    port: u16,

    // #[arg(short, long, default_value_t = false)]
    // compile: bool,
}

fn respond<T: io::Read>(request: Request, response: Response<T>) {
    if let Err(e) = request.respond(response) {
        error!("cannot respond: {:?}", e)
    }
}

fn respond_html(request: Request, text: impl Into<Vec<u8>>, status: impl Into<StatusCode>) {
    let response = Response::from_data(text.into())
        .with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf8"[..]).unwrap(),
        )
        .with_status_code(status);
    respond(request, response);
}

fn respond_404_html(request: Request) {
    let body = "<h1>No such file</h1>";
    let html = template(INDEX, &[("title", "mdopen"), ("body", &body)]).unwrap();
    respond_html(request, html, 404);
}

fn handle_request(request: Request) {
    debug!("{} {}", request.method(), request.url());

    let client_addr = request.remote_addr().expect("tcp listener address");
    if !client_addr.ip().is_loopback() {
        warn!(
            "forbid request to {} from non-localhost address {}",
            request.url(),
            client_addr
        );
        respond_html(request, "<h1>Forbidden</h1>", 403);
        return;
    }

    // if let Some(asset_url) = request.url().strip_prefix(STATIC_PREFIX) {
    //     let data = match asset_url {
    //         "style.css" => STYLE,
    //         _ => return respond_404_html(request)
    //     };
    //
    //     let content_type: &[u8] = match Path::new(asset_url).extension().and_then(|s| s.to_str()) {
    //         Some("js") => b"application/javascript",
    //         Some("css") => b"text/css; charset=utf8",
    //         Some("gif") => b"image/gif",
    //         Some("png") => b"image/png",
    //         Some("jpg") | Some("jpeg") => b"image/jpeg",
    //         Some("pdf") => b"application/pdf",
    //         Some("html") => b"text/html; charset=utf8",
    //         Some("txt") => b"text/plain; charset=utf8",
    //         _ => b"text/plain; charset=utf8"
    //     };
    //
    //     let response = Response::from_data(data)
    //         .with_header(Header::from_bytes(&b"Content-Type"[..], content_type).unwrap())
    //         .with_header(Header::from_bytes(&b"Cache-Control"[..], &b"max-age=31536000"[..]).unwrap())
    //         .with_status_code(200);
    //     respond(request, response);
    //     return;
    // };
    //

    let cwd = env::current_dir().expect("current dir");
    let path = cwd.join(request.url().strip_prefix("/").expect("urls start with /"));

    if path.is_dir() {
        // TODO: list files in dir? 
        let body = "<h1>Is a directory</h1>";
        let html = template(INDEX, &[("title", "mdopen"), ("body", &body)]).unwrap();
        respond_html(request, html, 404);
        return;
    }

    if !path.exists() {
        respond_404_html(request);
        return;
    }

    if !path.extension().map(|s| s.to_ascii_lowercase()).map_or(false, |ext| ext == "md" || ext == "markdown") {
        let body = format!("<h1>Not a markdown file: {:?}</h1>", &path);
        let html = template(INDEX, &[("title", "mdopen"), ("body", &body)]).unwrap();
        respond_html(request, html, 404);
        return;
    }

    let md = match fs::read_to_string(&path) {
        Err(e) => {
            error!("cannot read file: {:?}", e);
            let body = format!("<h1>Cannot read file: {:?}</h1>", &path);
            let html = template(INDEX, &[("title", "mdopen"), ("body", &body)]).unwrap();
            respond_html(request, html, 500);
            return;
        }
        Ok(text) => text,
    };

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
    respond_html(request, html, 200);
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
        handle_request(request);
    }
}
