//! Expression evaluator built on `rug` (MPFR) with ~50 significant digits.

use rug::ops::Pow;
use rug::float::Constant;
use rug::{Complete, Float, Integer};

use super::parser::{BinOp, Node};
use super::{CalcError, PRECISION};

/// Angle unit used by trigonometric functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AngleMode {
    #[default]
    Deg,
    Rad,
}

/// Evaluation environment: angle unit and the value of `ans`.
#[derive(Debug, Clone)]
pub struct Env<'a> {
    pub angle: AngleMode,
    pub ans: &'a Float,
}

/// True for names the parser treats as functions (they consume an argument).
pub fn is_function_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "sin" | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "ln"
            | "log"
            | "exp"
            | "sqrt"
    )
}

/// Evaluate `node`. Non-finite results (NaN/∞) are mapped to errors so the UI
/// can always show a message instead of `NaN`.
pub fn eval(node: &Node, env: &Env<'_>) -> Result<Float, CalcError> {
    let v = eval_node(node, env)?;
    if v.is_nan() {
        Err(CalcError::InvalidExpression)
    } else if v.is_infinite() {
        Err(CalcError::OutOfRange)
    } else {
        Ok(v)
    }
}

fn eval_node(node: &Node, env: &Env<'_>) -> Result<Float, CalcError> {
    match node {
        Node::Num(s) => parse_decimal(s),
        Node::Var(name) => match name.to_ascii_lowercase().as_str() {
            "pi" => Ok(Float::with_val(PRECISION, Constant::Pi)),
            "e" => Ok(Float::with_val(PRECISION, 1).exp()),
            "ans" => Ok(env.ans.clone()),
            _ => Err(CalcError::UnknownName(name.clone())),
        },
        Node::Neg(x) => Ok(-eval_node(x, env)?),
        Node::Percent(x) => {
            let v = eval_node(x, env)?;
            Ok(v / Float::with_val(PRECISION, 100))
        }
        Node::Fact(x) => factorial(&eval_node(x, env)?),
        Node::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) if matches!(**b, Node::Percent(_)) => {
            // `500-15%` means 500 minus 15% *of 500*, as on a desk calculator.
            let Node::Percent(inner) = &**b else { unreachable!() };
            let x = eval_node(a, env)?;
            let pct = eval_node(inner, env)? / Float::with_val(PRECISION, 100);
            let delta = x.clone() * pct;
            Ok(if matches!(op, BinOp::Add) { x + delta } else { x - delta })
        }
        Node::Bin(op, a, b) => {
            let x = eval_node(a, env)?;
            let y = eval_node(b, env)?;
            match op {
                BinOp::Add => Ok(x + y),
                BinOp::Sub => Ok(x - y),
                BinOp::Mul => Ok(x * y),
                BinOp::Div => {
                    if y.is_zero() {
                        Err(CalcError::DivisionByZero)
                    } else {
                        Ok(x / y)
                    }
                }
                BinOp::Pow => {
                    if y.is_zero() {
                        // anything^0 = 1 (including 0^0)
                        return Ok(Float::with_val(PRECISION, 1));
                    }
                    if x.is_zero() && y < Float::with_val(PRECISION, 0) {
                        return Err(CalcError::DivisionByZero);
                    }
                    Ok(x.pow(&y))
                }
            }
        }
        Node::Call(name, arg) => {
            let a = eval_node(arg, env)?;
            call_function(name, a, env)
        }
    }
}

fn parse_decimal(s: &str) -> Result<Float, CalcError> {
    match Float::parse(s) {
        Ok(parsed) => Ok(Float::with_val(PRECISION, parsed)),
        Err(_) => Err(CalcError::InvalidExpression),
    }
}

/// n! for a non-negative argument. Exact for integers ≤ 100 000, otherwise
/// computed as Γ(n+1). Domain/overflow errors surface as NaN/∞ and are
/// converted by [`eval`].
fn factorial(x: &Float) -> Result<Float, CalcError> {
    if x.is_integer() {
        if *x < 0 {
            return Err(CalcError::InvalidExpression);
        }
        if *x <= 100_000 {
            // x is an exact integer in this range, so the f64 cast is exact.
            let n = x.to_f64() as u32;
            let exact = Integer::factorial(n).complete();
            return Ok(Float::with_val(PRECISION, exact));
        }
        if *x > 1_000_000 {
            return Err(CalcError::OutOfRange);
        }
    }
    let next = x.clone() + Float::with_val(PRECISION, 1);
    Ok(next.gamma())
}

fn call_function(name: &str, arg: Float, env: &Env<'_>) -> Result<Float, CalcError> {
    let is_deg = env.angle == AngleMode::Deg;
    let pi = || Float::with_val(PRECISION, Constant::Pi);
    // degrees → radians for trig input
    let to_rad = |v: Float| {
        if is_deg {
            v * pi() / 180
        } else {
            v
        }
    };
    // radians → degrees for inverse trig output
    let from_rad = |v: Float| {
        if is_deg {
            v * 180 / pi()
        } else {
            v
        }
    };
    let lower = name.to_ascii_lowercase();
    let out = match lower.as_str() {
        "sin" => to_rad(arg).sin(),
        "cos" => to_rad(arg).cos(),
        "tan" => to_rad(arg).tan(),
        "asin" | "arcsin" => from_rad(arg.asin()),
        "acos" | "arccos" => from_rad(arg.acos()),
        "atan" | "arctan" => from_rad(arg.atan()),
        "ln" => arg.ln(),
        "log" => arg.log10(),
        "exp" => arg.exp(),
        "sqrt" => arg.sqrt(),
        _ => return Err(CalcError::UnknownName(name.to_string())),
    };
    Ok(out)
}
