//! Brain Display

use core::ffi::{VaList, c_char};
use tracing::trace;

pub use vex_sdk::v5_image;

use crate::{
    SIM_APP, SimEvent,
    canvas::{CANVAS, HEADER_HEIGHT, Point, Rect},
    display::{DISPLAY, SimDisplay},
};

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayForegroundColor(col: u32) {
    CANVAS.lock().state.fg_color = col;
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayBackgroundColor(col: u32) {
    CANVAS.lock().state.bg_color = col;
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayErase() {}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayScroll(nStartLine: i32, nLines: i32) {}
#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayScrollRect(x1: i32, y1: i32, x2: i32, y2: i32, nLines: i32) {}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayCopyRect(
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    pSrc: *mut u32,
    srcStride: i32,
) {
    unsafe {
        CANVAS.lock().copy_rect(
            Rect::from_sdk(x1, y1, x2, y2),
            pSrc,
            srcStride.max(0) as usize,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayPixelSet(x: u32, y: u32) {
    let mut canvas = CANVAS.lock();
    canvas.set_pixel(Point {
        x: x as i32,
        y: y as i32 + HEADER_HEIGHT,
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayPixelClear(x: u32, y: u32) {
    let mut canvas = CANVAS.lock();
    canvas.state.swap_colors();
    canvas.set_pixel(Point {
        x: x as i32,
        y: y as i32 + HEADER_HEIGHT,
    });
    canvas.state.swap_colors();
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayLineDraw(x1: i32, y1: i32, x2: i32, y2: i32) {
    let mut canvas = CANVAS.lock();
    canvas.draw_line(
        Point {
            x: x1,
            y: y1 + HEADER_HEIGHT,
        },
        Point {
            x: x2,
            y: y2 + HEADER_HEIGHT,
        },
    );
}
#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayLineClear(x1: i32, y1: i32, x2: i32, y2: i32) {
    let mut canvas = CANVAS.lock();
    canvas.state.swap_colors();
    canvas.draw_line(
        Point {
            x: x1,
            y: y1 + HEADER_HEIGHT,
        },
        Point {
            x: x2,
            y: y2 + HEADER_HEIGHT,
        },
    );
    canvas.state.swap_colors();
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayRectDraw(x1: i32, y1: i32, x2: i32, y2: i32) {
    let mut canvas = CANVAS.lock();
    canvas.trace_rect(Rect::from_sdk(x1, y1, x2, y2));
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayRectClear(x1: i32, y1: i32, x2: i32, y2: i32) {
    let mut canvas = CANVAS.lock();
    canvas.state.swap_colors();
    canvas.fill_rect(Rect::from_sdk(x1, y1, x2, y2));
    canvas.state.swap_colors();
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayRectFill(x1: i32, y1: i32, x2: i32, y2: i32) {
    let mut canvas = CANVAS.lock();
    canvas.fill_rect(Rect::from_sdk(x1, y1, x2, y2));
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayCircleDraw(xc: i32, yc: i32, radius: i32) {
    let mut canvas = CANVAS.lock();

    let point = Point {
        x: xc,
        y: yc + HEADER_HEIGHT,
    };
    canvas.trace_circle(point, radius.max(0) as u32);
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayCircleClear(xc: i32, yc: i32, radius: i32) {
    let mut canvas = CANVAS.lock();
    canvas.state.swap_colors();

    let point = Point {
        x: xc,
        y: yc + HEADER_HEIGHT,
    };
    canvas.fill_circle(point, radius.max(0) as u32);

    canvas.state.swap_colors();
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayCircleFill(xc: i32, yc: i32, radius: i32) {
    let mut canvas = CANVAS.lock();

    let point = Point {
        x: xc,
        y: yc + HEADER_HEIGHT,
    };
    canvas.fill_circle(point, radius.max(0) as u32);
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayTextSize(n: u32, d: u32) {}
#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayFontNamedSet(pFontName: *const c_char) {}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayForegroundColorGet() -> u32 {
    CANVAS.lock().state.fg_color
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayBackgroundColorGet() -> u32 {
    CANVAS.lock().state.bg_color
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayStringWidthGet(pString: *const c_char) -> i32 {
    Default::default()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayStringHeightGet(pString: *const c_char) -> i32 {
    Default::default()
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayPenSizeSet(width: u32) {
    CANVAS.lock().state.pen_size = width;
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayPenSizeGet() -> u32 {
    CANVAS.lock().state.pen_size
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayClipRegionSet(x1: i32, y1: i32, x2: i32, y2: i32) {
    let region = Rect::new(x1, y1, x2 + 1, y2 + 1);
    CANVAS.lock().state.set_clip_region(region);
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayRender(bVsyncWait: bool, bRunScheduler: bool) {
    trace!("Dispatching render");

    let app = SIM_APP
        .get()
        .expect("Attempted to dispatch render without an active render thread");

    let do_render = |display: &mut SimDisplay| {
        display.set_autorender(false);
        display.render_user_canvas(&CANVAS.lock());
        // We do not send an event to the renderer telling it to render because that could
        // potentially cause render speeds of more than 60fps which is not true to the V5 hardware.
    };

    if bVsyncWait {
        SimDisplay::run_synced(do_render);
    } else {
        do_render(&mut DISPLAY.lock());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayDoubleBufferDisable() {
    let mut display = DISPLAY.lock();
    display.set_autorender(true);
}

#[unsafe(no_mangle)]
pub extern "C" fn vexDisplayClipRegionSetWithIndex(index: i32, x1: i32, y1: i32, x2: i32, y2: i32) {
    unimplemented!("VEXos task api")
}

#[unsafe(no_mangle)]
pub extern "C" fn vexImageBmpRead(
    ibuf: *const u8,
    oBuf: *mut v5_image,
    maxw: u32,
    maxh: u32,
) -> u32 {
    Default::default()
}
#[unsafe(no_mangle)]
pub extern "C" fn vexImagePngRead(
    ibuf: *const u8,
    oBuf: *mut v5_image,
    maxw: u32,
    maxh: u32,
    ibuflen: u32,
) -> u32 {
    Default::default()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayVPrintf(
    xpos: i32,
    ypos: i32,
    bOpaque: i32,
    format: *const c_char,
    mut args: VaList<'_>,
) {
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayVString(
    nLineNumber: i32,
    format: *const c_char,
    args: VaList<'_>,
) {
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayVStringAt(
    xpos: i32,
    ypos: i32,
    format: *const c_char,
    args: VaList<'_>,
) {
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayVBigString(
    nLineNumber: i32,
    format: *const c_char,
    args: VaList<'_>,
) {
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayVBigStringAt(
    xpos: i32,
    ypos: i32,
    format: *const c_char,
    args: VaList<'_>,
) {
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayVSmallStringAt(
    xpos: i32,
    ypos: i32,
    format: *const c_char,
    args: VaList<'_>,
) {
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayVCenteredString(
    nLineNumber: i32,
    format: *const c_char,
    args: VaList<'_>,
) {
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayVBigCenteredString(
    nLineNumber: i32,
    format: *const c_char,
    args: VaList<'_>,
) {
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayPrintf(
    xpos: i32,
    ypos: i32,
    bOpaque: i32,
    format: *const c_char,
    args: ...
) {
    unsafe { vexDisplayVPrintf(xpos, ypos, bOpaque, format, args) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayString(nLineNumber: i32, format: *const c_char, args: ...) {
    unsafe { vexDisplayVString(nLineNumber, format, args) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayStringAt(
    xpos: i32,
    ypos: i32,
    format: *const c_char,
    args: ...
) {
    unsafe { vexDisplayVStringAt(xpos, ypos, format, args) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayBigString(nLineNumber: i32, format: *const c_char, args: ...) {
    unsafe { vexDisplayVBigString(nLineNumber, format, args) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayBigStringAt(
    xpos: i32,
    ypos: i32,
    format: *const c_char,
    args: ...
) {
    unsafe { vexDisplayVBigStringAt(xpos, ypos, format, args) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplaySmallStringAt(
    xpos: i32,
    ypos: i32,
    format: *const c_char,
    args: ...
) {
    unsafe { vexDisplayVSmallStringAt(xpos, ypos, format, args) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayCenteredString(
    nLineNumber: i32,
    format: *const c_char,
    args: ...
) {
    unsafe { vexDisplayVCenteredString(nLineNumber, format, args) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vexDisplayBigCenteredString(
    nLineNumber: i32,
    format: *const c_char,
    args: ...
) {
    unsafe { vexDisplayVBigCenteredString(nLineNumber, format, args) }
}
