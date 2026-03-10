//! Smart device registry and state management.

#![allow(unused)]

use std::{
    ptr,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use derive_more::{AsRef, From, TryInto};
use parking_lot::{Mutex, MutexGuard};
use roboscope_ipc::{
    DeviceReadings, DeviceSnapshot, PHYSICS_UPDATE_PERIOD, SMART_DEVICES_COUNT, Sample, SimServices, Subscriber
};
use tracing::{debug, trace};
use vex_sdk::{V5_DeviceT, V5_DeviceType};

use crate::sdk::vexSystemTimeGet;

pub fn start_device_handler(ipc: Arc<SimServices>) {
    thread::Builder::new()
        .name("Sim Device Handler".into())
        .spawn(move || {
            debug!("Connecting to physics provider");
            let dev_handler = DeviceHandler::new(ipc.clone()).expect("created device handler");

            while ipc.node.wait(PHYSICS_UPDATE_PERIOD).is_ok() {
                dev_handler.update().expect("device update OK");
            }
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

struct QueuedSample {
    inner: Sample<DeviceReadings>,
    timestamp: u32,
}

impl QueuedSample {
    pub fn snapshots(&self) -> &[DeviceSnapshot] {
        &self.inner.0
    }
}

pub static DEVICES: Devices = Devices::new();
const NUM_DEVICES_HANDLES: usize = 23;

pub struct Devices {
    queued_sample: Mutex<Option<QueuedSample>>,
    timestamp: AtomicU32,
    pub smart_devices: [V5Device; NUM_DEVICES_HANDLES],
}

impl Devices {
    pub const fn new() -> Self {
        Self {
            queued_sample: Mutex::new(None),
            timestamp: AtomicU32::new(0),
            smart_devices: [const { V5Device::new() }; _],
        }
    }

    pub fn queue_sample(&self, sample: Sample<DeviceReadings>) {
        trace!(?sample, "Queueing new device sample");
        *self.queued_sample.lock() = Some(QueuedSample {
            inner: sample,
            timestamp: vexSystemTimeGet(),
        });
    }

    /// Copy the latest device readings (if any are available) from shared memory.
    pub fn update_readings(&self) {
        trace!("Committing queued sample");

        if let Some(sample) = self.queued_sample.lock().take() {
            for (i, &snapshot) in sample.snapshots().iter().enumerate() {
                *self.smart_devices[i].0.lock() = V5DeviceData {
                    snapshot,
                    timestamp: sample.timestamp,
                };
            }
        }
    }

    pub fn handle_for(&self, port: u32) -> Option<V5_DeviceT> {
        let device = self.smart_devices.get(port as usize)?;
        Some(ptr::from_ref(device).cast_mut().cast())
    }

    pub fn get_by_handle(&self, handle: V5_DeviceT) -> Option<&V5Device> {
        let offset = handle
            .addr()
            .checked_sub(self.smart_devices.as_ptr().addr())?;

        if offset % size_of::<V5Device>() != 0 {
            return None;
        }

        self.smart_devices.get(offset / size_of::<V5Device>())
    }

    pub unsafe fn get_by_handle_unchecked(&self, handle: V5_DeviceT) -> &V5Device {
        // Better than dereferencing the pointer because you get precondition checks in debug mode.
        unsafe { self.get_by_handle(handle).unwrap_unchecked() }
    }
}

pub struct V5DeviceData {
    snapshot: DeviceSnapshot,
    timestamp: u32,
}

impl V5DeviceData {
    pub const fn new() -> Self {
        Self {
            snapshot: DeviceSnapshot::Empty,
            timestamp: 0,
        }
    }

    pub fn readings_mut<T>(&mut self) -> Option<&mut T>
    where
        for<'a> &'a mut T: TryFrom<&'a mut DeviceSnapshot>,
    {
        (&mut self.snapshot).try_into().ok()
    }

    pub fn readings<T>(&self) -> Option<&T>
    where
        for<'a> &'a T: TryFrom<&'a DeviceSnapshot>,
    {
        (&self.snapshot).try_into().ok()
    }

    pub fn timestamp(&self) -> u32 {
        self.timestamp
    }
}

#[derive(AsRef)]
pub struct V5Device(pub Mutex<V5DeviceData>);

impl V5Device {
    pub const fn new() -> Self {
        Self(Mutex::new(V5DeviceData::new()))
    }

    pub fn kind(&self) -> V5_DeviceType {
        match &self.as_ref().lock().snapshot {
            DeviceSnapshot::Empty => V5_DeviceType::kDeviceTypeNoSensor,
            DeviceSnapshot::Generic(_) => V5_DeviceType::kDeviceTypeGenericSensor,
            DeviceSnapshot::Distance(_) => V5_DeviceType::kDeviceTypeDistanceSensor,
        }
    }
}
