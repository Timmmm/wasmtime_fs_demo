# Wasmtime WASI Custom Filesystem Demo

This is a demo of how to replace Wasmtime's default wasi-filesystem implementation (which just reads files from disk) with a custom one, allowing filesystem virtualisation.

It includes a simple WASI `tree` implementation for testing. To test run these commands (see `test.nu`):

    cargo build --release --target wasm32-wasip2 --package wasi_ls
    cp target\wasm32-wasip2\release\wasi_ls.wasm .
    cargo run

It will print a load of warnings and then

    .
    ├── .cargo
    │   └── config.toml - Plain Text
    ├── .gitignore - Plain Text
    ├── Cargo.lock - Plain Text
    ...
    ├── wasi_ls
    │   ├── Cargo.toml - Plain Text
    │   └── src
    │       └── main.rs - Plain Text
    └── wasi_ls.wasm - WebAssembly Binary

which are the contents of the Git HEAD commit. It is exposed as a read-only filesystem so all write operations will fail. Also this is far from production quality - there are leaks, inefficiencies, TODOs, probably incorrect semantics (the WASI spec is approximately non-existent). But you should get the idea.
