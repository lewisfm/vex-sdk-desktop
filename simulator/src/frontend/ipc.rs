use std::{mem::MaybeUninit, ptr};

use roboscope_ipc::{DisplayFrame, SimServices};
use tracing::trace;

use crate::display::{DISPLAY, FRAME_FINISHED};

pub fn start(name: &str) -> anyhow::Result<()> {
    DISPLAY.lock().set_program_name(name);
    let ipc = SimServices::join(Some("vex-sdk-desktop"))?;

    // SAFETY: render_frame initializes the frame
    unsafe {
        ipc.publish_display(render_frame)?;
    }

    Ok(())
}

/// Renders a frame by copying the current display data into the given buffer, initializing it.
fn render_frame(frame: &mut MaybeUninit<DisplayFrame>) {
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
