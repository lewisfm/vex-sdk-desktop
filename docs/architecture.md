# Architecture

This document describes a goal for how different parts of the simulation will interoperate.

## Robot code

Robot code runs on its own thread and may use SDK functions to read views of simulated state and update it.

`vexTasksRun` can be called from user code to run the simulator's periodic state management callbacks (i.e. tasks).

Stack switching will not be supported.

## Display

The **display** is a image buffer that's updated at 60 Hz by the simulation's render thread. User code can edit the contents of the display by writing to the global canvas. Every time the render thread renders a frame (or whenever vexTasksRender is called, depending on display mode), the contents of the canvas are copied onto the display.

The top of the display has a program header that is drawn on a separate canvas owned by the render thread.

To commit a frame, the render thread either copies a scaled version of the display's image buffer into a window's framebuffer, publishes the image over IPC, or saves it to a file.

### Double buffer mode

If "double buffer" mode is enabled, the contents of the canvas are not automatically copied onto the display by the render thread before each frame. User code must instead explicitly call vexDisplayRender to copy the canvas to the display.

Contrary to what the name may imply, when "double buffer" mode is disabled the display still uses separate canvas and display buffers to prevent the displayed contents from updating at a rate faster than 60Hz.

## Smart Ports

Smart Ports are views from robot code into the state of a simulated device. Each Smart Port takes a snapshot of the output of the simulation which is updated periodically as `vexTasksRun` is called.

Smart Ports can also be used, in some cases, to change the simulation's inputs via functions like `vexDeviceMotorVoltageSet`.

### Device Snapshots

Each Smart Port keeps track of a device snapshot which holds some information about the last known state of a single device in the physics simulation. They are updated at 100Hz by the *device management task*.

```rs
enum DeviceSnapshot {
    Distance(DistanceSensorSnapshot),
    Motor(MotorSnapshot),
    // ...
}
```

If a Smart Port is queried about a different type of device than the one its current snapshot holds information about, the operation is a no-op and the function will return some default value.

### Device Control

Smart Ports also keep track of device control messages, which are messages sent to devices immediately after snapshots are updated. Each port has a single pending message at a time, and SDK functions such as `vexDeviceMotorVoltageSet` can be used to set the pending message.

## Observability

Simulation state is generally made available via Eclipse iceoryx2 for use by a visualizer application.

### Display Buffer

The display buffer is made available as a pub/sub service with a single frame of history.
