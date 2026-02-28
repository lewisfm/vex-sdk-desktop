use std::{thread, time::Duration};

use tracing_subscriber::EnvFilter;
use vex_sdk_sim::sdk::*;

fn main() {
    // e.g. RUST_LOG=debug,vex_sdk_sim=trace
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    vex_sdk_sim::run_simulator(|| {
        println!("Hello, world");
        vexDisplayForegroundColor(0xFF_FF_FF);
        vexDisplayRectFill(0, -10, 50, 50);


        vexDisplayCircleFill(25, 100, 0);
        vexDisplayCircleFill(50, 100, 1);
        vexDisplayCircleFill(150, 100, 2);

        loop {
            thread::sleep(Duration::from_millis(16));
        }
    })
    .unwrap();
}
