# mdopen
Quickly preview local markdown files in browser with GitHub-like look. Written in Rust.

# Installation

## From source

Build with `cargo`:

```sh
git clone https://github.com/immanelg/mdopen --depth=1
cd mdopen
cargo install --path .
```

This will install `mdopen` binary to `~/.cargo/bin`.

# Usage

Start the server and open files in the default web browser:

```sh
mdopen README.md TODO.md
```

You can access any files in current working directory from `http://localhost:5032/`. If you access a directory instead of a markdown file, you will see a directory listing.

# TODO
- Support cool markdown features (syntax highlighting, LaTeX, etc)
- Make this a static website / documentation generator
- File watcher, live reloading (probably need to use an async HTTP library like Hyper to have websockets / SSE)

