# mdopen
Quickly preview local markdown files in browser with GitHub-like look. 

Doesn't use GitHub API, but just compiles markdown to HTML in Rust.

# Installation

Install from crates.io:

```sh
cargo install mdopen
```

Or build from the main branch:

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

# TODO
- LaTeX
- Syntax highlighting for code blocks
- Live reloading (use async library like Hyper for WS or SSE)

## Ideas
- Make a simple static website generator from a directory of markdown files
- Make a neovim plugin for previewing markdown files like markdown-preview.nvim
- Make something like a file browser

# Acknowledgements
[grip](https://github.com/joeyespo/grip) is similar.
