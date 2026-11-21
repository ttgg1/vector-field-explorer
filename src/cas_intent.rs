//! Tolerant CAS input grammar and intent classification (#168).
//!
//! This module is deliberately pure: it never calls the symbolic engine, the
//! UI, or application state. It bounds, normalises, and classifies the text a
//! user is about to hand to the CAS, so the panel can echo the normalised form
//! and show the inferred operation while keeping the dropdown an override.

use std::fmt;

/// The largest accepted input, in UTF-8 bytes.
pub const MAX_INPUT_BYTES: usize = 512;
/// The deepest accepted bracket nesting, across `()`, `[]` and `{}`.
pub const MAX_NESTING: usize = 32;
/// The most `at name=value` assignments accepted by [`CasIntent::Evaluate`].
pub const MAX_ASSIGNMENTS: usize = 8;

/// The operation inferred from a CAS input line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CasIntent {
    Derivative {
        variable: String,
        expr: String,
    },
    Gradient {
        expr: String,
    },
    Divergence {
        expr: String,
    },
    Curl {
        expr: String,
    },
    Integral {
        variable: String,
        expr: String,
        bounds: Option<(String, String)>,
    },
    Solve {
        expr: String,
        variable: Option<String>,
    },
    Simplify {
        expr: String,
    },
    Evaluate {
        expr: String,
        assignments: Vec<(String, String)>,
    },
    Plain {
        expr: String,
    },
}

impl CasIntent {
    /// The menu-facing label for the inferred operation.
    pub fn operation_label(&self) -> &'static str {
        match self {
            Self::Derivative { .. } => "Derivative",
            Self::Gradient { .. } => "Gradient",
            Self::Divergence { .. } => "Divergence",
            Self::Curl { .. } => "Curl",
            Self::Integral { .. } => "Integral",
            Self::Solve { .. } => "Solve",
            Self::Simplify { .. } => "Simplify",
            Self::Evaluate { .. } => "Evaluate",
            Self::Plain { .. } => "Expression",
        }
    }
}

/// A classified input plus the normalised text the panel echoes and a backend
/// would receive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CasParse {
    pub intent: CasIntent,
    pub normalized: String,
}

/// Why a CAS input line was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CasIntentError {
    Empty,
    TooLong,
    TooDeep,
    MissingExpression,
    BadVariable(String),
    BadArity,
    UnbalancedDelimiters,
    Unknown(String),
}

impl fmt::Display for CasIntentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("CAS input is empty"),
            Self::TooLong => {
                write!(f, "CAS input exceeds the {MAX_INPUT_BYTES}-byte limit")
            }
            Self::TooDeep => {
                write!(f, "CAS input exceeds {MAX_NESTING} nesting levels")
            }
            Self::MissingExpression => f.write_str(
                "the operator needs an expression; accepted forms include \
                 `d/dx expr`, `grad expr`, `div expr`, `curl expr`, \
                 `∫ expr dx`, `solve expr` and `simplify expr`",
            ),
            Self::BadVariable(token) => write!(
                f,
                "unsupported variable `{token}`; use one name matching \
                 [A-Za-z][A-Za-z0-9_]*"
            ),
            Self::BadArity => f.write_str(
                "wrong number of arguments; accepted forms include \
                 `diff(expr, var)`, `integrate(expr, var[, a, b])` and at \
                 most 8 `at name=value` assignments",
            ),
            Self::UnbalancedDelimiters => f.write_str("CAS input has unbalanced delimiters"),
            Self::Unknown(detail) => write!(f, "unrecognised CAS form: {detail}"),
        }
    }
}

impl std::error::Error for CasIntentError {}

/// Bound, normalise, and classify `input`.
///
/// Classification is total over the accepted input: malformed recognised
/// operators return an error rather than silently degrading to
/// [`CasIntent::Plain`].
pub fn classify(input: &str) -> Result<CasParse, CasIntentError> {
    check_bounds(input)?;
    let normalized = normalize(input);
    if normalized.is_empty() {
        return Err(CasIntentError::Empty);
    }
    let intent = classify_normalized(&normalized)?;
    Ok(CasParse { intent, normalized })
}

