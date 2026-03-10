use std::{
    f64::consts::{PI, TAU},
    mem::MaybeUninit,
    process::exit,
    thread::{self, sleep},
    time::Duration,
};

use embedded_graphics::{
    pixelcolor::{Rgb888, raw::RawU24},
    prelude::RawData,
};
use tinybmp::Bmp;
use tracing_subscriber::EnvFilter;
use vex_sdk::*;
use vexide::prelude::Peripherals;

mod common;

common::create_main!(entry);

// Note that you need to connect a physics provider to run this example, if you just want something
// simple then `cargo run -p roboscope-ipc --example oscillator`.
async fn entry(_p: Peripherals) {
    unsafe {
        let sensor = vexDeviceGetByIndex(0);

        loop {
            let distance = vexDeviceDistanceDistanceGet(sensor);
            println!("distance: {distance}");

            vexTasksRun();

            // Intentionally make it slow so you can see the events better.
            sleep(Duration::from_millis(100));
        }
    }
}
