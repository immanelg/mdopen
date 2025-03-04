use std::net::{IpAddr, Ipv4Addr};

use lexopt::{
    Arg::{Long, Short, Value},
    ValueExt,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const USAGE: &str = include_str!("cli_help.txt");

#[derive(Debug)]
pub struct CommandArgs {
    pub files: Vec<String>,
    pub host: IpAddr,
    pub port: u16,
    pub browser: Option<String>,
    pub enable_reload: bool,
    pub enable_latex: bool,
    pub enable_syntax_highlight: bool,
}

impl CommandArgs {
    pub fn parse() -> Self {
        match parse_args() {
            Ok(args) => args,
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }
}

fn parse_args() -> Result<CommandArgs, lexopt::Error> {
    let mut args = CommandArgs {
        host: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        port: 5032,
        browser: None,
        files: Vec::new(),
        enable_latex: true,
        enable_reload: false,
        enable_syntax_highlight: true,
    };

    let mut parser = lexopt::Parser::from_env();

    while let Some(arg) = parser.next()? {
        match arg {
            Long("host") => {
                args.host = parser.value()?.parse()?;
            }
            Short('p') | Long("port") => {
                args.port = parser.value()?.parse()?;
            }
            Short('b') | Long("browser") => {
                args.browser = Some(parser.value()?.parse()?);
            }
            Long("enable-latex") => {
                args.enable_latex = true;
            }
            Long("disable-latex") => {
                args.enable_latex = false;
            }
            Long("enable-reload") => {
                args.enable_reload = true;
            }
            Long("disable-reload") => {
                args.enable_reload = false;
            }
            Long("enable-syntax-highlight") => {
                args.enable_syntax_highlight = true;
            }
            Long("disable-syntax-highlight") => {
                args.enable_syntax_highlight = false;
            }
            Value(val) => {
                args.files.push(val.parse()?);
            }
            Short('v') | Long("version") => {
                eprintln!("{}", VERSION);
                std::process::exit(0);
            }
            Short('h') | Long("help") => {
                eprintln!("mdopen {}", VERSION);
                eprintln!("{}", USAGE);
                std::process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    Ok(args)
}