/// Normalise LaTeX-lite and unicode operators, trim, and collapse whitespace.
///
/// Each rewrite is intentional and idempotent:
/// - `\frac{a}{b}` becomes `(a)/(b)` and `\sqrt{a}` becomes `sqrt(a)`;
/// - `\cdot` and `·` become `*`, `\times` stays `×`;
/// - `\partial` and `∂` become `d`, while `\nabla` and `∇` stay `∇`;
/// - `^{n}` (single-token exponents only) and superscript characters become
///   `^n` forms;
/// - `\left` and `\right` are dropped;
/// - `−` (U+2212) becomes `-` and `∗` becomes `*`;
/// - runs of whitespace collapse to one space and the result is trimmed.
pub fn normalize(input: &str) -> String {
    let collapsed = collapse_whitespace(input);
    // `\frac`/`\sqrt` rewriting recurses once per nested group. `normalize` is
    // public and has no error channel, so a direct caller can hand it input
    // deeper than `MAX_NESTING`. Skipping the group rewrites in that case
    // keeps recursion bounded; the remaining rewrites preserve brace counts,
    // so the result is still idempotent.
    let rewrite_groups = max_bracket_depth(&collapsed) <= MAX_NESTING;
    let rewritten = normalize_text(&collapsed, rewrite_groups);
    collapse_whitespace(&rewritten)
}

/// The deepest `()`, `[]`, `{}` nesting reached while scanning `input`.
fn max_bracket_depth(input: &str) -> usize {
    let mut depth = 0usize;
    let mut deepest = 0usize;
    for ch in input.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

fn collapse_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut pending_space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            pending_space = !output.is_empty();
        } else {
            if pending_space {
                output.push(' ');
                pending_space = false;
            }
            output.push(ch);
        }
    }
    output
}

fn normalize_text(input: &str, rewrite_groups: bool) -> String {
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        let rest = &input[index..];
        if rest.starts_with('\\')
            && let Some(consumed) = normalize_command(rest, &mut output, rewrite_groups)
        {
            index += consumed;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        match ch {
            '^' if rewrite_groups => {
                if let Some(consumed) = superscript_group(rest, &mut output) {
                    index += consumed;
                } else {
                    output.push('^');
                    index += 1;
                }
            }
            _ if superscript_char(ch).is_some() => {
                let (mapped, consumed) = take_superscript(rest);
                output.push('^');
                output.push_str(&mapped);
                index += consumed;
            }
            '·' | '∗' => {
                output.push('*');
                index += ch.len_utf8();
            }
            '−' => {
                output.push('-');
                index += ch.len_utf8();
            }
            '∂' => {
                output.push('d');
                index += ch.len_utf8();
            }
            _ => {
                output.push(ch);
                index += ch.len_utf8();
            }
        }
    }
    output
}

fn normalize_command(rest: &str, output: &mut String, rewrite_groups: bool) -> Option<usize> {
    if let Some(len) = command_prefix(rest, "\\frac") {
        if rewrite_groups
            && let Some((numerator, numerator_len)) = brace_group(&rest[len..])
            && let Some((denominator, denominator_len)) = brace_group(&rest[len + numerator_len..])
        {
            output.push('(');
            output.push_str(&normalize_text(numerator, rewrite_groups));
            output.push_str(")/(");
            output.push_str(&normalize_text(denominator, rewrite_groups));
            output.push(')');
            return Some(len + numerator_len + denominator_len);
        }
        output.push_str("\\frac");
        return Some(len);
    }
    if let Some(len) = command_prefix(rest, "\\sqrt") {
        if rewrite_groups && let Some((radicand, radicand_len)) = brace_group(&rest[len..]) {
            output.push_str("sqrt(");
            output.push_str(&normalize_text(radicand, rewrite_groups));
            output.push(')');
            return Some(len + radicand_len);
        }
        output.push_str("\\sqrt");
        return Some(len);
    }
    for (command, replacement) in [
        ("\\cdot", "*"),
        ("\\times", "×"),
        ("\\partial", "d"),
        ("\\nabla", "∇"),
        ("\\left", ""),
        ("\\right", ""),
    ] {
        if let Some(len) = command_prefix(rest, command) {
            output.push_str(replacement);
            return Some(len);
        }
    }
    None
}

fn command_prefix(rest: &str, command: &str) -> Option<usize> {
    let after = rest.strip_prefix(command)?;
    match after.chars().next() {
        Some(ch) if ch.is_ascii_alphabetic() => None,
        _ => Some(command.len()),
    }
}

