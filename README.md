Simple Rust wrapper for minimp3 prints rough ASCII art of an mp3's waveform/power over time. Usage:

    mp3view filename.mp3

To force ASCII (maybe if your terminal doesn't like unicode)

    mp3view filename.mp3 -a

To build, install Rust, then run `cargo build --release` and the exe will appear in `target/release`.

This program is by <<andi.m.mcclure@gmail.com>> and made available to you as [CC0](https://creativecommons.org/public-domain/cc0/) (public domain).
