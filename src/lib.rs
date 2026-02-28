#![feature(c_variadic)]
#![deny(unsafe_op_in_unsafe_fn)]

mod canvas;
mod display;
pub mod sdk;

#[cfg(target_os = "macos")]
mod macos;

use std::{
    io::Cursor,
    num::NonZeroU32,
    rc::Rc,
    sync::{OnceLock, atomic::Ordering},
    thread::{self},
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, anyhow};
use image::{ImageFormat, ImageReader, codecs::png::PngDecoder};
use softbuffer::{Context, Surface};
use tracing::{debug, error, trace};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy, OwnedDisplayHandle},
    window::{Theme, Window, WindowId},
};

use crate::{
    canvas::{AUTORENDER, CANVAS, Canvas, HEADER_COLOR, HEADER_HEIGHT, Rect, WIDTH},
    display::SimDisplay,
};

type DisplayCtx = Context<OwnedDisplayHandle>;

enum SimEvent {
    Render,
    SetAutoRender(bool),
}

static SIM_APP: OnceLock<EventLoopProxy<SimEvent>> = OnceLock::new();

pub fn run_simulator(run_app: impl FnOnce() + Send + 'static) -> Result<()> {
    let event_loop = EventLoop::with_user_event().build().unwrap();
    SIM_APP
        .set(event_loop.create_proxy())
        .map_err(|_| anyhow!("The simulator has already been initialized."))?;

    let context = DisplayCtx::new(event_loop.owned_display_handle())
        .map_err(|e| anyhow!(e.to_string()))
        .context("Failed to create display rendering context")?;

    let mut simulator = Simulator::new(context, run_app);
    event_loop.run_app(&mut simulator)?;

    Ok(())
}

struct Simulator<E> {
    sim_display: Option<SimDisplay>,
    context: DisplayCtx,
    entrypoint: Option<E>,
    last_frame_time: Option<Instant>,
}

impl<E> Simulator<E> {
    fn new(context: DisplayCtx, run_app: E) -> Self {
        Self {
            sim_display: None,
            context,
            entrypoint: Some(run_app),
            last_frame_time: None,
        }
    }

    fn schedule_render(&mut self, event_loop: &ActiveEventLoop, last_render: Instant) {
        let frame_period = Duration::from_secs(1) / 60;
        let now = Instant::now();

        let mut next_render = last_render + frame_period;
        if next_render < now {
            next_render = now + frame_period;
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(next_render));
    }
}

impl<E: FnOnce() + Send + 'static> ApplicationHandler<SimEvent> for Simulator<E> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.sim_display.is_none() {
            match SimDisplay::open(event_loop, &self.context) {
                Ok(sim_display) => self.sim_display = Some(sim_display),
                Err(error) => error!(%error, "Failed to open VEX V5 Display window"),
            }
        }

        if let Some(run_app) = self.entrypoint.take() {
            // thread::spawn(header_task);
            thread::spawn(run_app);
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        match cause {
            StartCause::Init => {
                // Start a timer for rendering the display at 60 fps.
                self.schedule_render(event_loop, Instant::now());
            }
            StartCause::ResumeTimeReached { requested_resume, .. } => {
                // 60Hz render timer has triggered, so render a frame.
                self.schedule_render(event_loop, requested_resume);

                let now = Instant::now();
                if let Some(last) = self.last_frame_time.replace(now) {
                    trace!(measured_period = ?now - last, "Frame time");
                }

                if let Some(d) = &self.sim_display {
                    d.queue_redraw();
                }
            }
            _ => {}
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Some(sim_display) = &mut self.sim_display
            && window_id == sim_display.window_id()
        {
            sim_display.handle_event(event_loop, event);
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: SimEvent) {
        match event {
            SimEvent::Render => {
                if let Some(d) = &self.sim_display {
                    d.queue_redraw();
                }
            }
            SimEvent::SetAutoRender(autorender) => {
                if let Some(d) = &mut self.sim_display {
                    d.set_autorender(autorender);
                }
            }
        }
    }
}
