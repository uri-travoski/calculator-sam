//! Pratt (top-down operator precedence) parser for calculator expressions.
//!
//! * left-associative `+ - × ÷` and implicit multiplication
//! * right-associative `^`
//! * prefix unary `+ -`, postfix `%` (percent) and `!` (factorial)
//! * function application: `sin(30)` and tight `√9`
//! * implicit multiplication: `2π`, `3(4+1)`, `2sin(30)`

use super::eval::is_function_name;
use super::lexer::{Tok, lex};
use super::CalcError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

/// The abstract syntax tree produced by [`parse`].
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Num(String),
    Var(String),
    Neg(Box<Node>),
    Bin(BinOp, Box<Node>, Box<Node>),
    /// Postfix factorial: `n!`
    Fact(Box<Node>),
    /// Postfix percent: `50%` → `0.5`
    Percent(Box<Node>),
    /// Function application with one argument: `sin(30)`.
    Call(String, Box<Node>),
}

// Binding powers. Higher binds tighter.
const BP_ADD: u8 = 2; // + -
const BP_MUL: u8 = 4; // * / and implicit multiplication
const BP_UNARY: u8 = 5; // right binding power of prefix +/-
const BP_POW: u8 = 6; // ^ (right-associative)
const BP_POSTFIX: u8 = 8; // ! %

const MAX_INPUT_BYTES: usize = 1500;
const MAX_DEPTH: u32 = 120;

