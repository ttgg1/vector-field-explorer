//! Bounded linear-to-TeX display printer for CAS results (issue #171).
//!
//! This is a *display* concern only: it never changes the canonical linear
//! formula, and every unsupported construct returns `None` so the UI can keep
//! showing the linear result. The accepted subset is deliberately small:
//!
//! - numbers, the variables `x`, `y`, `z`, `t`, and the constants `pi` and `e`;
//! - the binary operators `+`, `-`, `*`, `/`, `^` and parentheses;
//! - exactly the function names `sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
//!   `sinh`, `cosh`, `tanh`, `sqrt`, `exp`, `ln`, `log`, `abs`, each called
//!   with one parenthesised expression.
//!
//! Whitespace is ignored. Anything else — `=`, `|`, brackets, braces, commas,
//! unknown identifiers, a bare function name, empty input, or more than 512
//! bytes — is unsupported. A top-level parenthesised numerator over a
//! parenthesised denominator becomes `\frac`; every other `/` stays linear
//! because precedence without a structured tree is ambiguous.

/// Inputs longer than this are rejected rather than parsed.
const MAX_INPUT_BYTES: usize = 512;

/// Translate one CAS linear expression into a TeX display string, or `None`
/// when the expression uses anything outside the documented subset.
pub fn linear_to_tex(input: &str) -> Option<String> {
    if input.is_empty() || input.len() > MAX_INPUT_BYTES {
        return None;
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((numerator, denominator)) = parenthesised_fraction(trimmed)
        && let (Some(left), Some(right)) =
            (parse_expression(numerator), parse_expression(denominator))
    {
        return Some(format!(r"\frac{{{left}}}{{{right}}}"));
    }
    parse_expression(trimmed)
}

/// Parse one whole expression and require that the parser consumed it all.
fn parse_expression(input: &str) -> Option<String> {
    let mut parser = Parser {
        input: input.as_bytes(),
        at: 0,
    };
    let tex = parser.expression()?;
    parser.skip_whitespace();
    (parser.at == parser.input.len() && !tex.is_empty()).then_some(tex)
}

/// If `input` is exactly `(A)/(B)` with balanced outer groups, return `A` and
/// `B`. Trailing whitespace is allowed; any other shape returns `None`.
fn parenthesised_fraction(input: &str) -> Option<(&str, &str)> {
    let (numerator, after) = outer_group(input, 0)?;
    let mut at = after;
    at = skip_ws_at(input, at);
    if input.as_bytes().get(at) != Some(&b'/') {
        return None;
    }
    at = skip_ws_at(input, at + 1);
    let (denominator, after) = outer_group(input, at)?;
    (skip_ws_at(input, after) == input.len()).then_some((numerator, denominator))
}

/// Parse a group starting at `start` (after skipped whitespace) and return its
/// contents and the index just past the closing `)`.
fn outer_group(input: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = input.as_bytes();
    let mut at = skip_ws_at(input, start);
    if bytes.get(at) != Some(&b'(') {
        return None;
    }
    let open = at;
    let mut depth = 0usize;
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some((&input[open + 1..at], at + 1));
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

fn skip_ws_at(input: &str, mut at: usize) -> usize {
    let bytes = input.as_bytes();
    while matches!(bytes.get(at), Some(b' ' | b'\t' | b'\r' | b'\n')) {
        at += 1;
    }
    at
}

struct Parser<'a> {
    input: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.input.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.at += 1;
        Some(byte)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.at += 1;
        }
    }

    fn expression(&mut self) -> Option<String> {
        let mut tex = self.term()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'+') => {
                    self.at += 1;
                    let right = self.term()?;
                    tex = format!("{tex}+{right}");
                }
                Some(b'-') => {
                    self.at += 1;
                    let right = self.term()?;
                    tex = format!("{tex}-{right}");
                }
                _ => return Some(tex),
            }
        }
    }

    fn term(&mut self) -> Option<String> {
        let mut tex = self.unary()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'*') => {
                    self.at += 1;
                    let right = self.unary()?;
                    tex = format!(r"{tex}\cdot {right}");
                }
                Some(b'/') => {
                    self.at += 1;
                    let right = self.unary()?;
                    tex = format!("{tex}/{right}");
                }
                _ => return Some(tex),
            }
        }
    }

    fn unary(&mut self) -> Option<String> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'+') => {
                self.at += 1;
                self.unary()
            }
            Some(b'-') => {
                self.at += 1;
                Some(format!("-{}", self.unary()?))
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Option<String> {
        let base = self.atom()?;
        self.skip_whitespace();
        if self.peek() != Some(b'^') {
            return Some(base);
        }
        self.at += 1;
        self.skip_whitespace();
        let exponent = if self.peek() == Some(b'(') {
            let (inner, after) = outer_group(std::str::from_utf8(self.input).ok()?, self.at)?;
            self.at = after;
            parse_expression(inner)?
        } else {
            self.unary()?
        };
        Some(format!("{base}^{{{exponent}}}"))
    }

    fn atom(&mut self) -> Option<String> {
        self.skip_whitespace();
        match self.peek()? {
            b'(' => {
                self.at += 1;
                let inner = self.expression()?;
                self.skip_whitespace();
                if self.bump()? != b')' {
                    return None;
                }
                Some(format!("({inner})"))
            }
            b'0'..=b'9' | b'.' => self.number(),
            byte if byte.is_ascii_alphabetic() => self.identifier(),
            _ => None,
        }
    }

    fn number(&mut self) -> Option<String> {
        let start = self.at;
        let mut seen_dot = false;
        while let Some(byte) = self.peek() {
            match byte {
                b'0'..=b'9' => self.at += 1,
                b'.' if !seen_dot => {
                    seen_dot = true;
                    self.at += 1;
                }
                _ => break,
            }
        }
        let text = std::str::from_utf8(&self.input[start..self.at]).ok()?;
        // `.` alone or a trailing dot is not a number.
        if text == "." || text.ends_with('.') {
            return None;
        }
        Some(text.to_owned())
    }

    fn identifier(&mut self) -> Option<String> {
        let start = self.at;
        while matches!(self.peek(), Some(byte) if byte.is_ascii_alphabetic()) {
            self.at += 1;
        }
        let name = std::str::from_utf8(&self.input[start..self.at]).ok()?;
        let constant = match name {
            "x" | "y" | "z" | "t" => Some(name.to_owned()),
            "pi" => Some(r"\pi".to_owned()),
            "e" => Some("e".to_owned()),
            _ => None,
        };
        if let Some(constant) = constant {
            return Some(constant);
        }
        // A known function must be called with one parenthesised argument.
        let function = function_name(name)?;
        self.skip_whitespace();
        if self.peek() != Some(b'(') {
            return None;
        }
        self.at += 1;
        let argument = self.expression()?;
        self.skip_whitespace();
        if self.bump()? != b')' {
            return None;
        }
        Some(match function {
            Function::Sqrt => format!(r"\sqrt{{{argument}}}"),
            Function::Abs => format!(r"\left|{argument}\right|"),
            Function::Named(command) => format!(r"\{command}({argument})"),
        })
    }
}

enum Function {
    Named(&'static str),
    Sqrt,
    Abs,
}

fn function_name(name: &str) -> Option<Function> {
    Some(match name {
        "sin" => Function::Named("sin"),
        "cos" => Function::Named("cos"),
        "tan" => Function::Named("tan"),
        "asin" => Function::Named("arcsin"),
        "acos" => Function::Named("arccos"),
        "atan" => Function::Named("arctan"),
        "sinh" => Function::Named("sinh"),
        "cosh" => Function::Named("cosh"),
        "tanh" => Function::Named("tanh"),
        "exp" => Function::Named("exp"),
        "ln" => Function::Named("ln"),
        "log" => Function::Named("log"),
        "sqrt" => Function::Sqrt,
        "abs" => Function::Abs,
        _ => return None,
    })
}