/// Return the contents of a balanced `{...}` group and its consumed byte length.
fn brace_group(input: &str) -> Option<(&str, usize)> {
    let trimmed = input.trim_start_matches(' ');
    let leading = input.len() - trimmed.len();
    if !trimmed.starts_with('{') {
        return None;
    }
    let mut depth = 0usize;
    for (index, ch) in trimmed.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some((&trimmed[1..index], leading + index + 1));
                }
            }
            _ => {}
        }
    }
    None
}

fn superscript_char(ch: char) -> Option<char> {
    Some(match ch {
        '⁰' => '0',
        '¹' => '1',
        '²' => '2',
        '³' => '3',
        '⁴' => '4',
        '⁵' => '5',
        '⁶' => '6',
        '⁷' => '7',
        '⁸' => '8',
        '⁹' => '9',
        '⁺' => '+',
        '⁻' => '-',
        '⁼' => '=',
        '⁽' => '(',
        '⁾' => ')',
        'ⁿ' => 'n',
        'ⁱ' => 'i',
        _ => return None,
    })
}

fn take_superscript(input: &str) -> (String, usize) {
    let mut mapped = String::new();
    let mut consumed = 0;
    for ch in input.chars() {
        let Some(plain) = superscript_char(ch) else {
            break;
        };
        mapped.push(plain);
        consumed += ch.len_utf8();
    }
    (mapped, consumed)
}

/// Rewrite `^{2}` to `^2` when the exponent is one simple token.
fn superscript_group(rest: &str, output: &mut String) -> Option<usize> {
    let after = rest.strip_prefix("^{")?;
    let close = after.find('}')?;
    let exponent = &after[..close];
    if exponent.is_empty()
        || !exponent
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    output.push('^');
    output.push_str(exponent);
    Some(2 + close + 1)
}

fn check_bounds(input: &str) -> Result<(), CasIntentError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(CasIntentError::TooLong);
    }
    let mut stack = Vec::new();
    for ch in input.chars() {
        match ch {
            '(' | '[' | '{' => {
                stack.push(ch);
                if stack.len() > MAX_NESTING {
                    return Err(CasIntentError::TooDeep);
                }
            }
            ')' | ']' | '}' => {
                let expected = match ch {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                if stack.pop() != Some(expected) {
                    return Err(CasIntentError::UnbalancedDelimiters);
                }
            }
            _ => {}
        }
    }
    if stack.is_empty() {
        Ok(())
    } else {
        Err(CasIntentError::UnbalancedDelimiters)
    }
}

fn classify_normalized(input: &str) -> Result<CasIntent, CasIntentError> {
    if let Some(rest) = input.strip_prefix("d/d") {
        let rest = rest.trim_start();
        let Some((variable, after)) = take_identifier(rest) else {
            let token = rest.split_whitespace().next().unwrap_or("");
            if token.is_empty() {
                return Err(CasIntentError::MissingExpression);
            }
            return Err(CasIntentError::BadVariable(token.to_owned()));
        };
        let expr = after.trim();
        if expr.is_empty() {
            return Err(CasIntentError::MissingExpression);
        }
        return Ok(CasIntent::Derivative {
            variable: variable.to_owned(),
            expr: expr.to_owned(),
        });
    }

    if let Some(rest) = keyword_rest(input, "ddx") {
        let expr = rest.trim();
        if expr.is_empty() {
            return Err(CasIntentError::MissingExpression);
        }
        return Ok(CasIntent::Derivative {
            variable: "x".to_owned(),
            expr: expr.to_owned(),
        });
    }

    if let Some(rest) = keyword_rest(input, "diff") {
        let args = call_args(rest, "diff")?;
        if args.len() != 2 {
            return Err(CasIntentError::BadArity);
        }
        let expr = args[0].trim();
        if expr.is_empty() {
            return Err(CasIntentError::MissingExpression);
        }
        let variable = args[1].trim();
        validate_variable(variable)?;
        return Ok(CasIntent::Derivative {
            variable: variable.to_owned(),
            expr: expr.to_owned(),
        });
    }

    if let Some(rest) = keyword_rest(input, "grad") {
        let expr = rest.trim();
        if expr.is_empty() {
            return Err(CasIntentError::MissingExpression);
        }
        return Ok(CasIntent::Gradient {
            expr: expr.to_owned(),
        });
    }

    if let Some(rest) = input.strip_prefix('∇') {
        return nabla_intent(rest);
    }

    if let Some(rest) = keyword_rest(input, "div") {
        let expr = rest.trim();
        if expr.is_empty() {
            return Err(CasIntentError::MissingExpression);
        }
        return Ok(CasIntent::Divergence {
            expr: expr.to_owned(),
        });
    }

    if let Some(rest) = keyword_rest(input, "curl") {
        let expr = rest.trim();
        if expr.is_empty() {
            return Err(CasIntentError::MissingExpression);
        }
        return Ok(CasIntent::Curl {
            expr: expr.to_owned(),
        });
    }

    if let Some(rest) = input.strip_prefix('∫') {
        return integral_indefinite(rest);
    }

    if let Some(rest) = keyword_rest(input, "integrate") {
        return integral_call(rest);
    }

    if let Some(rest) = keyword_rest(input, "solve") {
        return solve_intent(rest);
    }

    if let Some(rest) = keyword_rest(input, "simplify") {
        let expr = rest.trim();
        if expr.is_empty() {
            return Err(CasIntentError::MissingExpression);
        }
        return Ok(CasIntent::Simplify {
            expr: expr.to_owned(),
        });
    }

    if let Some((expr, tail)) = input.rsplit_once(" at ") {
        let expr = expr.trim();
        if expr.is_empty() {
            return Err(CasIntentError::MissingExpression);
        }
        let assignments = parse_assignments(tail)?;
        return Ok(CasIntent::Evaluate {
            expr: expr.to_owned(),
            assignments,
        });
    }

    Ok(CasIntent::Plain {
        expr: input.to_owned(),
    })
}

