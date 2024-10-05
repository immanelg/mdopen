use lexopt::{
    Arg::{Long, Short, Value},
    ValueExt,
};

const VERSION: &'static str = env!("CARGO_PKG_VERSION");
const USAGE: &'static str =
    "usage: mdopen [-h|--help] [-v|--version] [-b|--browser BROWSER] [-p|--port PORT] [FILES...]";

#[derive(Debug)]
pub struct Args {
    pub files: Vec<String>,
    pub port: u16,
    pub browser: Option<String>,
}

impl Args {
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

fn parse_args() -> Result<Args, lexopt::Error> {
    let mut port = 5032;
    let mut browser = Option::<String>::None;
    let mut files = Vec::<String>::new();

    let mut parser = lexopt::Parser::from_env();

    while let Some(arg) = parser.next()? {
        match arg {
            Short('p') | Long("port") => {
                port = parser.value()?.parse()?;
            }
            Short('b') | Long("browser") => {
                browser = Some(parser.value()?.parse()?);
            }
            Value(val) => {
                files.push(val.parse()?);
            }
            Short('v') | Long("version") => {
                eprintln!("{}", VERSION);
                std::process::exit(0);
            }
            Short('h') | Long("help") => {
                eprintln!("{}", USAGE);
                std::process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    Ok(Args {
        browser,
        files,
        port,
    })
}
