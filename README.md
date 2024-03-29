# mdopen
[![github]](https://github.com/immanelg/mdopen)
[![latest_version]][crates.io]
[![build_status](https://github.com/immanelg/mdopen/actions/workflows/rust.yml/badge.svg)](https://github.com/immanelg/mdopen/actions)
[![dependency_status](https://deps.rs/repo/github/immanelg/mdopen/status.svg)](https://deps.rs/repo/github/immanelg/mdopen)

[github]: https://img.shields.io/badge/github-immanelg/mdopen-8da0cb?logo=github
[latest_version]: https://img.shields.io/crates/v/mdopen.svg?logo=rust
[crates.io]: https://crates.io/crates/mdopen

Quickly preview local markdown files in browser with GitHub-like look. 

Doesn't use GitHub API, but just compiles markdown to HTML in Rust.

# Installation

Install from crates.io:

```sh
cargo install mdopen
```

or directly from this repo:
```sh
cargo install --git https://github.com/immanelg/mdopen.git
```

# Usage

Start the server and open files in Firefox:

```sh
mdopen README.md TODO.md -b firefox
```

This will open files on addresses `http://localhost:5032/README.md` and `http://localhost:5032/TODO.md`.

You access any markdown files relative to the current working directory.

You can also browse current directory if you access `/` or other directory path.

# Features
- [x] Compile GitHub-flavoured markdown to HTML 
- [x] Steal GitHub CSS, automatic dark/light mode
- [x] Open files in the default browser automatically
- [x] Directory listing / serve any files in the filesystem
- [x] Syntax Highlighting (via highlight.js)
- [x] Render LaTeX (via KaTeX)
- [ ] Output to standalone HTML files 
- [ ] Live reloading via WS/SSE (tiny_http -> hyper)

Feedback and pull requests are welcome.

# Acknowledgements
[grip](https://github.com/joeyespo/grip) is similar.