fn nabla_intent(rest: &str) -> Result<CasIntent, CasIntentError> {
    let rest = rest.trim_start();
    // `·` is normalised to `*`, so both spellings mean divergence. `×` is the
    // normalised form of `\times`.
    if let Some(after) = rest.strip_prefix('·').or_else(|| rest.strip_prefix('*')) {
        let expr = strip_outer_parens(after);
        if expr.is_empty() {
            return Err(CasIntentError::MissingExpression);
        }
        return Ok(CasIntent::Divergence {
            expr: expr.to_owned(),
        });
    }
    if let Some(after) = rest.strip_prefix('×') {
        let expr = strip_outer_parens(after);
        if expr.is_empty() {
            return Err(CasIntentError::MissingExpression);
        }
        return Ok(CasIntent::Curl {
            expr: expr.to_owned(),
        });
    }
    let expr = strip_outer_parens(rest);
    if expr.is_empty() {
        return Err(CasIntentError::MissingExpression);
    }
    Ok(CasIntent::Gradient {
        expr: expr.to_owned(),
    })
}

fn integral_indefinite(rest: &str) -> Result<CasIntent, CasIntentError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(CasIntentError::MissingExpression);
    }
    let (expr, differential) = rest.rsplit_once(char::is_whitespace).unwrap_or(("", rest));
    let differential = differential.trim();
    let Some(variable) = differential.strip_prefix('d') else {
        return Err(CasIntentError::Unknown(
            "an integral needs a trailing differential such as `dx`".to_owned(),
        ));
    };
    if variable.is_empty() {
        return Err(CasIntentError::Unknown(
            "an integral differential must name a variable".to_owned(),
        ));
    }
    validate_variable(variable)?;
    let expr = expr.trim();
    if expr.is_empty() {
        return Err(CasIntentError::MissingExpression);
    }
    Ok(CasIntent::Integral {
        variable: variable.to_owned(),
        expr: expr.to_owned(),
        bounds: None,
    })
}

fn integral_call(rest: &str) -> Result<CasIntent, CasIntentError> {
    let args = call_args(rest, "integrate")?;
    let (expr, variable, bounds) = match args.as_slice() {
        [expr, variable] => (*expr, *variable, None),
        [expr, variable, lower, upper] => {
            let lower = lower.trim();
            let upper = upper.trim();
            if lower.is_empty() || upper.is_empty() {
                return Err(CasIntentError::Unknown(
                    "integral bounds must not be empty".to_owned(),
                ));
            }
            (*expr, *variable, Some((lower.to_owned(), upper.to_owned())))
        }
        _ => return Err(CasIntentError::BadArity),
    };
    let expr = expr.trim();
    if expr.is_empty() {
        return Err(CasIntentError::MissingExpression);
    }
    let variable = variable.trim();
    validate_variable(variable)?;
    Ok(CasIntent::Integral {
        variable: variable.to_owned(),
        expr: expr.to_owned(),
        bounds,
    })
}

