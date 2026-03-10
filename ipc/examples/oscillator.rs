use roboscope_ipc::{Config, DeviceReadings, DistanceSnapshot, SimServices};

fn main() {
    let sim = SimServices::join(Some("Oscillator example"), &Config::default()).unwrap();

    let mut readings = DeviceReadings::default();

    let mut x = -500.0;
    let mut vx = 0.0;

    sim.publish_device_readings(|_cmds| {
        vx -= x * 0.0001;
        x += vx;

        readings.0[0] = DistanceSnapshot {
            distance: (x + 500.0) as u32,
            ..Default::default()
        }
        .into();

        // println!("{readings:?}");

        readings
    })
    .unwrap();
}
