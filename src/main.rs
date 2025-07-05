use std::path::Path;

use anyhow::{Context as _, Result, anyhow, bail};
use wasmtime::{
    Engine, Store,
    component::{Component, Linker},
};
use wasmtime_wasi::{
    DirPerms, FilePerms, I32Exit, ResourceTable,
    p2::{WasiCtx, WasiCtxBuilder, WasiView, bindings::Command},
};
use wasmtime_wasi_io::IoView;

struct ComponentRunStates {
    wasi_ctx: WasiCtx,
    resource_table: ResourceTable,
}

impl WasiView for ComponentRunStates {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi_ctx
    }
}

impl IoView for ComponentRunStates {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.resource_table
    }
}

async fn run(wasi_component_path: &Path) -> Result<()> {
    let engine =
        Engine::new(wasmtime::Config::new().async_support(true)).context("creating WASM engine")?;

    let component = Component::from_file(&engine, wasi_component_path)?;

    let mut linker = Linker::new(&engine);

    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;

    let wasi = WasiCtxBuilder::new()
        .allow_tcp(false)
        .allow_udp(false)
        .allow_ip_name_lookup(false)
        .preopened_dir(".", ".", DirPerms::all(), FilePerms::all())?
        .inherit_stdout()
        .build();

    let state = ComponentRunStates {
        wasi_ctx: wasi,
        resource_table: ResourceTable::new(),
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
