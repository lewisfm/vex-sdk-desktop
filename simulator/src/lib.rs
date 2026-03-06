#![feature(c_variadic)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::path::Path;

mod canvas;
mod config;
mod display;
pub mod sdk;
mod device;
mod frontend;
pub mod error;


pub fn run_simulator(entrypoint: impl FnOnce() + Send + 'static) -> anyhow::Result<()> {
    let mut args = std::env::args();
    let path = args.next().unwrap_or_else(|| "Simulator".to_string());

    let exe_name = Path::new(&path)
        .file_name()
        .and_then(|str| str.to_str())
        .unwrap_or(&path);

    // frontend::windowed::SimulatorApp::start(exe_name.to_string(), entrypoint)?;
    frontend::ipc::start(exe_name)?;

    Ok(())
}
