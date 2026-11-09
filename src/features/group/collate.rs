//! Approximation of `String.prototype.localeCompare` under ICU's root collation, which orders
//! the member list.

use std::cmp::Ordering;
use std::sync::LazyLock;

/// ASCII characters in CLDR root order: whitespace, punctuation, symbols, digits, then letters
/// with the lower-case form first (`a A b B ..`). Taken from Node 22 / ICU 77 output.
const ASCII_ORDER: &str = "\t\n\u{b}\u{c}\r _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789aAbBcCdDeEfFgGhHiIjJkKlLmMnNoOpPqQrRsStTuUvVwWxXyYzZ";

/// Accented Latin-1 letters grouped by base letter; the position in the group stands in for the
/// accent's secondary weight.
const ACCENTED: [(&str, char); 8] = [
    ("àáâãäå", 'a'),
    ("ç", 'c'),
    ("èéêë", 'e'),
    ("ìíîï", 'i'),
    ("ñ", 'n'),
    ("òóôõöø", 'o'),
    ("ùúûü", 'u'),
    ("ýÿ", 'y'),
];

/// Weights compared in three passes: base letter, accent, case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Weights {
    primary: u32,
    secondary: u32,
    tertiary: u32,
}

/// Characters that ICU treats as ignorable (other control codes) sort first here.
const IGNORABLE: Weights = Weights {
    primary: 0,
    secondary: 0,
    tertiary: 0,
};

static ASCII_WEIGHTS: LazyLock<[Weights; 128]> = LazyLock::new(|| {
    let mut table = [IGNORABLE; 128];
    let letters_at = ASCII_ORDER
        .find('a')
        .expect("letters are in the order table");
    for (index, c) in ASCII_ORDER.char_indices() {
        let rank = index as u32 + 1;
        table[c as usize] = if index >= letters_at {
            // Letters follow as `aA`, `bB`, ..: one primary weight per pair, case breaks the tie.
            let offset = (index - letters_at) as u32;
            Weights {
                primary: letters_at as u32 + 1 + offset / 2,
                secondary: 0,
                tertiary: offset % 2,
            }
        } else {
            Weights {
                primary: rank,
                ..IGNORABLE
            }
        };
    }
    table
});

fn weights(c: char) -> Weights {
    if c.is_ascii() {
        return ASCII_WEIGHTS[c as usize];
    }
    let lower = c.to_lowercase().next().unwrap_or(c);
    let tertiary = u32::from(lower != c);
    for (group, base) in ACCENTED {
        if let Some(position) = group.chars().position(|g| g == lower) {
            return Weights {
                secondary: position as u32 + 1,
                tertiary,
                ..ASCII_WEIGHTS[base as usize]
            };
        }
    }
    // ponytail: other non-ASCII characters sort by code point after ASCII; add a real collator
    // only if member names beyond Latin-1 need exact ICU order.
    Weights {
        primary: 0x1000 + lower as u32,
        secondary: 0,
        tertiary,
    }
}

/// Compares like ICU: all base letters first, then accents, then case.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    let pass = |pick: fn(Weights) -> u32| {
        let key = |s: &str| s.chars().map(|c| pick(weights(c))).collect::<Vec<_>>();
        key(a).cmp(&key(b))
    };
    pass(|w| w.primary)
        .then_with(|| pass(|w| w.secondary))
        .then_with(|| pass(|w| w.tertiary))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut values: Vec<&str>) -> Vec<&str> {
        values.sort_by(|a, b| locale_compare(a, b));
        values
    }

    #[test]
    fn matches_icu_for_ascii_names_and_emails() {
        assert_eq!(
            sorted(vec!["b", "B", "Aa", "ab", "a-b", "a b", "A", "a"]),
            ["a", "A", "a b", "a-b", "Aa", "ab", "b", "B"]
        );
        assert_eq!(
            sorted(vec!["zed@x.id", "Budi", "9lives", "_x", "budi"]),
            ["_x", "9lives", "budi", "Budi", "zed@x.id"]
        );
    }

    #[test]
    fn accents_only_break_ties_between_equal_base_letters() {
        assert_eq!(
            sorted(vec!["f", "É", "e", "é", "Ñandú", "nz", "Álvaro", "Andi"]),
            ["Álvaro", "Andi", "e", "é", "É", "f", "Ñandú", "nz"]
        );
    }
}
