//! Megaminx scramble generation in WCA (Pochmann) notation.

use rand::Rng;

/// Lines per scramble, matching TNoodle's `MegaminxPuzzle`.
const LINES: usize = 7;
/// R and D moves per line, alternating, R first and D last.
const MOVES_PER_LINE: usize = 10;

/// One Megaminx scramble in WCA (Pochmann) notation, lines joined with \n.
///
/// Each line is ten alternating R and D moves, every one an independent coin
/// flip between `++` and `--`, closed by a single U move. That U is not a
/// draw of its own: TNoodle reuses the direction of the line's last D move,
/// so `D++` is always followed by `U` and `D--` by `U'`. Megaminx is a
/// random-move event even officially, so reproducing those emission rules is
/// all that "matches the WCA scrambler" means here.
pub fn scramble<R: Rng>(rng: &mut R) -> String {
    let mut out = String::with_capacity(LINES * 48);

    for line in 0..LINES {
        if line > 0 {
            out.push('\n');
        }

        // Lives outside the loop because the last D move decides the U.
        let mut clockwise = false;
        for i in 0..MOVES_PER_LINE {
            if i > 0 {
                out.push(' ');
            }
            clockwise = rng.gen_bool(0.5);
            out.push(if i % 2 == 0 { 'R' } else { 'D' });
            out.push_str(if clockwise { "++" } else { "--" });
        }

        out.push_str(if clockwise { " U" } else { " U'" });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    /// One R or D move: the face letter and whether it turned clockwise.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Turn {
        face: char,
        clockwise: bool,
    }

    /// A parsed line: its ten R/D turns and the direction of its closing U.
    #[derive(Debug)]
    struct Line {
        turns: Vec<Turn>,
        u_clockwise: bool,
    }

    /// Parse one line, asserting every structural rule of the notation.
    fn parse_line(line: &str) -> Line {
        assert_eq!(line.trim(), line, "line has stray whitespace: {line:?}");
        assert!(!line.contains("  "), "double space in line: {line:?}");

        let tokens: Vec<&str> = line.split(' ').collect();
        assert_eq!(
            tokens.len(),
            MOVES_PER_LINE + 1,
            "line must be {} R/D moves plus one U: {line:?}",
            MOVES_PER_LINE
        );

        let (u_token, move_tokens) = tokens.split_last().expect("non-empty line");

        let turns: Vec<Turn> = move_tokens
            .iter()
            .enumerate()
            .map(|(i, token)| {
                let want_face = if i % 2 == 0 { 'R' } else { 'D' };
                let (face, suffix) = token.split_at(1);
                let face = face.chars().next().expect("token has a face letter");
                assert_eq!(
                    face, want_face,
                    "position {i} must be {want_face}, got {token:?} in {line:?}"
                );
                let clockwise = match suffix {
                    "++" => true,
                    "--" => false,
                    other => panic!("bad suffix {other:?} in token {token:?} of {line:?}"),
                };
                Turn { face, clockwise }
            })
            .collect();

        let u_clockwise = match *u_token {
            "U" => true,
            "U'" => false,
            other => panic!("line must end in U or U', got {other:?} in {line:?}"),
        };

        Line {
            turns,
            u_clockwise,
        }
    }

    /// Parse a whole scramble, asserting the line count and the line joining.
    fn parse(scramble: &str) -> Vec<Line> {
        assert!(
            !scramble.ends_with('\n'),
            "scramble must not end with a newline: {scramble:?}"
        );
        assert!(
            !scramble.contains("\n\n"),
            "scramble must not contain a blank line: {scramble:?}"
        );
        assert!(
            !scramble.contains('\r'),
            "lines are joined with a bare \\n: {scramble:?}"
        );

        let lines: Vec<Line> = scramble.split('\n').map(parse_line).collect();
        assert_eq!(
            lines.len(),
            LINES,
            "scramble must be {LINES} lines: {scramble:?}"
        );
        lines
    }

    fn generate(seed: u64) -> String {
        scramble(&mut StdRng::seed_from_u64(seed))
    }

    #[test]
    fn every_scramble_has_seven_lines() {
        for seed in 0..400u64 {
            let text = generate(seed);
            assert_eq!(
                text.split('\n').count(),
                LINES,
                "seed {seed} produced the wrong line count: {text:?}"
            );
        }
    }

    #[test]
    fn every_line_is_ten_alternating_moves_and_a_u() {
        for seed in 0..400u64 {
            let text = generate(seed);
            for line in parse(&text) {
                assert_eq!(line.turns.len(), MOVES_PER_LINE);
                assert_eq!(line.turns[0].face, 'R', "a line starts with R: {text:?}");
                assert_eq!(
                    line.turns[MOVES_PER_LINE - 1].face,
                    'D',
                    "a line's last R/D move is D: {text:?}"
                );
                for pair in line.turns.chunks(2) {
                    assert_eq!(pair[0].face, 'R');
                    assert_eq!(pair[1].face, 'D');
                }
            }
        }
    }

    #[test]
    fn a_scramble_is_seventy_moves_plus_seven_u_turns() {
        for seed in 0..200u64 {
            let text = generate(seed);
            let lines = parse(&text);
            let turns: usize = lines.iter().map(|l| l.turns.len()).sum();
            assert_eq!(turns, LINES * MOVES_PER_LINE, "seed {seed}: {text:?}");
            assert_eq!(lines.len(), LINES);
        }
    }

    #[test]
    fn the_closing_u_follows_the_last_d_move() {
        // TNoodle reuses the last D move's direction rather than drawing
        // again, so D++ always closes with U and D-- with U'.
        for seed in 0..400u64 {
            let text = generate(seed);
            for line in parse(&text) {
                let last_d = line.turns[MOVES_PER_LINE - 1];
                assert_eq!(last_d.face, 'D');
                assert_eq!(
                    line.u_clockwise, last_d.clockwise,
                    "U direction must match the last D move: {text:?}"
                );
            }
        }
    }

    #[test]
    fn both_directions_and_both_u_turns_appear() {
        let mut plus = false;
        let mut minus = false;
        let mut u = false;
        let mut u_prime = false;
        for seed in 0..50u64 {
            for line in parse(&generate(seed)) {
                for turn in &line.turns {
                    plus |= turn.clockwise;
                    minus |= !turn.clockwise;
                }
                u |= line.u_clockwise;
                u_prime |= !line.u_clockwise;
            }
        }
        assert!(plus && minus, "both ++ and -- must occur");
        assert!(u && u_prime, "both U and U' must occur");
    }

    #[test]
    fn directions_are_a_fair_coin() {
        let mut clockwise = 0usize;
        let mut total = 0usize;
        for seed in 0..300u64 {
            for line in parse(&generate(seed)) {
                for turn in &line.turns {
                    total += 1;
                    clockwise += usize::from(turn.clockwise);
                }
            }
        }
        assert_eq!(total, 300 * LINES * MOVES_PER_LINE);
        // 21000 draws, so a fair coin sits far inside this band.
        let ratio = clockwise as f64 / total as f64;
        assert!(
            (0.45..=0.55).contains(&ratio),
            "++ appeared {clockwise} of {total} times, ratio {ratio}"
        );
    }

    #[test]
    fn seeded_generation_is_deterministic() {
        for seed in 0..64u64 {
            assert_eq!(
                generate(seed),
                generate(seed),
                "seed {seed} must always give the same scramble"
            );
        }
        assert_ne!(
            generate(1234),
            generate(4321),
            "different seeds should give different scrambles"
        );
    }

    #[test]
    fn scrambles_use_only_the_notation_alphabet() {
        for seed in 0..100u64 {
            let text = generate(seed);
            for ch in text.chars() {
                assert!(
                    matches!(ch, 'R' | 'D' | 'U' | '+' | '-' | '\'' | ' ' | '\n'),
                    "unexpected character {ch:?} in {text:?}"
                );
            }
        }
    }

    #[test]
    fn a_known_seed_parses_and_round_trips_its_shape() {
        let text = generate(7);
        let lines = parse(&text);
        let rebuilt: Vec<String> = lines
            .iter()
            .map(|line| {
                let mut s: Vec<String> = line
                    .turns
                    .iter()
                    .map(|t| format!("{}{}", t.face, if t.clockwise { "++" } else { "--" }))
                    .collect();
                s.push(if line.u_clockwise { "U" } else { "U'" }.to_string());
                s.join(" ")
            })
            .collect();
        assert_eq!(
            rebuilt.join("\n"),
            text,
            "the parser must reproduce the emitted text exactly"
        );
    }
}
