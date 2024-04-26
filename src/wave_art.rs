/// Character set for rendering waveform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum WaveCharset {
    Ascii,
    #[default]
    Blocks,
    Dots,
}

/// Approximate 'fullness' of a character on screen. Because characters are
/// rendered differently above and below the centerpoint, this lets us defer
/// choosing characters until later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BlockFullness {
    Empty,
    Third,
    Half,
    TwoThird,
    Full,
}

/// Are we rendering the top half of the waveform, the bottom half, or the center line?
enum Position {
    Upper,
    Lower,
    Middle,
}

/// Draws waveforms to stdout based on the provided magnitudes. The waveform will be
/// rendered using the chosen character set.
///
/// If `scale` is true, the wave will be scaled to fit. Otherwise, it will render with
/// the assumption that the maximum magnitude is 1.
///
/// If `hd` is true, each pair of magnitudes will be rendered into a single character.
/// It's the responsibility of the caller to provide an appropriate number of magnitudes
/// to fill the screen, but not overflow it.
pub fn draw_waves(
    wave_height: usize,
    magnitudes: &[f32],
    scale: bool,
    charset: WaveCharset,
    hd: bool,
) {
    if magnitudes.is_empty() {
        return;
    }

    let max_mag = magnitudes
        .iter()
        .copied()
        .max_by(|a, b| {
            a.partial_cmp(b)
                .expect("NaNs aren't real, and can't hurt us")
        })
        .expect("Magnitudes is non-empty");
    let scaling_factor = if scale {
        if max_mag == 0. {
            0.
        } else {
            max_mag / (wave_height as f32 - 0.5)
        }
    } else {
        1. / wave_height as f32
    };
    let mut rows = vec![];

    rows.resize_with(wave_height, Vec::new);

    for &magnitude in magnitudes {
        let mag = magnitude / scaling_factor;
        let mag_in_window = mag;
        if mag_in_window <= 0. {
            rows[0].push(BlockFullness::Empty);
        } else if mag_in_window >= 0.5 {
            rows[0].push(BlockFullness::Full);
        } else {
            rows[0].push(charset.partial(mag_in_window * 2.));
        }
    }

    for (y, row) in rows.iter_mut().enumerate().skip(1) {
        for &magnitude in magnitudes {
            let mag = magnitude / scaling_factor;
            let mag_in_window = mag - 0.5 - y as f32;
            if mag_in_window <= 0. {
                row.push(BlockFullness::Empty);
            } else if mag_in_window >= 1. {
                row.push(BlockFullness::Full);
            } else {
                row.push(charset.partial(mag_in_window));
            }
        }
    }

    if hd {
        render_hd(&rows, charset);
    } else {
        render_sd(&rows, charset);
    }
}

/// Print the waveform with a single character per magnitude
fn render_sd(rows: &[Vec<BlockFullness>], charset: WaveCharset) {
    for row in rows[1..].iter().rev() {
        println!(
            "{}",
            row.iter()
                .map(|&fullness| charset.render_single(Position::Upper, fullness))
                .collect::<String>()
        );
    }
    println!(
        "{}",
        rows[0]
            .iter()
            .map(|&fullness| charset.render_single(Position::Middle, fullness))
            .collect::<String>()
    );
    for row in &rows[1..] {
        println!(
            "{}",
            row.iter()
                .map(|&fullness| charset.render_single(Position::Lower, fullness))
                .collect::<String>()
        );
    }
}

/// Print the waveform with a single character per two magnitudes. If the number of
/// magnitudes is odd, the last one will be skipped.
fn render_hd(rows: &[Vec<BlockFullness>], charset: WaveCharset) {
    for row in rows[1..].iter().rev() {
        println!(
            "{}",
            row.chunks_exact(2)
                .map(|fullness| charset.render_double(Position::Upper, fullness))
                .collect::<String>()
        );
    }
    println!(
        "{}",
        rows[0]
            .chunks_exact(2)
            .map(|fullness| charset.render_double(Position::Middle, fullness))
            .collect::<String>()
    );
    for row in &rows[1..] {
        println!(
            "{}",
            row.chunks_exact(2)
                .map(|fullness| charset.render_double(Position::Lower, fullness))
                .collect::<String>()
        );
    }
}

impl WaveCharset {
    /// Choose a block fullness based on the character set and magnitude remainder between 0 and 1
    fn partial(self, mag_in_window: f32) -> BlockFullness {
        match self {
            WaveCharset::Ascii | WaveCharset::Blocks => {
                if mag_in_window > 0.66 {
                    BlockFullness::Full
                } else {
                    BlockFullness::Half
                }
            }
            WaveCharset::Dots => {
                if mag_in_window > 0.75 {
                    BlockFullness::Full
                } else if mag_in_window > 0.5 {
                    BlockFullness::TwoThird
                } else {
                    BlockFullness::Third
                }
            }
        }
    }

