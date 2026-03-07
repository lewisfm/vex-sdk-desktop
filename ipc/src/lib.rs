//! Data transfer layer for Roboscope

#![allow(unused)]

use std::fmt::Debug;
use std::mem::MaybeUninit;
use std::sync::LazyLock;
use std::time::Duration;

use derive_more::{From, TryInto};
use iceoryx2::{port::publisher::Publisher, prelude::*};

use crate::error::{RoboscopeIpcError, SimResult};

#[cfg(feature = "thread-safe")]
use ipc_threadsafe as ipc;

pub type PubSubFactory<T> =
    iceoryx2::service::port_factory::publish_subscribe::PortFactory<ipc::Service, T, ()>;
pub type Subscriber<T> =
    iceoryx2::port::subscriber::Subscriber<ipc::Service, T, ()>;
pub type Sample<T> =
    iceoryx2::sample::Sample<ipc::Service, T, ()>;

pub mod error;

pub const PHYSICS_UPDATE_PERIOD: Duration = Duration::from_millis(10);
pub const SMART_DEVICES_COUNT: usize = 21;

pub static DISPLAY_UPDATE_PERIOD: LazyLock<Duration> =
    LazyLock::new(|| Duration::from_secs_f64(1.0 / 60.0));
pub const DISPLAY_WIDTH: u32 = 480;
pub const DISPLAY_HEIGHT: u32 = 272;
pub const DISPLAY_BUF_SIZE: usize = DISPLAY_WIDTH as usize * DISPLAY_HEIGHT as usize;

#[derive(Debug, Copy, Clone, PartialEq, ZeroCopySend, Default)]
#[repr(C)]
pub struct DeviceReadings(pub [DeviceSnapshot; SMART_DEVICES_COUNT]);

#[derive(Debug, Copy, Clone, PartialEq, ZeroCopySend, Default, From, TryInto)]
#[try_into(owned, ref, ref_mut)]
#[repr(C)]
pub enum DeviceSnapshot {
    #[default]
    Empty,
    Distance(DistanceSnapshot),
}

#[derive(Debug, Copy, Clone, PartialEq, ZeroCopySend, Default)]
#[repr(C)]
pub struct DistanceSnapshot {
    pub distance: u32,
    pub confidence: u32,
    pub status: u32,
    pub object_size: i32,
    pub object_velocity: i32,
}

#[derive(Debug, Copy, Clone, PartialEq, ZeroCopySend, Default)]
#[repr(C)]
pub struct RobotOutputs(pub [DeviceCommand; SMART_DEVICES_COUNT]);

#[derive(Debug, Copy, Clone, PartialEq, ZeroCopySend, Default, From, TryInto)]
#[repr(C)]
pub enum DeviceCommand {
    #[default]
    Empty,
}

#[derive(derive_more::Debug, ZeroCopySend)]
#[debug("DisplayFrame")]
#[repr(C)]
pub struct DisplayFrame {
    pub buffer: [u32; DISPLAY_BUF_SIZE],
}

#[derive(Debug)]
pub struct SimServices {
    pub node: Node<ipc::Service>,
}

impl SimServices {
    pub fn join(name: Option<&str>) -> SimResult<Self> {
        let mut node = NodeBuilder::new().config(&Config::default());

        if let Some(name) = name {
            let fmted_name = format!("roboscope.{name}");
            node = node.name(&NodeName::new(&fmted_name).expect("name valid"));
        }

        Ok(Self {
            node: node.create()?,
        })
    }

    fn pub_sub<T: Debug + ZeroCopySend>(
        &self,
        name: &str,
    ) -> SimResult<PubSubFactory<T>> {
        let name = ServiceName::new(name).unwrap();
        let service = self
            .node
            .service_builder(&name)
            .publish_subscribe::<T>()
            .open_or_create()?;

        Ok(service)
    }

    pub fn display_frames(&self) -> SimResult<PubSubFactory<DisplayFrame>> {
        self.pub_sub("vexide/roboscope/display_frames")
    }

    pub fn device_cmds(&self) -> SimResult<PubSubFactory<RobotOutputs>> {
        self.pub_sub("vexide/roboscope/device_cmds")
    }

    pub fn device_readings(&self) -> SimResult<PubSubFactory<DeviceReadings>> {
        self.pub_sub("vexide/roboscope/device_readings")
    }

    pub fn publish_device_readings(
        &self,
        mut physics_sim: impl FnMut(Option<&RobotOutputs>) -> DeviceReadings,
    ) -> SimResult<()> {
        let robot_subscriber = self.device_cmds()?.subscriber_builder().create()?;
        let captures = self.device_readings()?.publisher_builder().create()?;

        while self.node.wait(PHYSICS_UPDATE_PERIOD).is_ok() {
            let robot_outputs = robot_subscriber.receive()?;
            let physics_inputs = robot_outputs.as_ref().map(Sample::payload);

            let physics_outputs = captures
                .loan_uninit()?
                .write_payload(physics_sim(physics_inputs));

            physics_outputs.send()?;
        }

        Ok(())
    }

    /// Publish a stream of display frames to the simulator at 60Hz.
    ///
    /// # Safety
    ///
    /// The renderer callback is responsible for initializing the frame passed as its argument.
    pub unsafe fn publish_display(
        &self,
        mut renderer: impl FnMut(&mut MaybeUninit<DisplayFrame>),
    ) -> SimResult<()> {
        let frames = self.display_frames()?.publisher_builder().create()?;

        while self.node.wait(*DISPLAY_UPDATE_PERIOD).is_ok() {
            let mut next_frame = frames.loan_uninit()?;

            renderer(next_frame.payload_mut());

            // SAFETY: init'd by renderer
            let sample = unsafe { next_frame.assume_init() };
            sample.send()?;
        }

        Ok(())
    }

    pub fn stream_display(&self, mut cb: impl FnMut(&DisplayFrame)) -> SimResult<()> {
        let frames = self.display_frames()?.subscriber_builder().create()?;

        while self.node.wait(*DISPLAY_UPDATE_PERIOD).is_ok() {
            if let Some(next_frame) = frames.receive()? {
                cb(&next_frame);
            }
        }

        Ok(())
    }
}
