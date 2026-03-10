# RoboScope - VEX V5 Simulation

This repository contains an ecosystem of projects used for VEX V5 simulation, known as RoboScope collectively (this is a working name). The end-goal for this project is to provide a way for users to design their robot code and train their drivers even when access to a physical robot is limited. This project is developed using only publicly-available or measurable information about the behavior of VEX products.

Right now this has an almost pixel-accurate V5 display simulator. It's difficult to get the text
drawing to have the right appearance of "boldness" but the size and placement of drawn text should
match a V5 brain very well.

![Display simulator](./assets/display.gif)

## Projects

- [Brain Simulator] aka `vex-sdk-desktop`: Drop-in replacement SDK library which provides desktop
    implementations of functions normally only available via VEXos V5. This lets you run unmodified
    robot code like an auton selector or drive program without a V5 brain. Simulation events and
    status updates are published via shared memory for other processes to use.
- [Display Viewer]: A fairly simple app that connects to an active brain simulator and shows the
    current image on the brain's display.
- [IPC library] aka `roboscope-ipc`: Packet definition library for publishing and subscribing to
    simulator data. This could be used to implement a custom simulator visualizer or physics engine.

There are also some example projects under `examples/` which you can try simulating (the GIF above
is the `display` example).

[Brain Simulator]: ./simulator
[Display Viewer]: ./viewer
[IPC Library]: ./ipc

## Planned

I'm working on wiring up the device APIs to the IPC library so you can send simulated sensor
readings to your brain simulator and control simulated motors.

If this project pans out I'm also planning to get this integrated into vexide to make it more plug
and play.

## Usage

Run an example V5 program:

```sh
cargo run --example display
```

At the same time, you can run one of these programs to dynamically add features to the simulation:

View the display:

```sh
cargo run -p roboscope-viewer -r
```

Start a physics server for a V5 program to use (this minimal example will connect a distance sensor
on port 1 and oscillate it back and forth):

```sh
cargo run -p roboscope-ipc --example oscillator
```

### Builtin Display mode

As a convenience, you can completely disable IPC support and instead open the display window directly from your V5 simulator process.

```sh
cargo run --example display -F vex-sdk-desktop/windowed
```
