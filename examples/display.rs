use std::{
    f64::consts::{PI, TAU}, process::exit, thread, time::Duration
};

use embedded_graphics::{
    pixelcolor::{Rgb888, raw::RawU24},
    prelude::RawData,
};
use tinybmp::Bmp;
use tracing_subscriber::EnvFilter;
use vex_sdk_desktop::sdk::*;

fn main() {
    // e.g. RUST_LOG=debug,vex_sdk_desktop=trace
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

        const AIRPLANE: &[u8] = include_bytes!("airplane.bmp");
        let bmp = Bmp::<Rgb888>::from_slice(AIRPLANE).unwrap();
        let img_data = bmp
            .pixels()
            .map(|p| RawU24::from(p.1).into_inner())
            .collect::<Vec<u32>>();
        let width = bmp.as_raw().header().image_size.width;

        let offset = 200 * width + 325;

        unsafe {
            vexDisplayCopyRect(
                20,
                150,
                100,
                220,
                img_data.as_ptr().cast_mut().wrapping_add(offset as usize),
                width as i32,
            );
        }

        let string = c"Vex V5!";
        unsafe {
            vexDisplayPrintf(105, 180, 0, string.as_ptr());
        }

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

            let angle = (position / 50.0 + 1.0) * PI;
            let (y, x) = angle.sin_cos();
            // println!("({x}, {y})");

            let x1 = 250 + (x * 20.0) as i32;
            let x2 = 250 - (x * 20.0) as i32;

            let y1 = 100 + (y * 20.0) as i32;
            let y2 = 100 - (y * 20.0) as i32;

            vexDisplayForegroundColor(0xFF_FF_FF);
            vexDisplayCircleFill(250, 100, 20);
            vexDisplayForegroundColor(0x00_00_00);
            // println!("{:?}", (x1, y1, x2, y2));
            vexDisplayLineDraw(x1, y1, x2, y2);

            vexDisplayRender(true, false);
        }
    })
    .unwrap();
}
