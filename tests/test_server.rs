use std::{
    env,
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

#[test]
fn test_index() {
    let url = http_url_from_env();
    let resp = reqwest::blocking::get(url).unwrap();
    assert_eq!(resp.status(), 200);
}

#[test]
fn test_readme() {
    let base_url = http_url_from_env();
    let url = format!("{}/{}", base_url, "README.md");
    let resp = reqwest::blocking::get(url).unwrap();
    assert_eq!(resp.status(), 200);
}

fn http_url_from_env() -> String {
    let addr = addr_from_env();
    format!("http://{}:{}", addr.ip(), addr.port())
}

fn addr_from_env() -> SocketAddr {
    let host = env::var("HOST").unwrap();
    let port = env::var("PORT").unwrap();

    let host = IpAddr::from_str(&host).unwrap();
    let port = u16::from_str(&port).unwrap();

    SocketAddr::new(host, port)
}
