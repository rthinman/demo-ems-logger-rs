# Toolchain

## Summary

If you are familiar with embedded Rust development, most of this will be straightforward.  The summary is:

- I have been developing on a Windows machine with VSCode and its PowerShell terminal.
- This project uses the [Embassy Embedded Async framework](https://embassy.dev/) to manage asynchronous tasks.
- It uses [probe-rs](https://probe.rs/) as a debug probe adapter.
- I have been using an ST-Link V3SET + galvanic isolation daughterboard as my main debug probe.  I'm sure others will work fine, and the isolation is only because I needed it for another project.
- The project uses [flip-link](https://github.com/knurling-rs/flip-link) for stack overflow safety.
- I have been using Claude Code to help write tests.  So far, I haven't figured out how to get it to read my mind to build the architecture I want.

## Installation details

Install Rust: [Download the rustup installer](https://www.rust-lang.org/learn/get-started) and run it.

The default installation is for your target (Windows, MacOS, Linux). Add the embedded target, so Rust can compile for the microcontroller, too.  From a command prompt: > `rustup target add thumbv7em-none-eabi`.

[Probe-rs](https://probe.rs/) connects to the debug probe. For Windows, they recommend using their install script with PowerShell. By default, your execution policy is probably set to not allow scripts to run, so you'll have to do this:

- Open a new PowerShell terminal.
- Execute > `Set-ExecutionPolicy -ExecutionPolicy Unrestricted -Scope Process` to allow scripts to run in this process only.
- Execute > `irm https://github.com/probe-rs/probe-rs/releases/latest/download/probe-rs-tools-installer.ps1 | iex`
- This installs probe-rs 0.xx.y. to C:\Users\<username>\.cargo\bin.
- Close the terminal.

Install drivers for your debug probe hardware.  In the case of STLink, I found it convenient to install the STM32CubeProgrammer (just the programmer application, not the full Cube middleware and management application).

Flip-link changes the default linking setup so that the stack grows down toward the memory boundary instead of toward your variables.  If the stack overflows, it causess a hard fault instead of overwriting other parts of RAM.  It can be installed with Rust's package manager, which will download it, install for your development computer, and include the executable in the cargo binary directory. > `cargo install flip-link`.

Install the `rust-analyzer` extension for VSCode.  This plugin provides language services.

Install the `debugger for probe-rs` extension for VSCode to aid in connecting to your probe.

## Building and testing

The project's VSCode settings are set up so that you should be able to do:

- Terminal > Run Build Task... (or Terminal > Run Task > Build hardware_main (MCU)) to build for the hardware target, but not program it.
- Run > Start debugging... or <F5> to build for the hardware, flash it to the hardware, and connect a "defmt" (deferred formatting) terminal to view debug output.  The debugger is active, so you can set breakpoints, etc.
- Terminal > Run Task > Test business_logic (host) to run unit tests of the business logic crate on your host.
