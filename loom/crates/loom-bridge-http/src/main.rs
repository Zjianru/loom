use anyhow::{Context, Result, bail};
use loom_bridge_http::build_router;
use loom_harness::LoomHarness;
use loom_store::LoomStore;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    let command = env::args().nth(1).unwrap_or_else(|| "serve".to_string());
    if command != "serve" {
        bail!("unsupported command: {command}");
    }

    let mut runtime_root_arg = PathBuf::from("runtime");
    let mut bind_addr = "127.0.0.1:6417".to_string();
    let mut args = env::args().skip(2);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--runtime-root" => {
                let Some(value) = args.next() else {
                    bail!("--runtime-root requires a value");
                };
                runtime_root_arg = PathBuf::from(value);
            }
            "--bind" => {
                let Some(value) = args.next() else {
                    bail!("--bind requires a value");
                };
                bind_addr = value;
            }
            other => bail!("unsupported argument: {other}"),
        }
    }

    let loom_runtime_root = runtime_root_arg.join("loom");
    std::fs::create_dir_all(&loom_runtime_root)
        .with_context(|| format!("creating runtime root {}", loom_runtime_root.display()))?;
    let database_path = loom_runtime_root.join("loom.sqlite3");
    let store = LoomStore::open(&database_path, &loom_runtime_root)?;
    let harness = LoomHarness::new(store);
    let app = build_router(harness);
    let socket_addr: SocketAddr = bind_addr.parse().context("parsing bind address")?;
    let listener = TcpListener::bind(socket_addr)
        .await
        .with_context(|| format!("binding {socket_addr}"))?;
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .await
        .context("serving LocalHttpBridge")?;
    Ok(())
}
