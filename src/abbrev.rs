//! Long-number abbreviation for the main display: a number shows at most
//! [`SHOW_DIGITS`] digits followed by `…`; the full number (at most
//! [`MAX_DIGITS`] digits) is one click away.

/// Digits visible in the display before the `…`.
pub const SHOW_DIGITS: usize = 7;
/// Digits a number may have in total.
pub const MAX_DIGITS: usize = 15;

/// Byte ranges of one number token: `start..mant_end` is the digits and
/// decimal point, `mant_end..end` an optional exponent such as `e+20`.
struct Num {
    start: usize,
    mant_end: usize,
    end: usize,
}

fn numbers(s: &str) -> Vec<Num> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let digit = |j: usize| b.get(j).is_some_and(u8::is_ascii_digit);
        if digit(i) || (b[i] == b'.' && digit(i + 1)) {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            let mant_end = i;
            if b.get(i) == Some(&b'e') && matches!(b.get(i + 1), Some(b'+' | b'-')) && digit(i + 2) {
                i += 2;
                while digit(i) {
                    i += 1;
                }
            }
            out.push(Num { start, mant_end, end: i });
        } else {
            i += 1;
        }
    }
    out
}

/// Byte index just after the `n`th digit of `mantissa` (all of it if shorter).
fn cut_after(mantissa: &str, n: usize) -> usize {
    mantissa
        .bytes()
        .enumerate()
        .filter(|(_, c)| c.is_ascii_digit())
        .nth(n.saturating_sub(1))
        .map_or(mantissa.len(), |(i, _)| i + 1)
}

fn digit_count(mantissa: &str) -> usize {
    mantissa.bytes().filter(u8::is_ascii_digit).count()
}

/// `Some(trimmed)` if any number in `s` has more than [`MAX_DIGITS`] digits.
pub fn limit_digits(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut at = 0;
    let mut changed = false;
    for n in numbers(s) {
        let mant = &s[n.start..n.mant_end];
        if digit_count(mant) > MAX_DIGITS {
            out.push_str(&s[at..n.start]);
            out.push_str(&mant[..cut_after(mant, MAX_DIGITS)]);
            out.push_str(&s[n.mant_end..n.end]);
            at = n.end;
            changed = true;
        }
    }
    out.push_str(&s[at..]);
    changed.then_some(out)
}

pub enum Piece {
    Text(String),
    /// A number cut to [`SHOW_DIGITS`] digits (`short`, without the `…`).
    Cut { short: String, full: String },
}

/// Split `s` into plain text and abbreviated numbers.
pub fn pieces(s: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut at = 0;
    for n in numbers(s) {
        let mant = &s[n.start..n.mant_end];
        if digit_count(mant) > SHOW_DIGITS {
            if n.start > at {
                out.push(Piece::Text(s[at..n.start].to_string()));
            }
            out.push(Piece::Cut {
                short: mant[..cut_after(mant, SHOW_DIGITS)].to_string(),
                full: s[n.start..n.end].to_string(),
            });
            at = n.end;
        }
    }
    if at < s.len() {
        out.push(Piece::Text(s[at..].to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(s: &str) -> String {
        pieces(s)
            .iter()
            .map(|p| match p {
                Piece::Text(t) => t.clone(),
                Piece::Cut { short, .. } => format!("{short}…"),
            })
            .collect()
    }

    #[test]
    fn short_numbers_untouched() {
        assert_eq!(flat("1234567+2.5"), "1234567+2.5");
        assert_eq!(flat("1234567.5"), "1234567…");
        assert_eq!(flat("0.123456"), "0.123456");
    }

    #[test]
    fn long_numbers_cut_per_number() {
        assert_eq!(flat("123456789×2"), "1234567…×2");
        assert_eq!(flat("12345678+98765432"), "1234567…+9876543…");
        assert_eq!(flat("-3.14159265358979"), "-3.141592…");
        assert_eq!(flat("1.23456789012345e+20"), "1.234567…");
        assert_eq!(flat("1e+20"), "1e+20");
    }

    #[test]
    fn full_number_is_kept() {
        let Piece::Cut { full, .. } = &pieces("5+1.23456789012345e+20")[1] else { panic!() };
        assert_eq!(full, "1.23456789012345e+20");
    }

    #[test]
    fn digit_limit() {
        assert_eq!(limit_digits("123456789012345"), None);
        assert_eq!(limit_digits("1234567890123456+7").as_deref(), Some("123456789012345+7"));
        assert_eq!(limit_digits("0.12345678901234567").as_deref(), Some("0.12345678901234"));
        assert_eq!(limit_digits("e+5π"), None);
    }
}
