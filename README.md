Simple Rust wrapper for minimp3/hound prints each individual sample of an mp3/wav. Usage:

    mp3dump filename.mp3

To show as hex or decimal:

    mp3dump filename.mp3 -x
    mp3dump filename.mp3 -d

To build, install Rust, then run `cargo build --release` and the exe will appear in `target/release`.

This program is by <<andi.m.mcclure@gmail.com>> and made available to you as [CC0](https://creativecommons.org/public-domain/cc0/) (public domain).
