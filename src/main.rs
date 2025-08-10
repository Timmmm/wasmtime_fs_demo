mod wasi_fs;
mod wasi_linker_excluding_filesystem;
mod wasi_state;

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use wasi_state::WasiState;
use wasmtime::{
    Engine, Store,
    component::{Component, Linker},
};
use wasmtime_wasi::{
    I32Exit, ResourceTable,
    p2::{WasiCtxBuilder, bindings::Command},
};

async fn run(wasi_component_path: &Path) -> Result<()> {
    let engine =
        Engine::new(wasmtime::Config::new().async_support(true)).context("creating WASM engine")?;

    let component = Component::from_file(&engine, wasi_component_path)?;

    let mut linker = Linker::new(&engine);

    // Normally we would do
    //
    //   wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    //
    // But that adds the filesystem API too and we want to use our own one. So
    // instead we copy & paste it, removing the filesystem API ...
    wasi_linker_excluding_filesystem::add_to_linker_async(&mut linker)?;

    // ... and then add our custom one instead.
    wasi_state::add_to_linker_async(&mut linker)?;

    let wasi = WasiCtxBuilder::new()
        .allow_tcp(false)
        .allow_udp(false)
        .allow_ip_name_lookup(false)
        .inherit_stdout()
        .inherit_stderr()
        .build();

    let repo = gix::open(Path::new(".")).context("opening repo")?;
    let root = repo.head_tree_id().context("finding HEAD tree")?.detach();

    let state = WasiState {
        wasi_ctx: wasi,
        resource_table: ResourceTable::new(),
        repo,
        root,
    };

    let mut store = Store::new(&engine, state);

    let command = Command::instantiate_async(&mut store, &component, &linker).await?;

    let run_result = command.wasi_cli_run().call_run(&mut store).await;

    // The return type here is very weird. See
    // https://github.com/bytecodealliance/wasmtime/issues/10767
    match run_result {
        Ok(res) => res.map_err(|_| anyhow!("Unknown error running WASM component"))?,
        Err(error) => {
            if let Some(exit) = error.downcast_ref::<I32Exit>() {
                // Err(I32Exit(0)) is actually success.
                if exit.0 != 0 {
                    bail!("WASM failed with exit code {:?}", exit.0);
                }
            } else {
                bail!(error);
            }
        }
    };

    Ok(())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    run(Path::new("wasi_ls.wasm")).await
}
