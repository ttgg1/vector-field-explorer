//! Native-only symbolic derivatives, with meval as the input/output contract.
use std::{fmt, str::FromStr};

use exmex::{
    ExError, Express,
    prelude::{Differentiate, FlatEx},
};
use meval::tokenizer::{Operation, Token};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variable {
    X,
    Y,
    Z,
    T,
}

impl Variable {
    fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
            Self::T => "t",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CasError(String);

impl fmt::Display for CasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for CasError {}
impl From<String> for CasError {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl From<&str> for CasError {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}
impl From<ExError> for CasError {
    fn from(value: ExError) -> Self {
        Self(value.to_string())
    }
}

type Result<T> = std::result::Result<T, CasError>;

fn finite(value: f64) -> Result<String> {
    if !value.is_finite() {
        return Err("non-finite numeric literal or result".into());
    }
    Ok(format!("{value:?}"))
}

fn lower_input(source: &str) -> Result<(String, Vec<String>)> {
    let tokens = meval::tokenizer::tokenize(source).map_err(|e| CasError(e.to_string()))?;
    let rpn = meval::shunting_yard::to_rpn(&tokens).map_err(|e| CasError(e.to_string()))?;
    let mut stack: Vec<String> = Vec::new();
    let mut variables = Vec::new();
    for token in rpn {
        match token {
            Token::Number(n) => stack.push(finite(n)?),
            Token::Var(name) => match name.as_str() {
                "x" | "y" | "z" | "t" => {
                    if !variables.contains(&name) {
                        variables.push(name.clone());
                    }
                    stack.push(name);
                }
                "pi" => stack.push(finite(std::f64::consts::PI)?),
                "e" => stack.push(finite(std::f64::consts::E)?),
                _ => return Err(format!("unsupported identifier `{name}`").into()),
            },
            Token::Unary(op) => {
                let a = stack.pop().ok_or("malformed unary expression")?;
                let sign = match op {
                    Operation::Plus => "+",
                    Operation::Minus => "-",
                    _ => return Err("unsupported unary operator".into()),
                };
                stack.push(format!("({sign}{a})"));
            }
            Token::Binary(op) => {
                let b = stack.pop().ok_or("malformed binary expression")?;
                let a = stack.pop().ok_or("malformed binary expression")?;
                let sign = match op {
                    Operation::Plus => "+",
                    Operation::Minus => "-",
                    Operation::Times => "*",
                    Operation::Div => "/",
                    Operation::Pow => "^",
                    Operation::Rem => return Err("unsupported operator `%`".into()),
                };
                stack.push(format!("({a}{sign}{b})"));
            }
            Token::Func(name, Some(1)) if approved(&name) => {
                let a = stack.pop().ok_or("malformed function expression")?;
                stack.push(format!("{name}({a})"));
            }
            Token::Func(name, _) => {
                return Err(format!("unsupported function or arity `{name}`").into());
            }
            _ => return Err("unsupported input syntax".into()),
        }
    }
    if stack.len() != 1 {
        return Err("malformed expression".into());
    }
    Ok((stack.pop().unwrap(), variables))
}

fn approved(name: &str) -> bool {
    matches!(
        name,
        "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "asinh"
            | "acosh"
            | "atanh"
            | "sqrt"
            | "exp"
            | "ln"
    )
}

fn diff(source: &str, variable: Variable) -> Result<String> {
    let (lowered, variables) = lower_input(source)?;
    if !variables.iter().any(|name| name == variable.name()) {
        return Ok("0".into());
    }
    let expression = FlatEx::<f64>::parse(&lowered)?;
    let names = expression.var_names();
    let index = names
        .iter()
        .position(|name| name == variable.name())
        .ok_or("differentiation variable not found")?;
    let output = expression.partial(index)?.to_string();
    lower_output(&output)
}

pub fn differentiate(source: &str, variable: Variable) -> Result<String> {
    diff(source, variable)
}

fn dimension(dimension: usize, arity: usize) -> Result<()> {
    if !matches!(dimension, 2 | 3) {
        return Err("dimension must be 2 or 3".into());
    }
    if arity != dimension {
        return Err(format!("expected {dimension} components, got {arity}").into());
    }
    Ok(())
}

pub fn gradient(source: &str, dimension: usize) -> Result<Vec<String>> {
    if !matches!(dimension, 2 | 3) {
        return Err("dimension must be 2 or 3".into());
    }
    let vars = [Variable::X, Variable::Y, Variable::Z];
    vars[..dimension].iter().map(|v| diff(source, *v)).collect()
}

pub fn divergence(components: &[&str], dimension: usize) -> Result<String> {
    dimension_check(dimension, components.len())?;
    let vars = [Variable::X, Variable::Y, Variable::Z];
    let mut terms = Vec::new();
    for (source, variable) in components.iter().zip(vars) {
        terms.push(diff(source, variable)?);
    }
    Ok(format!("({})", terms.join("+")))
}

pub fn curl(components: &[&str], dimension: usize) -> Result<Vec<String>> {
    dimension_check(dimension, components.len())?;
    let d = |component, variable| diff(components[component], variable);
    if dimension == 2 {
        return Ok(vec![format!(
            "({}-{})",
            d(1, Variable::X)?,
            d(0, Variable::Y)?
        )]);
    }
    Ok(vec![
        format!("({}-{})", d(2, Variable::Y)?, d(1, Variable::Z)?),
        format!("({}-{})", d(0, Variable::Z)?, d(2, Variable::X)?),
        format!("({}-{})", d(1, Variable::X)?, d(0, Variable::Y)?),
    ])
}

fn dimension_check(d: usize, n: usize) -> Result<()> {
    dimension(d, n)
}

// Parse the engine's printed expression as its own bounded token language. A compact Pratt
// parser keeps braced variable identifiers distinct from ordinary names.
#[derive(Clone, Debug, PartialEq)]
enum OutToken {
    Number(String),
    Function(String),
    Variable(String),
    Op(char),
    LParen,
    RParen,
}

fn lex_output(text: &str) -> Result<Vec<OutToken>> {
    let b = text.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b[i].is_ascii_digit() || b[i] == b'.' {
            let start = i;
            i += 1;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            if i < b.len() && matches!(b[i], b'e' | b'E') {
                i += 1;
                if i < b.len() && matches!(b[i], b'+' | b'-') {
                    i += 1;
                }
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let s = &text[start..i];
            let n = s
                .parse::<f64>()
                .map_err(|_| CasError("malformed engine number".into()))?;
            finite(n)?;
            out.push(OutToken::Number(s.to_owned()));
            continue;
        }
        if b[i] == b'{' {
            let end = text[i + 1..]
                .find('}')
                .ok_or("malformed engine identifier")?
                + i
                + 1;
            let name = &text[i + 1..end];
            if !matches!(name, "x" | "y" | "z" | "t") {
                return Err("unexpected engine identifier".into());
            }
            out.push(OutToken::Variable(name.into()));
            i = end + 1;
            continue;
        }
        if b[i].is_ascii_alphabetic() {
            let start = i;
            while i < b.len() && b[i].is_ascii_alphabetic() {
                i += 1;
            }
            let name = &text[start..i];
            if !approved(name) {
                return Err(format!("unexpected engine identifier `{name}`").into());
            }
            out.push(OutToken::Function(name.into()));
            continue;
        }
        out.push(match b[i] {
            b'+' | b'-' | b'*' | b'/' | b'^' => OutToken::Op(b[i] as char),
            b'(' => OutToken::LParen,
            b')' => OutToken::RParen,
            _ => return Err("unexpected engine output syntax".into()),
        });
        i += 1;
    }
    Ok(out)
}

#[derive(Clone)]
enum Node {
    Atom(String),
    Unary(String, Box<Node>),
    Binary(char, Box<Node>, Box<Node>),
}
impl Node {
    fn emit(&self) -> String {
        match self {
            Self::Atom(s) => s.clone(),
            Self::Unary(op, a) if approved(op) => format!("{op}({})", a.emit()),
            Self::Unary(op, a) => format!("({op}{})", a.emit()),
            Self::Binary(op, a, b) => format!("({}{}{})", a.emit(), op, b.emit()),
        }
    }
}

struct OutputParser {
    tokens: Vec<OutToken>,
    at: usize,
}
impl OutputParser {
    fn parse(&mut self, min: u8) -> Result<Node> {
        let token = self
            .tokens
            .get(self.at)
            .cloned()
            .ok_or("incomplete engine output")?;
        self.at += 1;
        let mut left = match token {
            OutToken::Number(n) => Node::Atom(n),
            OutToken::Variable(n) => Node::Atom(n),
            OutToken::Function(n) => {
                if self.tokens.get(self.at) != Some(&OutToken::LParen) {
                    return Err("function lacks argument".into());
                }
                self.at += 1;
                let arg = self.parse(0)?;
                if self.tokens.get(self.at) != Some(&OutToken::RParen) {
                    return Err("malformed function call".into());
                }
                self.at += 1;
                Node::Unary(n, Box::new(arg))
            }
            OutToken::Op('+' | '-') => {
                let op = match token {
                    OutToken::Op(c) => c,
                    _ => unreachable!(),
                };
                Node::Unary(op.to_string(), Box::new(self.parse(5)?))
            }
            OutToken::LParen => {
                let e = self.parse(0)?;
                if self.tokens.get(self.at) != Some(&OutToken::RParen) {
                    return Err("unbalanced engine output".into());
                }
                self.at += 1;
                e
            }
            _ => return Err("unexpected engine output token".into()),
        };
        while let Some(OutToken::Op(op)) = self.tokens.get(self.at) {
            // These priorities are exmex's defaults, whose meaning differs
            // from meval for each mixed pair (notably `/` versus `*`).
            let prec = match op {
                '+' => 0,
                '-' => 1,
                '*' => 2,
                '/' => 3,
                '^' => 4,
                _ => return Err("unexpected operator".into()),
            };
            if prec < min {
                break;
            }
            let op = *op;
            self.at += 1;
            let right = self.parse(prec + 1)?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }
}

fn lower_output(text: &str) -> Result<String> {
    let tokens = lex_output(text)?;
    let mut parser = OutputParser { tokens, at: 0 };
    let node = parser.parse(0)?;
    if parser.at != parser.tokens.len() {
        return Err("trailing engine output".into());
    }
    let result = node.emit();
    let expression = meval::Expr::from_str(&result).map_err(|e| CasError(e.to_string()))?;
    let _bound = expression
        .bind4("x", "y", "z", "t")
        .map_err(|e| CasError(e.to_string()))?;
    Ok(result)
}