fn solve_intent(rest: &str) -> Result<CasIntent, CasIntentError> {
    let trimmed = rest.trim_start();
    if trimmed.starts_with('(') {
        let args = call_args(rest, "solve")?;
        return match args.as_slice() {
            [expr, variable] => {
                let expr = expr.trim();
                if expr.is_empty() {
                    return Err(CasIntentError::MissingExpression);
                }
                let variable = variable.trim();
                validate_variable(variable)?;
                Ok(CasIntent::Solve {
                    expr: expr.to_owned(),
                    variable: Some(variable.to_owned()),
                })
            }
            // `solve(expr)` is the prefix form with a parenthesised expression.
            [_] => Ok(CasIntent::Solve {
                expr: trimmed.to_owned(),
                variable: None,
            }),
            _ => Err(CasIntentError::BadArity),
        };
    }
    let expr = rest.trim();
    if expr.is_empty() {
        return Err(CasIntentError::MissingExpression);
    }
    Ok(CasIntent::Solve {
        expr: expr.to_owned(),
        variable: None,
    })
}

fn parse_assignments(tail: &str) -> Result<Vec<(String, String)>, CasIntentError> {
    let pieces = split_top_level(tail);
    if pieces.is_empty() {
        return Err(CasIntentError::Unknown(
            "`at` needs at least one `name=value` assignment".to_owned(),
        ));
    }
    if pieces.len() > MAX_ASSIGNMENTS {
        return Err(CasIntentError::BadArity);
    }
    let mut assignments = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let piece = piece.trim();
        let Some((name, value)) = piece.split_once('=') else {
            return Err(CasIntentError::Unknown(format!(
                "expected a `name=value` assignment, found `{piece}`"
            )));
        };
        let name = name.trim();
        validate_variable(name)?;
        let value = value.trim();
        if value.is_empty() {
            return Err(CasIntentError::MissingExpression);
        }
        assignments.push((name.to_owned(), value.to_owned()));
    }
    Ok(assignments)
}

fn keyword_rest<'a>(input: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = input.strip_prefix(keyword)?;
    match rest.chars().next() {
        Some(ch) if ch.is_ascii_alphanumeric() || ch == '_' => None,
        _ => Some(rest),
    }
}

fn take_identifier(input: &str) -> Option<(&str, &str)> {
    let mut chars = input.char_indices();
    let (_, first) = chars.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let mut end = input.len();
    for (index, ch) in chars {
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            end = index;
            break;
        }
    }
    Some((&input[..end], &input[end..]))
}

fn validate_variable(token: &str) -> Result<(), CasIntentError> {
    let token = token.trim();
    let mut chars = token.chars();
    let valid = matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if valid {
        Ok(())
    } else {
        Err(CasIntentError::BadVariable(token.to_owned()))
    }
}

fn call_args<'a>(after_keyword: &'a str, name: &str) -> Result<Vec<&'a str>, CasIntentError> {
    let rest = after_keyword.trim_start();
    if !rest.starts_with('(') {
        return Err(CasIntentError::Unknown(format!(
            "`{name}` requires parenthesised arguments"
        )));
    }
    let Some(close) = matching_close(rest, '(', ')') else {
        return Err(CasIntentError::UnbalancedDelimiters);
    };
    let trailing = rest[close + 1..].trim();
    if !trailing.is_empty() {
        return Err(CasIntentError::Unknown(format!(
            "unexpected trailing input `{trailing}` after `{name}(...)`"
        )));
    }
    Ok(split_top_level(&rest[1..close]))
}

fn matching_close(input: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in input.char_indices() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn strip_outer_parens(input: &str) -> &str {
    let trimmed = input.trim();
    if trimmed.starts_with('(')
        && let Some(close) = matching_close(trimmed, '(', ')')
        && close + 1 == trimmed.len()
    {
        return trimmed[1..close].trim();
    }
    trimmed
}

fn split_top_level(input: &str) -> Vec<&str> {
    if input.trim().is_empty() {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (index, ch) in input.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&input[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}
