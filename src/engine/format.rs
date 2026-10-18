//! Result formatting: round to N significant digits, trim trailing zeros,
//! switch to scientific notation for very large/small numbers.

use rug::Float;

/// Default number of significant digits shown to the user.
pub const DISPLAY_DIGITS: usize = 15;

/// Extra digits requested from MPFR so that rounding to
/// [`DISPLAY_DIGITS`] is always correct.
const GUARD_DIGITS: usize = 60;

/// Format a finite value for display.
///
/// * zero → `"0"`
/// * up to `max_sig` significant digits, trailing zeros trimmed
/// * scientific notation (`1e+20`, `1.5e-10`) outside `[1e-9, 1e15)`
///
/// Falls back to the raw MPFR string if it cannot be understood; never panics.
pub fn format_float(value: &Float, max_sig: usize) -> String {
    if value.is_zero() {
        return "0".to_string();
    }
    let raw = value.to_string_radix(10, Some(GUARD_DIGITS));
    format_decimal_str(&raw, max_sig)
}

/// Format a plain decimal string (as produced by `Float::to_string_radix`)
/// with `max_sig` significant digits. Pure function — unit tested directly.
pub fn format_decimal_str(raw: &str, max_sig: usize) -> String {
    let max_sig = max_sig.max(1);
    let (neg, digits, exp) = match parse_decimal(raw) {
        Some(parts) => parts,
        None => return raw.to_string(),
    };
    // Strip leading zeros; the integer value does not change.
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return "0".to_string();
    }
    let mut digits = digits.to_string();
    let mut exp = exp;

    // Round to max_sig significant digits (half-up).
    if digits.len() > max_sig {
        let mut kept: String = digits[..max_sig].to_string();
        let round_up = digits.as_bytes()[max_sig] >= b'5';
        exp += (digits.len() - max_sig) as i32;
        if round_up && increment(&mut kept) {
            // 999…9 + 1 = 1000…0: value grew by one decimal order.
            exp += 1;
        }
        digits = kept;
    }

    // Trim insignificant trailing zeros while the exponent is still negative
    // (fractional values); this is what turns 0.300000000000000 into 0.3
    // while keeping 120 as 120.
    while exp < 0 && digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
        exp += 1;
    }

    // Position of the most significant digit: value ≈ d.ddd × 10^big_e.
    let big_e = exp + digits.len() as i32 - 1;
    let body = if big_e > 14 || big_e < -9 {
        // Scientific notation with trailing zeros trimmed.
        let trimmed = digits.trim_end_matches('0');
        let mut mantissa = String::with_capacity(trimmed.len() + 1);
        mantissa.push_str(&trimmed[..1]);
        if trimmed.len() > 1 {
            mantissa.push('.');
            mantissa.push_str(&trimmed[1..]);
        }
        let sign = if big_e < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{}", big_e.abs())
    } else if exp >= 0 {
        let mut s = digits;
        for _ in 0..exp {
            s.push('0');
        }
        s
    } else {
        let gap = (-exp) as usize;
        if gap < digits.len() {
            let split = digits.len() - gap;
            format!("{}.{}", &digits[..split], &digits[split..])
        } else {
            format!("0.{}{}", "0".repeat(gap - digits.len()), digits)
        }
    };
    if neg {
        format!("-{body}")
    } else {
        body
    }
}

/// Split a decimal string into `(negative, digits, exp)` such that
/// `value = int(digits) × 10^exp`. Handles `123`, `-1.5`, `1.5e-7`.
fn parse_decimal(raw: &str) -> Option<(bool, String, i32)> {
    let s = raw.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => match s.strip_prefix('+') {
            Some(r) => (false, r),
            None => (false, s),
        },
    };
    let (mantissa, exp_str) = match s.find(['e', 'E']) {
        Some(p) => (&s[..p], &s[p + 1..]),
        None => (s, "0"),
    };
    let exp_extra: i32 = exp_str.parse().ok()?;
    let (int_part, frac_part) = match mantissa.find('.') {
        Some(p) => (&mantissa[..p], &mantissa[p + 1..]),
        None => (mantissa, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.bytes().chain(frac_part.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut digits = String::with_capacity(int_part.len() + frac_part.len());
    digits.push_str(int_part);
    digits.push_str(frac_part);
    let exp = exp_extra - frac_part.len() as i32;
    Some((neg, digits, exp))
}

/// Increment the decimal digit string in place.
/// Returns `true` if the increment carried out of the most significant digit
/// (i.e. the string was 999…9 and is now 100…0 with the same length).
fn increment(digits: &mut String) -> bool {
    let mut bytes = digits.as_bytes().to_vec();
    for i in (0..bytes.len()).rev() {
        if bytes[i] == b'9' {
            bytes[i] = b'0';
        } else {
            bytes[i] += 1;
            *digits = String::from_utf8(bytes).unwrap_or_else(|_| digits.clone());
            return false;
        }
    }
    // All nines became zeros: value is now 10^len.
    let len = digits.len();
    *digits = format!("1{}", "0".repeat(len.saturating_sub(1)));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(raw: &str) -> String {
        format_decimal_str(raw, DISPLAY_DIGITS)
    }

    #[test]
    fn trims_trailing_zeros() {
        assert_eq!(fmt("0.300000000000000000004996003610813204431894734913"), "0.3");
        assert_eq!(fmt("2.3000000000000000"), "2.3");
        assert_eq!(fmt("120"), "120");
        assert_eq!(fmt("100"), "100");
        assert_eq!(fmt("1.5000000000000000000"), "1.5");
    }

    #[test]
    fn rounds_half_up_at_15_digits() {
        assert_eq!(fmt("0.1000000000000000000016653345369377348106"), "0.1");
        // 16th significant digit is 5 → round the 15th up.
        assert_eq!(fmt("1.234567890123455"), "1.23456789012346");
        // Carry across nines.
        assert_eq!(fmt("9.999999999999999999"), "10");
        assert_eq!(fmt("0.999999999999999999"), "1");
    }

    #[test]
    fn scientific_notation() {
        assert_eq!(fmt("1.0000000000000000e20"), "1e+20");
        assert_eq!(fmt("1.5000000000000000e30"), "1.5e+30");
        assert_eq!(fmt("1.2345678901234500e-15"), "1.23456789012345e-15");
        assert_eq!(fmt("1e-10"), "1e-10");
        assert_eq!(fmt("1e-9"), "0.000000001");
        assert_eq!(fmt("999999999999999"), "999999999999999");
        assert_eq!(fmt("1000000000000000"), "1e+15");
    }

    #[test]
    fn plain_decimal_forms() {
        assert_eq!(fmt("0.000123456789012345678"), "0.000123456789012346");
        assert_eq!(fmt("12.5"), "12.5");
        assert_eq!(fmt("-1.5000000000000000"), "-1.5");
        assert_eq!(fmt("123.4567890123456789"), "123.456789012346");
    }

    #[test]
    fn zeros_and_garbage() {
        assert_eq!(fmt("0"), "0");
        assert_eq!(fmt("-0.0000"), "0");
        assert_eq!(fmt("inf"), "inf"); // defensive fallback, no panic
        assert_eq!(fmt(""), "");
    }

    #[test]
    fn from_float() {
        let zero = Float::with_val(167, 0);
        assert_eq!(format_float(&zero, DISPLAY_DIGITS), "0");
        let two = Float::with_val(167, 2);
        assert_eq!(format_float(&two, DISPLAY_DIGITS), "2");
    }
}
