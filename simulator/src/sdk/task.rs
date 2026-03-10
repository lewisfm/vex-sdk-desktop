//! VEXos Task Scheduler Functions

use core::ffi::{c_char, c_int, c_void};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use vex_sdk::V5_TouchEvent;

use crate::{canvas::HEADER_HEIGHT, device::DEVICES, display::DISPLAY};

#[unsafe(no_mangle)]
pub extern "system" fn vexTaskAdd(
    callback: unsafe extern "C" fn() -> c_int,
    interval: c_int,
    label: *const c_char,
) {
    super::sdk_unimplemented!("vexTaskAdd");
}

#[unsafe(no_mangle)]
pub extern "system" fn vexTaskGetCallbackAndId(index: u32, callback_id: *mut c_int) -> *mut c_void {
    super::sdk_unimplemented!("vexTaskGetCallbackAndId");
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "system" fn vexTaskSleep(time: u32) {
    super::sdk_unimplemented!("vexTaskSleep");
}

#[unsafe(no_mangle)]
pub extern "system" fn vexTaskHardwareConcurrency() -> i32 {
    super::sdk_unimplemented!("vexTaskHardwareConcurrency");
    0
}

#[unsafe(no_mangle)]
pub extern "system" fn vexBackgroundProcessing() {
    super::sdk_unimplemented!("vexBackgroundProcessing");
}

struct Task {
    func: fn(),
    interval: Duration,
    last_run: Option<Instant>,
}

impl Task {
    const fn new(func: fn(), interval: Duration) -> Self {
        Self {
            func,
            interval,
            last_run: None,
        }
    }

    fn poll(&mut self, now: Instant) {
        if let Some(last_run) = self.last_run {
            if (last_run + self.interval) < now {
                self.last_run = Some(now);
                (self.func)();
            }
        } else {
            self.last_run = Some(now);
            (self.func)();
        }
    }
}

static TASKS: Mutex<[Task; 1]> = Mutex::new([
    // Should this be a task? I'm not sure if touch data updates automatically.
    // Task::new(update_touch_status, Duration::from_millis(10)),
    Task::new(update_device_readings, Duration::from_millis(10)),
]);

pub fn update_touch_status() {
    let mut display = DISPLAY.lock();

    if display.mouse_down {
        if display.touch.lastEvent == V5_TouchEvent::kTouchEventRelease {
            display.touch.lastEvent = V5_TouchEvent::kTouchEventPress;
            display.touch.pressCount += 1;
        } else {
            display.touch.lastEvent = V5_TouchEvent::kTouchEventPressAuto;
        }

        display.touch.lastXpos = display.mouse_coords.x as i16;
        display.touch.lastYpos = (display.mouse_coords.y - HEADER_HEIGHT) as i16;
    } else {
        if display.touch.lastEvent != V5_TouchEvent::kTouchEventRelease {
            display.touch.releaseCount += 1;
        }

        display.touch.lastEvent = V5_TouchEvent::kTouchEventRelease;
    }
}

pub fn update_device_readings() {
    DEVICES.update_readings();
}

#[unsafe(no_mangle)]
pub extern "system" fn vexTasksRun() {
    let mut tasks = TASKS.lock();
    let now = Instant::now();

    for task in tasks.as_mut() {
        task.poll(now);
    }
}
