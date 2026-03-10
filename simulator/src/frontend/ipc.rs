use std::{mem::MaybeUninit, ptr, sync::Arc, thread, time::Duration};

use roboscope_ipc::{Config, DisplayFrame, SimServices};
use tracing::trace;

use crate::{
    device::start_device_handler,
    display::{DISPLAY, FRAME_FINISHED},
};

pub fn start(name: &str, entrypoint: impl FnOnce() + Send + 'static) -> anyhow::Result<()> {
    DISPLAY.lock().set_program_name(name);

    let ipc = Arc::new(SimServices::join(
        Some("vex-sdk-desktop"),
        &Config::default(),
    )?);

    start_renderer(ipc.clone());
    start_device_handler(ipc.clone());
    let user_code = thread::spawn(entrypoint);

    while ipc.node.wait(Duration::from_millis(10)).is_ok() {
        if user_code.is_finished() {
            break;
        }
    }

    Ok(())
}

fn start_renderer(ipc: Arc<SimServices>) {
    thread::Builder::new()
        .name("Sim Display Render".into())
        .spawn(move || {
            // SAFETY: render_frame initializes the frame
            unsafe {
                ipc.publish_display(publish_frame).unwrap();
            }
        })
        .unwrap();
}

/// Renders a frame by copying the current display data into the given buffer, initializing it.
fn publish_frame(frame: &mut MaybeUninit<DisplayFrame>) {
    let mut disp = DISPLAY.lock();
    disp.render();

    trace!("Publishing a frame");

    let frame_ptr = frame.as_mut_ptr();
    unsafe {
        let source = &raw const disp.buffer;
        let destination = &raw mut (*frame_ptr).buffer;
        source.copy_to(destination, 1);
    }

    FRAME_FINISHED.notify_all();
}
