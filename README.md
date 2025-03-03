# mdopen
[![github]](https://github.com/immanelg/mdopen)
[![latest_version]][crates.io]
[![build_status](https://github.com/immanelg/mdopen/actions/workflows/rust.yml/badge.svg)](https://github.com/immanelg/mdopen/actions)
[![dependency_status](https://deps.rs/repo/github/immanelg/mdopen/status.svg)](https://deps.rs/repo/github/immanelg/mdopen)

[github]: https://img.shields.io/badge/github-immanelg/mdopen-8da0cb?logo=github
[latest_version]: https://img.shields.io/crates/v/mdopen.svg?logo=rust
[crates.io]: https://crates.io/crates/mdopen

Quickly preview local markdown files in browser with GitHub-like look.

Doesn't use GitHub API, but locally compiles markdown to HTML in Rust and renders it for a browser.

Supports most of the GitHub markdown features, including syntax highlighting and math formulas.

Has GitHub-like CSS including automatic dark/light colorschemes.

Supports hot-reload.

# Installation

Install from main branch:
```sh
cargo install --git https://github.com/immanelg/mdopen.git
```

Install from crates.io:
```sh
cargo install mdopen
```

# Usage

Start the server and open served `README.md` in Firefox:
```sh
mdopen --browser firefox README.md 
```
Run `mdopen --help` for more info.

# Acknowledgements
[grip](https://github.com/joeyespo/grip) is similar.
