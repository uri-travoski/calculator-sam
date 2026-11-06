//! Expression engine: lexer → Pratt parser → evaluator → formatter.
//!
//! This module is pure Rust with no GTK dependency and is fully unit-tested.

pub mod eval;
pub mod format;
pub mod lexer;
pub mod parser;

use std::fmt;

use rug::Float;

/// Internal working precision: 167 bits ≈ 50 decimal significant digits.
pub const PRECISION: u32 = 167;

/// All errors that can occur while parsing or evaluating an expression.
///
/// The engine never panics on user input; every failure is one of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalcError {
    /// A division (or zero raised to a negative power) by zero.
    DivisionByZero,
    /// Unbalanced `(` / `)` in the expression.
    MismatchedParentheses,
    /// The expression is syntactically or semantically invalid.
    InvalidExpression,
    /// A variable or function that does not exist.
    UnknownName(String),
    /// The result cannot be represented (overflow, factorial too large, ...).
    OutOfRange,
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalcError::DivisionByZero => write!(f, "Division by zero"),
            CalcError::MismatchedParentheses => write!(f, "Mismatched parentheses"),
            CalcError::InvalidExpression => write!(f, "Invalid expression"),
            CalcError::UnknownName(name) => write!(f, "Unknown name: {name}"),
            CalcError::OutOfRange => write!(f, "Result out of range"),
        }
    }
}

impl std::error::Error for CalcError {}

pub use eval::{AngleMode, Env};
pub use format::{DISPLAY_DIGITS, format_float};
pub use parser::parse;

/// Parse and evaluate `expr` in the given environment.
///
/// Returns the full-precision result or a user-displayable error.
pub fn compute(expr: &str, env: &Env<'_>) -> Result<Float, CalcError> {
    let ast = parse(expr)?;
    eval::eval(&ast, env)
}

