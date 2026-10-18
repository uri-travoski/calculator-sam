//! Tokenizer for calculator expressions.

use super::CalcError;

/// A single lexical token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// A numeric literal, normalised to a canonical decimal form.
    Num(String),
    /// An identifier: function name, constant or variable (`sin`, `pi`, `ans`, ...).
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Percent,
    Bang,
    LParen,
    RParen,
}

/// Tokenize `src`.
///
/// Accepts both ASCII operators (`*`, `/`, `-`) and the glyphs the on-screen
/// buttons insert (`×`, `÷`, `−`, `π`, `√`). Returns [`CalcError`] on any
/// character that cannot start a token; never panics.
pub fn lex(src: &str) -> Result<Vec<Tok>, CalcError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => i += 1,
            '0'..='9' | '.' => {
                let start = i;
                let mut saw_digit = false;
                let mut seen_dot = false;
                while i < chars.len() {
                    let c = chars[i];
                    if c.is_ascii_digit() {
                        saw_digit = true;
                        i += 1;
                    } else if c == '.' {
                        if seen_dot {
                            // "1.2.3" — reject instead of guessing.
                            return Err(CalcError::InvalidExpression);
                        }
                        seen_dot = true;
                        i += 1;
                    } else if (c == 'e' || c == 'E') && exponent_follows(&chars, i + 1) {
                        i += 1; // consume 'e'
                        if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                            i += 1;
                        }
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                        break;
                    } else {
                        break;
                    }
                }
                if !saw_digit {
                    // A lone "." or ".e5".
                    return Err(CalcError::InvalidExpression);
                }
                let raw: String = chars[start..i].iter().collect();
                out.push(Tok::Num(normalize_number(&raw)));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_')
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                out.push(Tok::Ident(word));
            }
            'π' => {
                out.push(Tok::Ident("pi".to_string()));
                i += 1;
            }
            '√' => {
                out.push(Tok::Ident("sqrt".to_string()));
                i += 1;
            }
            '×' | '·' => {
                out.push(Tok::Star);
                i += 1;
            }
            '÷' => {
                out.push(Tok::Slash);
                i += 1;
            }
            '−' => {
                out.push(Tok::Minus);
                i += 1;
            }
            '+' => {
                out.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                out.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                out.push(Tok::Star);
                i += 1;
            }
            '/' => {
                out.push(Tok::Slash);
                i += 1;
            }
            '^' => {
                out.push(Tok::Caret);
                i += 1;
            }
            '%' => {
                out.push(Tok::Percent);
                i += 1;
            }
            '!' => {
                out.push(Tok::Bang);
                i += 1;
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            _ => return Err(CalcError::InvalidExpression),
        }
    }
    Ok(out)
}

/// True if `chars[j..]` starts with digits, i.e. `1e5` / `1e-5` are literals
/// while `2e` is `2 × e`.
fn exponent_follows(chars: &[char], j: usize) -> bool {
    match chars.get(j) {
        Some(c) if c.is_ascii_digit() => true,
        Some(&c) if c == '+' || c == '-' => matches!(chars.get(j + 1), Some(d) if d.is_ascii_digit()),
        _ => false,
    }
}

/// Canonicalise a numeric literal: `"007"` → `"7"`, `"5."` → `"5"`,
/// `".5"` → `"0.5"`, `"1.e3"` → `"1e3"`.
fn normalize_number(raw: &str) -> String {
    let (mantissa, exp) = match raw.find(['e', 'E']) {
        Some(p) => (&raw[..p], &raw[p..]),
        None => (raw, ""),
    };
    let mut m = mantissa.trim_end_matches('.').to_string();
    if m.is_empty() {
        m.push('0');
    }
    if m.starts_with('.') {
        m.insert(0, '0');
    }
    m.push_str(&exp.to_ascii_lowercase());
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_basic_tokens() {
        let t = lex("1+2*3").unwrap();
        assert_eq!(
            t,
            vec![
                Tok::Num("1".into()),
                Tok::Plus,
                Tok::Num("2".into()),
                Tok::Star,
                Tok::Num("3".into()),
            ]
        );
    }

    #[test]
    fn lexes_glyphs() {
        let t = lex("2×3÷4−5").unwrap();
        assert_eq!(t[1], Tok::Star);
        assert_eq!(t[3], Tok::Slash);
        assert_eq!(t[5], Tok::Minus);
        let t = lex("√(9)π").unwrap();
        assert_eq!(t[0], Tok::Ident("sqrt".into()));
        assert_eq!(t[4], Tok::Ident("pi".into()));
    }

    #[test]
    fn number_forms() {
        assert_eq!(lex(".5").unwrap()[0], Tok::Num("0.5".into()));
        assert_eq!(lex("5.").unwrap()[0], Tok::Num("5".into()));
        assert_eq!(lex("1e3").unwrap()[0], Tok::Num("1e3".into()));
        assert_eq!(lex("1E-3").unwrap()[0], Tok::Num("1e-3".into()));
        // `2e` is 2 × e, not an exponent literal.
        assert_eq!(
            lex("2e").unwrap(),
            vec![Tok::Num("2".into()), Tok::Ident("e".into())]
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert!(lex("2..3").is_err());
        assert!(lex(".").is_err());
        assert!(lex("@").is_err());
        assert!(lex("1,000").is_err());
        assert!(lex("☃").is_err());
        assert!(lex("").is_ok());
    }
}