    /// Choose a character to render the fullness at SD resolution
    fn render_single(self, pos: Position, fullness: BlockFullness) -> char {
        match self {
            WaveCharset::Ascii => match fullness {
                BlockFullness::Empty => ' ',
                BlockFullness::Half => match pos {
                    Position::Upper => ',',
                    Position::Lower => '\'',
                    Position::Middle => '*',
                },
                BlockFullness::Full => '#',
                _ => unreachable!(),
            },
            WaveCharset::Blocks => match fullness {
                BlockFullness::Empty => ' ',
                BlockFullness::Half => match pos {
                    Position::Upper => '▄',
                    Position::Lower => '▀',
                    Position::Middle => '█',
                },
                BlockFullness::Full => '█',
                _ => unreachable!(),
            },
            WaveCharset::Dots => match fullness {
                BlockFullness::Empty => '⠀',
                BlockFullness::Third => match pos {
                    Position::Upper => '⠤',
                    Position::Lower => '⠉',
                    Position::Middle => '⠒',
                },
                BlockFullness::TwoThird => match pos {
                    Position::Upper => '⠶',
                    Position::Lower => '⠛',
                    Position::Middle => '⠿',
                },
                BlockFullness::Full => '⠿',
                BlockFullness::Half => unreachable!(),
            },
        }
    }

    /// Choose a character to render the fullness at HD resolution
    fn render_double(self, pos: Position, fullness: &[BlockFullness]) -> char {
        let &[a, b] = fullness else {
            panic!("Block fullness wasn't 2 elements")
        };
        match self {
            // Rather than figure out proper ASCII characters here, just glue them together
            WaveCharset::Ascii => match a.max(b) {
                BlockFullness::Empty => ' ',
                BlockFullness::Half => match pos {
                    Position::Upper => ',',
                    Position::Lower => '\'',
                    Position::Middle => '*',
                },
                BlockFullness::Full => '#',
                _ => unreachable!(),
            },
            WaveCharset::Blocks => match (a, b) {
                (x, y) if x == y => self.render_single(pos, a),
                // The (Empty, Empty) case is handled above
                (BlockFullness::Empty, _) => '▐',
                (_, BlockFullness::Empty) => '▌',
                _ => '█',
            },
            WaveCharset::Dots => match (a, b) {
                (x, y) if x == y => self.render_single(pos, a),

                // Why am I doing this to myself?
                (BlockFullness::Empty, BlockFullness::Third) => match pos {
                    Position::Upper => '⠠',
                    Position::Lower => '⠈',
                    Position::Middle => '⠐',
                },
                (BlockFullness::Empty, BlockFullness::TwoThird) => match pos {
                    Position::Upper => '⠰',
                    Position::Lower => '⠘',
                    Position::Middle => '⠸',
                },
                (BlockFullness::Empty, BlockFullness::Full) => '⠸',

                (BlockFullness::Third, BlockFullness::Empty) => match pos {
                    Position::Upper => '⠄',
                    Position::Lower => '⠁',
                    Position::Middle => '⠇',
                },
                (BlockFullness::Third, BlockFullness::TwoThird) => match pos {
                    Position::Upper => '⠴',
                    Position::Lower => '⠙',
                    Position::Middle => '⠿',
                },
                (BlockFullness::Third, BlockFullness::Full) => match pos {
                    Position::Upper => '⠼',
                    Position::Lower => '⠹',
                    Position::Middle => '⠿',
                },

                (BlockFullness::TwoThird, BlockFullness::Empty) => match pos {
                    Position::Upper => '⠆',
                    Position::Lower => '⠃',
                    Position::Middle => '⠇',
                },
                (BlockFullness::TwoThird, BlockFullness::Third) => match pos {
                    Position::Upper => '⠦',
                    Position::Lower => '⠙',
                    Position::Middle => '⠿',
                },
                (BlockFullness::TwoThird, BlockFullness::Full) => match pos {
                    Position::Upper => '⠾',
                    Position::Lower => '⠻',
                    Position::Middle => '⠿',
                },

                (BlockFullness::Full, BlockFullness::Empty) => '⠇',
                (BlockFullness::Full, BlockFullness::Third) => match pos {
                    Position::Upper => '⠧',
                    Position::Lower => '⠏',
                    Position::Middle => '⠿',
                },
                (BlockFullness::Full, BlockFullness::TwoThird) => match pos {
                    Position::Upper => '⠷',
                    Position::Lower => '⠟',
                    Position::Middle => '⠿',
                },

                _ => unreachable!(),
            },
        }
    }
}
