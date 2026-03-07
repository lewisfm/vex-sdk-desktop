//! Smart device registry and state management.

#![allow(unused)]

use std::{sync::Arc, thread};

use derive_more::{From, TryInto};
use parking_lot::Mutex;
use roboscope_ipc::{DeviceReadings, DeviceSnapshot, SMART_DEVICES_COUNT, Sample, SimServices, Subscriber};

static DEVICES: Devices = Devices::new();

pub fn start_device_handler(ipc: Arc<SimServices>) {
    thread::Builder::new()
        .name("Sim Device Handler".into())
        .spawn(move || {
            let dev_handler = DeviceHandler::new(ipc).expect("created device handler");
        })
        .unwrap();
}

struct DeviceHandler {
    readings: Subscriber<DeviceReadings>,
}

impl DeviceHandler {
    pub fn new(ipc: Arc<SimServices>) -> anyhow::Result<Self> {
        let captures = ipc.device_readings()?.subscriber_builder().create()?;

        Ok(Self { readings: captures })
    }

    pub fn update(&self) -> anyhow::Result<()> {
        if let Some(sample) = self.readings.receive()? {
            DEVICES.queue_sample(sample);
        }

        Ok(())
    }
}

struct Devices {
    queued_sample: Mutex<Option<Sample<DeviceReadings>>>,
    readings: Mutex<DeviceReadings>,
}

impl Devices {
    pub const fn new() -> Self {
        Self {
            queued_sample: Mutex::new(None),
            readings: Mutex::new(DeviceReadings([DeviceSnapshot::Empty; _])),
        }
    }

    pub fn queue_sample(&self, sample: Sample<DeviceReadings>) {
        *self.queued_sample.lock() = Some(sample);
    }

    /// Copy the latest device readings (if any are available) from shared memory.
    pub fn update_readings(&self) {
        if let Some(sample) = self.queued_sample.lock().take() {
            *self.readings.lock() = *sample;
        }
    }

    pub fn readings_for<T>(&self, port: usize, cb: impl FnOnce(Option<&mut T>))
    where
        for<'a> &'a mut T: TryFrom<&'a mut DeviceSnapshot>,
    {
        let mut readings = self.readings.lock();
        let port = &mut readings.0[port];
        cb(port.try_into().ok());
    }
}