/// Parse a complete expression.
pub fn parse(src: &str) -> Result<Node, CalcError> {
    // Bound the work (and tree depth, which evaluation and drop recurse
    // over) so pasted garbage can never overflow the stack.
    if src.len() > MAX_INPUT_BYTES {
        return Err(CalcError::InvalidExpression);
    }
    let toks = lex(src)?;
    if toks.is_empty() {
        return Err(CalcError::InvalidExpression);
    }
    let mut p = Parser { toks, pos: 0, depth: 0 };
    let node = p.expr(0)?;
    if p.pos != p.toks.len() {
        // The only token that can be left over at min_bp 0 is an unmatched ')'.
        return Err(match p.toks[p.pos] {
            Tok::RParen => CalcError::MismatchedParentheses,
            _ => CalcError::InvalidExpression,
        });
    }
    Ok(node)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    depth: u32,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    /// Pratt loop: parse a left-hand side and fold in any operators whose
    /// left binding power is at least `min_bp`.
    fn expr(&mut self, min_bp: u8) -> Result<Node, CalcError> {
        self.depth += 1;
        let r = if self.depth > MAX_DEPTH {
            Err(CalcError::InvalidExpression)
        } else {
            self.expr_inner(min_bp)
        };
        self.depth -= 1;
        r
    }

    fn expr_inner(&mut self, min_bp: u8) -> Result<Node, CalcError> {
        let mut lhs = self.prefix()?;
        loop {
            let (bp, implicit) = match self.peek() {
                Some(Tok::Plus) | Some(Tok::Minus) => (BP_ADD, false),
                Some(Tok::Star) | Some(Tok::Slash) => (BP_MUL, false),
                Some(Tok::Caret) => (BP_POW, false),
                Some(Tok::Percent) | Some(Tok::Bang) => (BP_POSTFIX, false),
                Some(Tok::Num(_)) | Some(Tok::Ident(_)) | Some(Tok::LParen) => (BP_MUL, true),
                _ => break,
            };
            if bp < min_bp {
                break;
            }
            match self.peek() {
                Some(Tok::Percent) => {
                    self.pos += 1;
                    lhs = Node::Percent(Box::new(lhs));
                }
                Some(Tok::Bang) => {
                    self.pos += 1;
                    lhs = Node::Fact(Box::new(lhs));
                }
                Some(Tok::Plus) => {
                    self.pos += 1;
                    let rhs = self.expr(BP_ADD + 1)?;
                    lhs = Node::Bin(BinOp::Add, Box::new(lhs), Box::new(rhs));
                }
                Some(Tok::Minus) => {
                    self.pos += 1;
                    let rhs = self.expr(BP_ADD + 1)?;
                    lhs = Node::Bin(BinOp::Sub, Box::new(lhs), Box::new(rhs));
                }
                Some(Tok::Star) => {
                    self.pos += 1;
                    let rhs = self.expr(BP_MUL + 1)?;
                    lhs = Node::Bin(BinOp::Mul, Box::new(lhs), Box::new(rhs));
                }
                Some(Tok::Slash) => {
                    self.pos += 1;
                    let rhs = self.expr(BP_MUL + 1)?;
                    lhs = Node::Bin(BinOp::Div, Box::new(lhs), Box::new(rhs));
                }
                Some(Tok::Caret) => {
                    self.pos += 1;
                    let rhs = self.expr(BP_POW)?; // right-associative
                    lhs = Node::Bin(BinOp::Pow, Box::new(lhs), Box::new(rhs));
                }
                _ if implicit => {
                    // Do not consume the token: it starts the right operand.
                    let rhs = self.expr(BP_MUL + 1)?;
                    lhs = Node::Bin(BinOp::Mul, Box::new(lhs), Box::new(rhs));
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    /// Parse a prefix (nud) position.
    fn prefix(&mut self) -> Result<Node, CalcError> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Node::Num(n)),
            Some(Tok::Ident(name)) => {
                if is_function_name(&name) {
                    if matches!(self.peek(), Some(Tok::LParen)) {
                        self.pos += 1;
                        let arg = self.expr(0)?;
                        match self.next() {
                            Some(Tok::RParen) => Ok(Node::Call(name, Box::new(arg))),
                            _ => Err(CalcError::MismatchedParentheses),
                        }
                    } else if self.pos < self.toks.len() {
                        // Tight application: `√9`, `sin-1`.
                        let arg = self.expr(BP_POW)?;
                        Ok(Node::Call(name, Box::new(arg)))
                    } else {
                        Err(CalcError::InvalidExpression)
                    }
                } else {
                    Ok(Node::Var(name))
                }
            }
            Some(Tok::LParen) => {
                let inner = self.expr(0)?;
                match self.next() {
                    Some(Tok::RParen) => Ok(inner),
                    _ => Err(CalcError::MismatchedParentheses),
                }
            }
            Some(Tok::RParen) => Err(CalcError::MismatchedParentheses),
            Some(Tok::Minus) => {
                let operand = self.expr(BP_UNARY)?;
                Ok(Node::Neg(Box::new(operand)))
            }
            Some(Tok::Plus) => self.expr(BP_UNARY),
            _ => Err(CalcError::InvalidExpression),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_tree() {
        // 2 + (3 * 4)
        assert_eq!(
            parse("2+3*4").unwrap(),
            Node::Bin(
                BinOp::Add,
                Box::new(Node::Num("2".into())),
                Box::new(Node::Bin(
                    BinOp::Mul,
                    Box::new(Node::Num("3".into())),
                    Box::new(Node::Num("4".into()))
                ))
            )
        );
    }

    #[test]
    fn pow_is_right_associative() {
        let n = parse("2^3^2").unwrap();
        // 2 ^ (3 ^ 2)
        match n {
            Node::Bin(BinOp::Pow, _, rhs) => match *rhs {
                Node::Bin(BinOp::Pow, _, _) => {}
                other => panic!("expected right-assoc tree, got {other:?}"),
            },
            other => panic!("expected power, got {other:?}"),
        }
    }

    #[test]
    fn unary_minus_binds_looser_than_pow() {
        // -2^2 must parse as -(2^2)
        let n = parse("-2^2").unwrap();
        assert!(matches!(n, Node::Neg(inner) if matches!(
            *inner,
            Node::Bin(BinOp::Pow, _, _)
        )));
    }

    #[test]
    fn implicit_multiplication_nodes() {
        let n = parse("2π").unwrap();
        assert!(matches!(n, Node::Bin(BinOp::Mul, _, _)));
        let n = parse("3(4+1)").unwrap();
        assert!(matches!(n, Node::Bin(BinOp::Mul, _, _)));
    }

    #[test]
    fn postfix_and_grouping() {
        assert!(matches!(parse("50%").unwrap(), Node::Percent(_)));
        assert!(matches!(parse("5!").unwrap(), Node::Fact(_)));
        let n = parse("(1+2)").unwrap();
        assert_eq!(n, Node::Bin(BinOp::Add, Box::new(Node::Num("1".into())), Box::new(Node::Num("2".into()))));
    }

    #[test]
    fn parse_errors() {
        assert_eq!(parse("(1+2").unwrap_err(), CalcError::MismatchedParentheses);
        assert_eq!(parse("1+2)").unwrap_err(), CalcError::MismatchedParentheses);
        assert_eq!(parse("()").unwrap_err(), CalcError::MismatchedParentheses);
        assert_eq!(parse("").unwrap_err(), CalcError::InvalidExpression);
        assert_eq!(parse("+").unwrap_err(), CalcError::InvalidExpression);
        assert_eq!(parse("2*").unwrap_err(), CalcError::InvalidExpression);
        assert_eq!(parse("^2").unwrap_err(), CalcError::InvalidExpression);
    }
}
