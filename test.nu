#!/usr/bin/env nu

cargo build --release --target wasm32-wasip2 --package wasi_ls
cp target\wasm32-wasip2\release\wasi_ls.wasm .
cargo run