/// Parse, evaluate and format `expr` in one go.
pub fn compute_display(expr: &str, env: &Env<'_>) -> Result<String, CalcError> {
    compute(expr, env).map(|v| format_float(&v, DISPLAY_DIGITS))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_with(expr: &str, angle: AngleMode, ans: f64) -> Result<String, CalcError> {
        let ans = Float::with_val(PRECISION, ans);
        compute_display(expr, &Env { angle, ans: &ans })
    }

    fn run(expr: &str) -> Result<String, CalcError> {
        run_with(expr, AngleMode::Deg, 0.0)
    }

    fn ok(expr: &str) -> String {
        run(expr).unwrap_or_else(|e| panic!("{expr}: {e}"))
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(ok("2+3*4"), "14");
        assert_eq!(ok("(2+3)*4"), "20");
        assert_eq!(ok("10-4-3"), "3");
        assert_eq!(ok("100/10/5"), "2");
        assert_eq!(ok("2^3^2"), "512");
        assert_eq!(ok("2*3^2"), "18");
    }

    #[test]
    fn percent_of_left_operand_in_add_sub() {
        assert_eq!(ok("500-15%"), "425");
        assert_eq!(ok("200+10%"), "220");
        assert_eq!(ok("500*15%"), "75");
        assert_eq!(ok("50%"), "0.5");
    }

    #[test]
    fn unary_minus() {
        assert_eq!(ok("-2^2"), "-4");
        assert_eq!(ok("(-2)^2"), "4");
        assert_eq!(ok("2^-1"), "0.5");
        assert_eq!(ok("--3"), "3");
        assert_eq!(ok("5--3"), "8");
        assert_eq!(ok("-(2+3)"), "-5");
    }

    #[test]
    fn implicit_multiplication() {
        assert_eq!(ok("3(4+1)"), "15");
        assert_eq!(ok("(1+1)(2+2)"), "8");
        assert_eq!(ok("2π"), "6.28318530717959");
        assert_eq!(ok("2sin(30)"), "1");
        assert_eq!(ok("2e"), "5.43656365691809");
    }

    #[test]
    fn exact_decimals() {
        assert_eq!(ok("0.1+0.2"), "0.3");
        assert_eq!(ok("0.3-0.1"), "0.2");
        assert_eq!(ok("1/3"), "0.333333333333333");
        assert_eq!(ok("0.1*3"), "0.3");
        assert_eq!(ok("1.1*1.1"), "1.21");
    }

    #[test]
    fn scientific_notation_output() {
        assert_eq!(ok("10^20"), "1e+20");
        assert_eq!(ok("10^-12"), "1e-12");
        assert_eq!(ok("1e3+1"), "1001");
    }

    #[test]
    fn factorials() {
        assert_eq!(ok("5!"), "120");
        assert_eq!(ok("0!"), "1");
        assert_eq!(ok("20!"), "2.43290200817664e+18");
        assert_eq!(ok("100!"), "9.33262154439442e+157");
        assert_eq!(ok("1000!"), "4.02387260077094e+2567");
        assert_eq!(ok("0.5!"), "0.886226925452758");
        assert_eq!(run("(-1)!"), Err(CalcError::InvalidExpression));
        assert_eq!(run("10000000!"), Err(CalcError::OutOfRange));
    }

    #[test]
    fn trig_degrees_and_radians() {
        assert_eq!(ok("sin(30)"), "0.5");
        assert_eq!(ok("cos(60)"), "0.5");
        assert_eq!(ok("tan(45)"), "1");
        assert_eq!(ok("asin(0.5)"), "30");
        assert_eq!(ok("atan(1)"), "45");
        let rad = |e: &str| run_with(e, AngleMode::Rad, 0.0).unwrap();
        assert_eq!(rad("sin(π/6)"), "0.5");
        assert_eq!(rad("cos(π)"), "-1");
        assert_eq!(rad("atan(1)"), "0.785398163397448");
    }

    #[test]
    fn other_functions_and_constants() {
        assert_eq!(ok("√144"), "12");
        assert_eq!(ok("√(2)"), "1.4142135623731");
        assert_eq!(ok("ln(e)"), "1");
        assert_eq!(ok("log(1000)"), "3");
        assert_eq!(ok("exp(0)"), "1");
        assert_eq!(ok("50%"), "0.5");
        assert_eq!(ok("π"), "3.14159265358979");
    }

    #[test]
    fn ans_uses_previous_result() {
        assert_eq!(run_with("ans*2", AngleMode::Deg, 21.0).unwrap(), "42");
        assert_eq!(run_with("2ans", AngleMode::Deg, 4.0).unwrap(), "8");
    }

    #[test]
    fn errors_are_reported() {
        assert_eq!(run("1/0"), Err(CalcError::DivisionByZero));
        assert_eq!(run("5/(3-3)"), Err(CalcError::DivisionByZero));
        assert_eq!(run("0^-1"), Err(CalcError::DivisionByZero));
        assert_eq!(run("(1+2"), Err(CalcError::MismatchedParentheses));
        assert_eq!(run("1+2)"), Err(CalcError::MismatchedParentheses));
        assert_eq!(run("1+"), Err(CalcError::InvalidExpression));
        assert_eq!(run(""), Err(CalcError::InvalidExpression));
        assert_eq!(run("√(-1)"), Err(CalcError::InvalidExpression));
        assert!(matches!(run("foo+1"), Err(CalcError::UnknownName(_))));
        assert_eq!(CalcError::DivisionByZero.to_string(), "Division by zero");
        assert_eq!(CalcError::MismatchedParentheses.to_string(), "Mismatched parentheses");
        assert_eq!(CalcError::InvalidExpression.to_string(), "Invalid expression");
    }

    #[test]
    fn malformed_input_never_panics() {
        let samples = [
            "", " ", "(", ")", "((((", "))))", "()", "+", "-", "*", "/", "^", "!", "%", "!!", "%%",
            "1..2", ".", "..", "1e", "1e+", "e1", "2^", "^2", "sin", "sin(", "sin()", "sin)",
            "√", "√√", "π π", "1 2 3", "1,5", "abc", "1/", "/1", "((1)", "(1))", "5!!!", "--",
            "1e999999999", "9^9^9^9", "10^10^10", "1000000!", "tan(90)", "ln(0)", "ln(-1)",
            "log(0)", "asin(2)", "acos(-2)", "0^0", "☃", "1💥2", "\0", "\u{202e}1", "日本語",
            "2 ** 3", "3 +* 4", "(((1+2)*3)", "ans!", "e^e^e", "1e-999999999",
        ];
        for s in samples {
            let _ = run(s);
            let _ = run_with(s, AngleMode::Rad, 1.5);
        }
        // Deep nesting must not blow the stack.
        let deep = "(".repeat(2000) + "1" + &")".repeat(2000);
        assert!(run(&deep).is_err());
        let deep = "-".repeat(1000) + "1";
        assert!(run(&deep).is_err());
        let ok_deep = "(".repeat(50) + "1" + &")".repeat(50);
        assert_eq!(ok(&ok_deep), "1");
        let long = "1+".repeat(500) + "1";
        assert_eq!(ok(&long), "501");
        assert!(run(&"1+".repeat(5000)).is_err());
    }
}
