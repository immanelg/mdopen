use std::net::SocketAddr;

pub(crate) struct AppConfig {
    pub addr: SocketAddr,
    pub enable_reload: bool,
    pub enable_latex: bool,
    pub enable_syntax_highlight: bool,
}
