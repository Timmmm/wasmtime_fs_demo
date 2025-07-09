# Wasmtime WASI Custom Filesystem Demo

This is a demo of how to replace Wasmtime's default wasi-filesystem implementation (which just reads files from disk) with a custom one, allowing filesystem virtualisation.

It includes a simple WASI `ls` implementation for testing. To test run these commands (see `test.nu`):

    cargo build --release --target wasm32-wasip2 --package wasi_ls
    cp target\wasm32-wasip2\release\wasi_ls.wasm .
    cargo run

It will print a load of warnings and then

    bar
    foo

which are the contents of an imaginary filesystem. Most filesystem functions (e.g. actually opening/reading files) are `todo!()`.
