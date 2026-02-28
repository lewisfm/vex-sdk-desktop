use std::{thread, time::Duration};

use tracing_subscriber::EnvFilter;
use vex_sdk_desktop::sdk::*;

fn main() {
    // e.g. RUST_LOG=debug,vex_sdk_sim=trace
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    vex_sdk_desktop::run_simulator(|| {
        println!("Hello, world");

        vexDisplayForegroundColor(0xFF_FF_FF);
        vexDisplayRectFill(0, -10, 50, 50);
        vexDisplayForegroundColor(0xFF_00_FF);
        vexDisplayRectDraw(30, 30, 70, 70);

        vexDisplayForegroundColor(0xFF_FF_FF);

        vexDisplayCircleFill(25, 100, 0);
        vexDisplayCircleDraw(30, 100, 0);

        vexDisplayCircleFill(50, 100, 1);
        vexDisplayCircleDraw(70, 100, 1);

        vexDisplayCircleDraw(100, 100, 2);
        vexDisplayCircleFill(120, 100, 2);

        vexDisplayCircleDraw(150, 100, 10);
        vexDisplayCircleFill(180, 100, 10);

        let mut velocity = 0.0;
        let mut position = -50.0;
        loop {
            velocity -= position * 0.01;
            position += velocity;

            vexDisplayForegroundColor(0x00_00_00);
            vexDisplayRectFill(200, 0, 220, 200);

            let y = position as i32 + 100;
            vexDisplayForegroundColor(0x00_FF_00);
            vexDisplayCircleFill(210, y, 10);

            vexDisplayRender(true, false);
        }
    })
    .unwrap();
}
