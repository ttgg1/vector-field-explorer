//! Native CAS backend bridge (#167).
//!
//! One [`CasBackend`] seam over the optional external CAS binaries (`qalc`,
//! `sage`, `sympy`) and the built-in reduced evaluator in [`crate::cas`].
//!
//! # Safety and bounds
//!
//! External backends are launched with an explicit argv (never a shell), the
//! request expression arrives on stdin, and the child runs in a fresh temporary
//! working directory with a cleared environment (`PATH`, `LANG=C`, and the
//! parent `HOME` when set). Every request is clamped to a fixed 5 s wall-clock
//! timeout with kill, 64 KiB stdout, 1 KiB stderr and 512 stdout lines. The
//! child is killed on timeout, oversize output and cancellation; the temporary
//! directory is removed on every return path. Reader threads hand their bounded
//! buffer back through a channel with a timeout, so a surviving grandchild that
//! keeps a pipe open cannot hang the request.
//!
//! # Native protocol
//!
//! `qalc` receives only the expression on stdin (`qalc -t -c 0`).
//!
//! `sage` and `sympy` run a fixed program constant with `-c`; the program reads
//! a bounded line-oriented payload from stdin:
//!
//! ```text
//! CAS1
//! op <token>
//! var <variable>
//! point <x,y,z|->
//! bounds <a,b|->
//! <expression>
//! ```
//!
//! No user data is ever interpolated into argv, and the expression is a single
//! validated line.
//!
//! # wasm
//!
//! wasm compiles the types, capability reporting and the built-in fallback
//! only. Process code is gated to non-wasm targets, and the symbolic engine in
//! [`crate::cas`] is itself native-only, so wasm reports the reduced fallback
//! with no capabilities rather than pretending a backend exists.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The largest accepted request expression, in UTF-8 bytes.
pub const MAX_EXPRESSION_BYTES: usize = 512;
/// The largest accepted variable name, in UTF-8 bytes.
pub const MAX_VARIABLE_BYTES: usize = 32;
/// The largest accepted result, in UTF-8 bytes.
pub const MAX_RESULT_BYTES: usize = 4096;
/// The stdout cap for one request, in bytes.
pub const MAX_STDOUT_BYTES: usize = 64 * 1024;
/// The stderr cap for one request, in bytes.
pub const MAX_STDERR_BYTES: usize = 1024;
/// The stdout line cap for one request.
pub const MAX_LINES: usize = 512;
/// The fixed external-request timeout.
pub const REQUEST_TIMEOUT_SECONDS: u64 = 5;
/// The fixed detection-probe timeout.
pub const PROBE_TIMEOUT_SECONDS: u64 = 2;

/// One operation a CAS backend may support.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CasCapability {
    Differentiate,
    DifferentiateN,
    Gradient,
    Divergence,
    Curl,
    Integrate,
    Solve,
    Simplify,
    Evaluate,
    Limits,
}

impl CasCapability {
    /// A short human-readable name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Differentiate => "differentiate",
            Self::DifferentiateN => "differentiate (higher order)",
            Self::Gradient => "gradient",
            Self::Divergence => "divergence",
            Self::Curl => "curl",
            Self::Integrate => "integrate",
            Self::Solve => "solve",
            Self::Simplify => "simplify",
            Self::Evaluate => "evaluate",
            Self::Limits => "limits",
        }
    }
}

/// The operation carried by a [`CasRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CasBackendOp {
    Differentiate,
    DifferentiateN { order: u32 },
    Gradient { dimension: usize },
    Divergence { dimension: usize },
    Curl { dimension: usize },
    Integrate { bounds: Option<(String, String)> },
    Solve,
    Simplify,
    Evaluate,
    Limits,
}

impl CasBackendOp {
    /// The capability this operation needs from a backend.
    pub fn capability(&self) -> CasCapability {
        match self {
            Self::Differentiate => CasCapability::Differentiate,
            Self::DifferentiateN { .. } => CasCapability::DifferentiateN,
            Self::Gradient { .. } => CasCapability::Gradient,
            Self::Divergence { .. } => CasCapability::Divergence,
            Self::Curl { .. } => CasCapability::Curl,
            Self::Integrate { .. } => CasCapability::Integrate,
            Self::Solve => CasCapability::Solve,
            Self::Simplify => CasCapability::Simplify,
            Self::Evaluate => CasCapability::Evaluate,
            Self::Limits => CasCapability::Limits,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn token(&self) -> String {
        match self {
            Self::Differentiate => "differentiate".into(),
            Self::DifferentiateN { order } => format!("differentiate_n:{order}"),
            Self::Gradient { dimension } => format!("gradient:{dimension}"),
            Self::Divergence { dimension } => format!("divergence:{dimension}"),
            Self::Curl { dimension } => format!("curl:{dimension}"),
            Self::Integrate { .. } => "integrate".into(),
            Self::Solve => "solve".into(),
            Self::Simplify => "simplify".into(),
            Self::Evaluate => "evaluate".into(),
            Self::Limits => "limits".into(),
        }
    }
}

/// One request handed to a [`CasBackend`].
#[derive(Clone, Debug, PartialEq)]
pub struct CasRequest {
    pub operation: CasBackendOp,
    /// The expression text; at most [`MAX_EXPRESSION_BYTES`], one line.
    pub expression: String,
    /// One `[A-Za-z][A-Za-z0-9_]*` name, at most [`MAX_VARIABLE_BYTES`].
    pub variable: String,
    /// The evaluation point for [`CasBackendOp::Evaluate`], if any.
    pub point: Option<(f64, f64, f64)>,
}

impl CasRequest {
    /// Validate every field before any backend work or process spawn.
    pub fn validate(&self) -> Result<(), CasBackendError> {
        if self.expression.trim().is_empty() {
            return Err(CasBackendError::InvalidRequest(
                "the expression is empty".into(),
            ));
        }
        if self.expression.len() > MAX_EXPRESSION_BYTES {
            return Err(CasBackendError::InvalidRequest(format!(
                "the expression exceeds {MAX_EXPRESSION_BYTES} bytes"
            )));
        }
        if self.expression.chars().any(char::is_control) {
            return Err(CasBackendError::InvalidRequest(
                "the expression contains control characters".into(),
            ));
        }
        if self.variable.len() > MAX_VARIABLE_BYTES || !valid_identifier(&self.variable) {
            return Err(CasBackendError::InvalidRequest(format!(
                "`{}` is not one identifier matching [A-Za-z][A-Za-z0-9_]*",
                sanitize(&self.variable, 32)
            )));
        }
        if let Some((x, y, z)) = self.point
            && !(x.is_finite() && y.is_finite() && z.is_finite())
        {
            return Err(CasBackendError::InvalidRequest(
                "the evaluation point must be finite".into(),
            ));
        }
        match &self.operation {
            CasBackendOp::Gradient { dimension }
            | CasBackendOp::Divergence { dimension }
            | CasBackendOp::Curl { dimension } => {
                if !matches!(dimension, 2 | 3) {
                    return Err(CasBackendError::InvalidRequest(
                        "vector operations require dimension 2 or 3".into(),
                    ));
                }
            }
            CasBackendOp::DifferentiateN { order } => {
                if *order == 0 || *order > 100 {
                    return Err(CasBackendError::InvalidRequest(
                        "the derivative order must be between 1 and 100".into(),
                    ));
                }
            }
            CasBackendOp::Integrate {
                bounds: Some((lower, upper)),
            } if lower.trim().is_empty()
                || upper.trim().is_empty()
                || lower.len() > MAX_EXPRESSION_BYTES
                || upper.len() > MAX_EXPRESSION_BYTES
                || lower.chars().any(char::is_control)
                || upper.chars().any(char::is_control) =>
            {
                return Err(CasBackendError::InvalidRequest(
                    "integration bounds must be two non-empty single-line strings".into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }
}

fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn sanitize(text: &str, max_chars: usize) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_owned();
    }
    let mut out: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A backend family for detection and persisted selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CasBackendKind {
    #[default]
    Qalc,
    Sage,
    Sympy,
    Builtin,
}

impl CasBackendKind {
    /// Every backend family, including the built-in fallback.
    pub const ALL: [Self; 4] = [Self::Qalc, Self::Sage, Self::Sympy, Self::Builtin];

    /// The documented external detection preference order.
    pub fn detect_order() -> [Self; 3] {
        [Self::Qalc, Self::Sage, Self::Sympy]
    }

    /// A short human-readable name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Qalc => "qalc",
            Self::Sage => "sage",
            Self::Sympy => "sympy",
            Self::Builtin => "built-in",
        }
    }
}

/// What one backend reports about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CasCapabilities {
    pub kind: CasBackendKind,
    pub capabilities: Vec<CasCapability>,
    /// Why the report is reduced or unverified, when it is.
    pub reason: Option<String>,
}

/// One CAS backend.
pub trait CasBackend: Send + Sync {
    fn capabilities(&self) -> &CasCapabilities;
    fn evaluate(&self, request: &CasRequest) -> Result<String, CasBackendError>;

    /// Internal cancellation hook used by `CasRunner`; normal callers use
    /// [`CasBackend::evaluate`].
    #[cfg(not(target_arch = "wasm32"))]
    #[doc(hidden)]
    fn cancel_token(&self) -> Option<CancelToken> {
        None
    }
}

/// Why a backend request failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CasBackendError {
    /// The selected backend does not claim the requested capability.
    Unsupported(CasCapability),
    /// Another request is already in flight in this runner.
    Busy,
    /// The request was abandoned by a cancel request.
    Cancelled,
    /// The child exceeded the fixed wall-clock limit and was killed.
    Timeout { seconds: u64 },
    /// The child exceeded a bounded stdout/stderr/line limit and was killed.
    OutputTooLarge { limit: usize },
    /// The child exited non-zero; stderr is bounded to 1 KiB.
    NonZero { status: Option<i32>, stderr: String },
    /// The child produced unusable output; the detail is sanitised and short.
    MalformedOutput(String),
    /// The backend or its executable is not usable.
    Unavailable(String),
    /// The request never reached a backend because validation failed.
    InvalidRequest(String),
}

impl fmt::Display for CasBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(capability) => {
                write!(f, "the backend does not support {}", capability.label())
            }
            Self::Busy => f.write_str("another CAS request is already running"),
            Self::Cancelled => f.write_str("the CAS request was cancelled"),
            Self::Timeout { seconds } => {
                write!(f, "the CAS request exceeded {seconds} s and was stopped")
            }
            Self::OutputTooLarge { limit } => {
                write!(f, "the CAS backend exceeded the {limit}-unit output limit")
            }
            Self::NonZero { status, stderr } => {
                match status {
                    Some(code) => write!(f, "the CAS backend exited with status {code}")?,
                    None => f.write_str("the CAS backend was terminated by a signal")?,
                }
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                Ok(())
            }
            Self::MalformedOutput(detail) => write!(f, "malformed CAS output: {detail}"),
            Self::Unavailable(detail) => write!(f, "CAS backend unavailable: {detail}"),
            Self::InvalidRequest(detail) => write!(f, "invalid CAS request: {detail}"),
        }
    }
}

impl std::error::Error for CasBackendError {}

/// The reduced built-in evaluator over [`crate::cas`].
///
/// Native builds support differentiation, gradient, divergence and curl; the
/// native-only `cas` module is why wasm reports no operations and returns
/// [`CasBackendError::Unavailable`] rather than faking a result.
pub struct BuiltinBackend {
    capabilities: CasCapabilities,
}

impl BuiltinBackend {
    /// Build the fallback. `reason` replaces the default reduced-evaluator
    /// explanation (for example, why an external backend was not found).
    pub fn new(reason: Option<String>) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let capabilities = vec![
            CasCapability::Differentiate,
            CasCapability::Gradient,
            CasCapability::Divergence,
            CasCapability::Curl,
        ];
        #[cfg(target_arch = "wasm32")]
        let capabilities = Vec::new();
        let reason = reason.unwrap_or_else(|| {
            if cfg!(target_arch = "wasm32") {
                "reduced built-in evaluator: this build has no symbolic backend".into()
            } else {
                "reduced built-in evaluator: differentiation, gradient, divergence and curl only"
                    .into()
            }
        });
        Self {
            capabilities: CasCapabilities {
                kind: CasBackendKind::Builtin,
                capabilities,
                reason: Some(reason),
            },
        }
    }
}

impl Default for BuiltinBackend {
    fn default() -> Self {
        Self::new(None)
    }
}

impl CasBackend for BuiltinBackend {
    fn capabilities(&self) -> &CasCapabilities {
        &self.capabilities
    }

    fn evaluate(&self, request: &CasRequest) -> Result<String, CasBackendError> {
        request.validate()?;
        // wasm advertises no capabilities, so it must report the real reason
        // (no compiled engine) rather than a misleading `Unsupported`.
        #[cfg(target_arch = "wasm32")]
        {
            let _ = request.operation.capability();
            return Err(CasBackendError::Unavailable(
                "the symbolic engine is not compiled for wasm".into(),
            ));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let capability = request.operation.capability();
            if !self.capabilities.capabilities.contains(&capability) {
                return Err(CasBackendError::Unsupported(capability));
            }
            builtin_evaluate(request)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn builtin_variable(name: &str) -> Result<crate::cas::Variable, CasBackendError> {
    match name {
        "x" => Ok(crate::cas::Variable::X),
        "y" => Ok(crate::cas::Variable::Y),
        "z" => Ok(crate::cas::Variable::Z),
        "t" => Ok(crate::cas::Variable::T),
        other => Err(CasBackendError::InvalidRequest(format!(
            "the built-in evaluator only understands x, y, z and t, not `{}`",
            sanitize(other, 32)
        ))),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn builtin_components(expression: &str, dimension: usize) -> Result<Vec<&str>, CasBackendError> {
    let components: Vec<&str> = expression.split(',').map(str::trim).collect();
    if components.len() != dimension || components.iter().any(|part| part.is_empty()) {
        return Err(CasBackendError::InvalidRequest(format!(
            "expected {dimension} comma-separated components, got {}",
            components.len()
        )));
    }
    Ok(components)
}

#[cfg(not(target_arch = "wasm32"))]
fn builtin_evaluate(request: &CasRequest) -> Result<String, CasBackendError> {
    use crate::cas;

    let expression = request.expression.as_str();
    let variable = builtin_variable(&request.variable)?;
    match &request.operation {
        CasBackendOp::Differentiate => cas::differentiate(expression, variable).map_err(cas_error),
        CasBackendOp::Gradient { dimension } => cas::gradient(expression, *dimension)
            .map(|parts| parts.join(", "))
            .map_err(cas_error),
        CasBackendOp::Divergence { dimension } => {
            let components = builtin_components(expression, *dimension)?;
            cas::divergence(&components, *dimension).map_err(cas_error)
        }
        CasBackendOp::Curl { dimension } => {
            let components = builtin_components(expression, *dimension)?;
            cas::curl(&components, *dimension)
                .map(|parts| parts.join(", "))
                .map_err(cas_error)
        }
        other => Err(CasBackendError::Unsupported(other.capability())),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn cas_error(error: crate::cas::CasError) -> CasBackendError {
    CasBackendError::Unavailable(error.to_string())
}

/// Resolve a backend kind to a ready backend, falling back to the built-in
/// evaluator (with an honest reason) when the external binary is missing.
pub fn select_backend(
    kind: CasBackendKind,
    path_env: Option<&std::ffi::OsStr>,
) -> Box<dyn CasBackend> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (kind, path_env);
        return Box::new(BuiltinBackend::new(Some(
            "this build has no external CAS backends; using the built-in evaluator".into(),
        )));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if kind == CasBackendKind::Builtin {
            return Box::new(BuiltinBackend::new(None));
        }
        match native::resolve(kind, path_env) {
            Ok((program, capabilities)) => {
                Box::new(native::NativeBackend::new(kind, program, capabilities))
            }
            Err(reason) => Box::new(BuiltinBackend::new(Some(format!(
                "{reason}; using the built-in evaluator"
            )))),
        }
    }
}

/// Probe the documented external backends through `PATH` only.
///
/// Missing binaries are omitted. A binary that exists but fails its version
/// probe is still listed, with the reduced/unverified reason recorded. `sympy`
/// needs a working `python3` with the `sympy` module, so it is omitted when
/// that probe fails.
pub fn detect_candidates(path_env: Option<&std::ffi::OsStr>) -> Vec<CasCapabilities> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path_env;
        Vec::new()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::detect(path_env)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::sync::{Arc, Mutex, MutexGuard};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    use super::{
        CasBackend, CasBackendError, CasBackendKind, CasBackendOp, CasCapabilities, CasCapability,
        CasRequest, MAX_LINES, MAX_RESULT_BYTES, MAX_STDERR_BYTES, MAX_STDOUT_BYTES,
        PROBE_TIMEOUT_SECONDS, REQUEST_TIMEOUT_SECONDS,
    };

    const POLL_INTERVAL: Duration = Duration::from_millis(50);
    /// How long a reader may take to hand back its buffer after the direct
    /// child exits. A surviving grandchild can hold a pipe open forever, so
    /// this bounds the handoff instead of joining the reader unboundedly.
    const READER_GRACE: Duration = Duration::from_millis(500);
    /// How long `CasRunner::cancel`/`Drop` wait for the worker to stop before
    /// detaching it.
    const WORKER_STOP_TIMEOUT: Duration = Duration::from_secs(1);

    const QALC_ARGS: [&str; 3] = ["-t", "-c", "0"];
    const SAGE_ARGS: [&str; 1] = ["--version"];
    const PYTHON_PROBE: [&str; 2] = ["-c", "import sympy; print(sympy.__version__)"];

    // Fixed program text: it contains no user data. It reads the documented
    // `CAS1` payload from stdin and prints exactly one line.
    pub(super) const SYMPY_PROGRAM: &str = r#"
import sys
from sympy import symbols, sympify, diff, integrate, solve, simplify, limit
data = sys.stdin.read().split("\n")
if len(data) < 6 or data[0] != "CAS1":
    print("malformed CAS1 request", file=sys.stderr)
    sys.exit(2)
fields = dict(line.split(" ", 1) for line in data[1:5])
x, y, z, t = symbols("x y z t")
env = {"x": x, "y": y, "z": z, "t": t}
expr = sympify(data[5], locals=env)
op = fields.get("op", "")
var_name = fields.get("var", "x")
var = env.get(var_name, x)
point = fields.get("point", "-")
bounds = fields.get("bounds", "-")
if op == "differentiate":
    result = diff(expr, var)
elif op.startswith("differentiate_n:"):
    result = diff(expr, var, int(op.split(":", 1)[1]))
elif op.startswith("gradient"):
    result = [diff(expr, env[v]) for v in ("x", "y", "z")]
elif op == "integrate":
    if bounds == "-":
        result = integrate(expr, var)
    else:
        lower, upper = bounds.split(",", 1)
        result = integrate(
            expr, (var, sympify(lower, locals=env), sympify(upper, locals=env))
        )
elif op == "solve":
    result = solve(expr, var)
elif op == "simplify":
    result = simplify(expr)
elif op == "evaluate":
    if point == "-":
        result = simplify(expr)
    else:
        coords = [sympify(part, locals=env) for part in point.split(",")]
        if len(coords) != 3:
            print("malformed point", file=sys.stderr)
            sys.exit(2)
        substitutions = {
            env[name]: coords[index]
            for index, name in enumerate(("x", "y", "z"))
        }
        result = simplify(expr.subs(substitutions)).evalf()
elif op == "limits":
    result = limit(expr, var, 0)
else:
    print("unsupported operation", file=sys.stderr)
    sys.exit(2)
print(result)
"#;

    pub(super) const SAGE_PROGRAM: &str = r#"
import sys
from sage.all import *
data = sys.stdin.read().split("\n")
if len(data) < 6 or data[0] != "CAS1":
    print("malformed CAS1 request", file=sys.stderr)
    sys.exit(2)
fields = dict(line.split(" ", 1) for line in data[1:5])
x, y, z, t = var("x y z t")
env = {"x": x, "y": y, "z": z, "t": t}
expr = sage_eval(data[5], locals=env)
op = fields.get("op", "")
var_name = fields.get("var", "x")
var = env.get(var_name, x)
point = fields.get("point", "-")
bounds = fields.get("bounds", "-")
if op == "differentiate":
    result = diff(expr, var)
elif op.startswith("differentiate_n:"):
    result = diff(expr, var, int(op.split(":", 1)[1]))
elif op.startswith("gradient"):
    result = [diff(expr, env[v]) for v in ("x", "y", "z")]
elif op == "integrate":
    if bounds == "-":
        result = integrate(expr, var)
    else:
        lower, upper = bounds.split(",", 1)
        result = integrate(
            expr, var, sage_eval(lower, locals=env), sage_eval(upper, locals=env)
        )
elif op == "solve":
    result = solve(expr == 0, var)
elif op == "simplify":
    result = simplify(expr)
elif op == "evaluate":
    if point == "-":
        result = simplify(expr)
    else:
        coords = [sage_eval(part, locals=env) for part in point.split(",")]
        if len(coords) != 3:
            print("malformed point", file=sys.stderr)
            sys.exit(2)
        substitutions = {
            env[name]: coords[index]
            for index, name in enumerate(("x", "y", "z"))
        }
        result = expr.subs(substitutions).simplify().n()
elif op == "limits":
    result = limit(expr, var, 0)
else:
    print("unsupported operation", file=sys.stderr)
    sys.exit(2)
print(result)
"#;

    fn declared(kind: CasBackendKind) -> Vec<CasCapability> {
        match kind {
            CasBackendKind::Qalc => vec![CasCapability::Evaluate, CasCapability::Simplify],
            CasBackendKind::Sage | CasBackendKind::Sympy => vec![
                CasCapability::Differentiate,
                CasCapability::DifferentiateN,
                CasCapability::Gradient,
                CasCapability::Divergence,
                CasCapability::Curl,
                CasCapability::Integrate,
                CasCapability::Solve,
                CasCapability::Simplify,
                CasCapability::Evaluate,
                CasCapability::Limits,
            ],
            CasBackendKind::Builtin => Vec::new(),
        }
    }

    fn declared_capabilities(kind: CasBackendKind, reason: Option<String>) -> CasCapabilities {
        CasCapabilities {
            kind,
            capabilities: declared(kind),
            reason,
        }
    }

    pub(super) struct NativeBackend {
        kind: CasBackendKind,
        program: PathBuf,
        capabilities: CasCapabilities,
        token: CancelToken,
    }

    impl NativeBackend {
        pub(super) fn new(
            kind: CasBackendKind,
            program: PathBuf,
            capabilities: CasCapabilities,
        ) -> Self {
            Self {
                kind,
                program,
                capabilities,
                token: CancelToken::new(),
            }
        }

        fn invocation(&self, request: &CasRequest) -> (Vec<&'static str>, String) {
            match self.kind {
                CasBackendKind::Qalc => (QALC_ARGS.to_vec(), request.expression.clone()),
                CasBackendKind::Sage => (vec!["-c", SAGE_PROGRAM], payload(request)),
                CasBackendKind::Sympy => (vec!["-c", SYMPY_PROGRAM], payload(request)),
                CasBackendKind::Builtin => unreachable!("built-in is not a native backend"),
            }
        }
    }

    impl CasBackend for NativeBackend {
        fn capabilities(&self) -> &CasCapabilities {
            &self.capabilities
        }

        fn evaluate(&self, request: &CasRequest) -> Result<String, CasBackendError> {
            request.validate()?;
            let capability = request.operation.capability();
            if !self.capabilities.capabilities.contains(&capability) {
                return Err(CasBackendError::Unsupported(capability));
            }
            let (args, stdin) = self.invocation(request);
            let output = run_process(
                &self.program,
                &args,
                &stdin,
                REQUEST_LIMITS,
                std::env::var_os("PATH").as_deref(),
                &self.token,
            )?;
            if !output.status.success() {
                return Err(CasBackendError::NonZero {
                    status: output.status.code(),
                    stderr: output.stderr,
                });
            }
            validate_result(&output.stdout)
        }

        fn cancel_token(&self) -> Option<CancelToken> {
            Some(self.token.clone())
        }
    }

    fn payload(request: &CasRequest) -> String {
        let point = request
            .point
            .map(|(x, y, z)| format!("{x},{y},{z}"))
            .unwrap_or_else(|| "-".into());
        let bounds = match &request.operation {
            CasBackendOp::Integrate {
                bounds: Some((lower, upper)),
            } => format!("{lower},{upper}"),
            _ => "-".into(),
        };
        format!(
            "CAS1\nop {}\nvar {}\npoint {}\nbounds {}\n{}\n",
            request.operation.token(),
            request.variable,
            point,
            bounds,
            request.expression
        )
    }

    fn validate_result(raw: &str) -> Result<String, CasBackendError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(CasBackendError::MalformedOutput(
                "the backend produced no output".into(),
            ));
        }
        if trimmed.len() > MAX_RESULT_BYTES {
            return Err(CasBackendError::MalformedOutput(format!(
                "the result exceeds {MAX_RESULT_BYTES} bytes"
            )));
        }
        if trimmed.contains('\n') || trimmed.contains('\r') {
            return Err(CasBackendError::MalformedOutput(
                "the backend produced more than one line".into(),
            ));
        }
        if trimmed.chars().any(char::is_control) {
            return Err(CasBackendError::MalformedOutput(format!(
                "unexpected control characters: {}",
                super::sanitize(trimmed, 128)
            )));
        }
        Ok(trimmed.to_owned())
    }

    /// Resolve the executable for `kind`, probing it so a candidate whose
    /// version probe fails is never selected. The error is the honest reason
    /// the caller falls back to the built-in evaluator.
    pub(super) fn resolve(
        kind: CasBackendKind,
        path_env: Option<&OsStr>,
    ) -> Result<(PathBuf, CasCapabilities), String> {
        match kind {
            CasBackendKind::Qalc => {
                let program = find_in_path("qalc", path_env)
                    .ok_or_else(|| "qalc was not found".to_owned())?;
                probe(&program, &["-v"], path_env).map_err(|error| {
                    format!("qalc was found but its version probe failed: {error}")
                })?;
                Ok((program, declared_capabilities(kind, None)))
            }
            CasBackendKind::Sage => {
                let program = find_in_path("sage", path_env)
                    .ok_or_else(|| "sage was not found".to_owned())?;
                probe(&program, &SAGE_ARGS, path_env).map_err(|error| {
                    format!("sage was found but its version probe failed: {error}")
                })?;
                Ok((program, declared_capabilities(kind, None)))
            }
            CasBackendKind::Sympy => {
                let python = find_in_path("python3", path_env)
                    .ok_or_else(|| "python3 was not found".to_owned())?;
                probe(&python, &PYTHON_PROBE, path_env).map_err(|error| {
                    format!("python3 was found but its sympy probe failed: {error}")
                })?;
                Ok((python, declared_capabilities(kind, None)))
            }
            CasBackendKind::Builtin => {
                Err("the built-in evaluator needs no external executable".into())
            }
        }
    }

    pub(super) fn detect(path_env: Option<&OsStr>) -> Vec<CasCapabilities> {
        let mut candidates = Vec::new();
        for kind in CasBackendKind::detect_order() {
            let program = match kind {
                CasBackendKind::Qalc => find_in_path("qalc", path_env),
                CasBackendKind::Sage => find_in_path("sage", path_env),
                CasBackendKind::Sympy => find_in_path("python3", path_env),
                CasBackendKind::Builtin => None,
            };
            let Some(program) = program else {
                continue;
            };
            let probe_result = match kind {
                CasBackendKind::Qalc => probe(&program, &["-v"], path_env),
                CasBackendKind::Sage => probe(&program, &SAGE_ARGS, path_env),
                CasBackendKind::Sympy => probe(&program, &PYTHON_PROBE, path_env),
                CasBackendKind::Builtin => Ok(()),
            };
            match (kind, probe_result) {
                // Without a working python3+sympy there is no sympy backend.
                (CasBackendKind::Sympy, Err(_)) => {}
                (kind, Ok(())) => candidates.push(declared_capabilities(kind, None)),
                (kind, Err(error)) => candidates.push(CasCapabilities {
                    kind,
                    // A failed probe leaves the candidate listed so the user
                    // can see why, but it claims nothing and is never selected.
                    capabilities: Vec::new(),
                    reason: Some(format!(
                        "version probe failed; no capabilities are claimed: {}",
                        super::sanitize(&error, 160)
                    )),
                }),
            }
        }
        candidates
    }

    fn probe(program: &Path, args: &[&str], path_env: Option<&OsStr>) -> Result<(), String> {
        let output = run_process(
            program,
            args,
            "",
            PROBE_LIMITS,
            path_env,
            &CancelToken::new(),
        )
        .map_err(|error| error.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "exit status {:?}: {}",
                output.status.code(),
                output.stderr
            ))
        }
    }

    fn find_in_path(name: &str, path_env: Option<&OsStr>) -> Option<PathBuf> {
        let path = match path_env {
            Some(path) => path.to_owned(),
            None => std::env::var_os("PATH")?,
        };
        for directory in std::env::split_paths(&path) {
            // An empty entry would mean the current directory; never resolve
            // executables from there.
            if directory.as_os_str().is_empty() {
                continue;
            }
            for candidate in executable_names(name) {
                let path = directory.join(candidate);
                if is_executable(&path) {
                    return Some(path);
                }
            }
        }
        None
    }

    #[cfg(unix)]
    fn executable_names(name: &str) -> Vec<String> {
        vec![name.to_owned()]
    }

    #[cfg(not(unix))]
    fn executable_names(name: &str) -> Vec<String> {
        vec![name.to_owned(), format!("{name}.exe")]
    }

    #[cfg(unix)]
    fn is_executable(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    fn is_executable(path: &Path) -> bool {
        fs::metadata(path)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
    }

    #[derive(Clone, Copy)]
    struct ProcessLimits {
        stdout: usize,
        stderr: usize,
        lines: usize,
        timeout: Duration,
    }

    const REQUEST_LIMITS: ProcessLimits = ProcessLimits {
        stdout: MAX_STDOUT_BYTES,
        stderr: MAX_STDERR_BYTES,
        lines: MAX_LINES,
        timeout: Duration::from_secs(REQUEST_TIMEOUT_SECONDS),
    };

    const PROBE_LIMITS: ProcessLimits = ProcessLimits {
        stdout: 1024,
        stderr: 1024,
        lines: 16,
        timeout: Duration::from_secs(PROBE_TIMEOUT_SECONDS),
    };

    struct ProcessOutput {
        status: ExitStatus,
        stdout: String,
        stderr: String,
    }

    #[derive(Default)]
    struct OutputFlags {
        stdout_bytes: AtomicBool,
        stdout_lines: AtomicBool,
        stderr_bytes: AtomicBool,
    }

    impl OutputFlags {
        fn overrun_limit(&self) -> Option<usize> {
            if self.stdout_bytes.load(Ordering::SeqCst) {
                Some(MAX_STDOUT_BYTES)
            } else if self.stdout_lines.load(Ordering::SeqCst) {
                Some(MAX_LINES)
            } else if self.stderr_bytes.load(Ordering::SeqCst) {
                Some(MAX_STDERR_BYTES)
            } else {
                None
            }
        }
    }

    enum Stream {
        Stdout,
        Stderr,
    }

    /// Test seam: exercise the bounded reader without spawning a process.
    pub(super) fn read_capped_for_test<R: Read>(reader: R, cap: usize) -> std::io::Result<Vec<u8>> {
        read_capped(reader, cap, None, &OutputFlags::default(), Stream::Stdout)
    }

    fn read_capped<R: Read>(
        mut reader: R,
        cap: usize,
        line_cap: Option<usize>,
        flags: &OutputFlags,
        stream: Stream,
    ) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::new();
        let mut lines = 0usize;
        let mut buffer = [0u8; 4096];
        loop {
            let read = match reader.read(&mut buffer) {
                // Only `Ok(0)` is a clean EOF; an `Err` must not be mistaken
                // for a successful, complete answer.
                Ok(0) => break,
                Ok(read) => read,
                Err(error) => return Err(error),
            };
            let chunk = &buffer[..read];
            if let Some(max_lines) = line_cap {
                lines += chunk.iter().filter(|byte| **byte == b'\n').count();
                if lines > max_lines {
                    flags.stdout_lines.store(true, Ordering::SeqCst);
                    return Ok(data);
                }
            }
            if data.len() + read > cap {
                match stream {
                    Stream::Stdout => flags.stdout_bytes.store(true, Ordering::SeqCst),
                    Stream::Stderr => flags.stderr_bytes.store(true, Ordering::SeqCst),
                }
                return Ok(data);
            }
            data.extend_from_slice(chunk);
        }
        Ok(data)
    }

    /// One request-control handle shared between the runner and the worker.
    ///
    /// It carries the cancel flag and the live child, so a cancel kills the
    /// process without waiting for the polling loop.
    #[derive(Clone, Default)]
    pub struct CancelToken {
        inner: Arc<CancelInner>,
    }

    #[derive(Default)]
    struct CancelInner {
        cancelled: AtomicBool,
        child: Mutex<Option<Child>>,
    }

    impl CancelToken {
        pub fn new() -> Self {
            Self::default()
        }

        /// Clear the flag and any stale child before a new request.
        pub fn reset(&self) {
            self.inner.cancelled.store(false, Ordering::SeqCst);
            *lock(&self.inner.child) = None;
        }

        /// Request cancellation and kill the live child, if any.
        pub fn cancel(&self) {
            self.inner.cancelled.store(true, Ordering::SeqCst);
            if let Some(child) = lock(&self.inner.child).as_mut() {
                let _ = child.kill();
            }
        }

        /// Whether cancellation was requested.
        pub fn is_cancelled(&self) -> bool {
            self.inner.cancelled.load(Ordering::SeqCst)
        }

        fn set_child(&self, child: Child) {
            *lock(&self.inner.child) = Some(child);
        }

        fn try_wait(&self) -> std::io::Result<Option<ExitStatus>> {
            match lock(&self.inner.child).as_mut() {
                Some(child) => child.try_wait(),
                None => Ok(None),
            }
        }

        fn take_child(&self) {
            let _ = lock(&self.inner.child).take();
        }

        fn kill_and_wait(&self) {
            if let Some(mut child) = lock(&self.inner.child).take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn run_process(
        program: &Path,
        args: &[&str],
        stdin: &str,
        limits: ProcessLimits,
        path_env: Option<&OsStr>,
        token: &CancelToken,
    ) -> Result<ProcessOutput, CasBackendError> {
        if token.is_cancelled() {
            return Err(CasBackendError::Cancelled);
        }
        let temp = TempDir::new()?;
        let path: OsString = path_env
            .map(OsStr::to_owned)
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();

        let home = std::env::var_os("HOME");
        let mut child = spawn_with_retry(program, args, temp.path(), &path, home.as_deref())?;

        if let Some(mut child_stdin) = child.stdin.take() {
            // A short expression fits well inside the pipe buffer; a broken
            // pipe means the child exited early and is reported by its status.
            let _ = child_stdin.write_all(stdin.as_bytes());
        }

        let flags = Arc::new(OutputFlags::default());
        let stdout = child.stdout.take().expect("stdout was requested as a pipe");
        let stderr = child.stderr.take().expect("stderr was requested as a pipe");
        // Readers hand their bounded buffer back over a channel. Dropping the
        // `JoinHandle` detaches a reader that is still blocked on a pipe held
        // open by a surviving grandchild; the worker never joins it directly.
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let _stdout_reader = thread::spawn({
            let flags = flags.clone();
            move || {
                let _ = stdout_tx.send(read_capped(
                    stdout,
                    limits.stdout,
                    Some(limits.lines),
                    &flags,
                    Stream::Stdout,
                ));
            }
        });
        let (stderr_tx, stderr_rx) = mpsc::channel();
        let _stderr_reader = thread::spawn({
            let flags = flags.clone();
            move || {
                let _ = stderr_tx.send(read_capped(
                    stderr,
                    limits.stderr,
                    None,
                    &flags,
                    Stream::Stderr,
                ));
            }
        });

        token.set_child(child);
        let deadline = Instant::now() + limits.timeout;
        let status = loop {
            if token.is_cancelled() {
                token.kill_and_wait();
                return Err(CasBackendError::Cancelled);
            }
            if let Some(limit) = flags.overrun_limit() {
                token.kill_and_wait();
                return Err(CasBackendError::OutputTooLarge { limit });
            }
            match token.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    token.kill_and_wait();
                    return Err(CasBackendError::Unavailable(format!(
                        "waiting for {} failed: {error}",
                        program.display()
                    )));
                }
            }
            if Instant::now() >= deadline {
                token.kill_and_wait();
                return Err(CasBackendError::Timeout {
                    seconds: limits.timeout.as_secs(),
                });
            }
            thread::sleep(POLL_INTERVAL);
        };
        token.take_child();

        // Wait for the readers no longer than the request deadline and, in
        // any case, no longer than the fixed post-exit grace. A timeout means
        // an inherited pipe is still open; report that honestly and let the
        // reader thread detach.
        let reader_deadline = deadline.min(Instant::now() + READER_GRACE);
        let stdout = receive_reader(&stdout_rx, reader_deadline, "stdout")?;
        let stderr = receive_reader(&stderr_rx, reader_deadline, "stderr")?;
        if let Some(limit) = flags.overrun_limit() {
            return Err(CasBackendError::OutputTooLarge { limit });
        }
        Ok(ProcessOutput {
            status,
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }

    fn receive_reader(
        receiver: &mpsc::Receiver<std::io::Result<Vec<u8>>>,
        deadline: Instant,
        stream: &str,
    ) -> Result<Vec<u8>, CasBackendError> {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(timeout) {
            Ok(Ok(data)) => Ok(data),
            Ok(Err(error)) => Err(CasBackendError::Unavailable(format!(
                "reading the CAS backend {stream} failed: {error}"
            ))),
            Err(RecvTimeoutError::Timeout) => Err(CasBackendError::Unavailable(format!(
                "the CAS backend {stream} stayed open after the child exited; the request was abandoned"
            ))),
            Err(RecvTimeoutError::Disconnected) => Err(CasBackendError::MalformedOutput(format!(
                "the CAS backend {stream} reader stopped without a result"
            ))),
        }
    }

    fn spawn_with_retry(
        program: &Path,
        args: &[&str],
        working_directory: &Path,
        path: &OsStr,
        home: Option<&OsStr>,
    ) -> Result<Child, CasBackendError> {
        // `ExecutableFileBusy` is a transient fork/exec race when another
        // thread still holds a write handle to a freshly replaced executable
        // (or a package manager is rewriting it). Retry briefly instead of
        // turning that into a hard failure.
        for attempt in 0..5 {
            let mut command = Command::new(program);
            command
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .current_dir(working_directory)
                .env_clear()
                .env("PATH", path)
                .env("LANG", "C");
            // Sage/python tooling may consult `HOME` for a config/cache
            // directory. Pass the parent's value unchanged; no other user
            // state is inherited.
            if let Some(home) = home {
                command.env("HOME", home);
            }
            match command.spawn() {
                Ok(child) => return Ok(child),
                Err(error)
                    if error.kind() == std::io::ErrorKind::ExecutableFileBusy && attempt < 4 =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => {
                    return Err(CasBackendError::Unavailable(format!(
                        "could not start {}: {error}",
                        program.display()
                    )));
                }
            }
        }
        Err(CasBackendError::Unavailable(format!(
            "could not start {} after retrying: the executable stayed busy",
            program.display()
        )))
    }

    /// A private working directory removed on every return path.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Result<Self, CasBackendError> {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let base = std::env::temp_dir();
            // A time-derived nonce makes the name unpredictable across runs;
            // the counter only separates same-nanosecond attempts.
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0);
            for _ in 0..16 {
                let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
                let path = base.join(format!("vfe-cas-{}-{nonce}-{serial}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self { path }),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => {
                        return Err(CasBackendError::Unavailable(format!(
                            "could not create the CAS temporary directory: {error}"
                        )));
                    }
                }
            }
            Err(CasBackendError::Unavailable(
                "could not create a unique CAS temporary directory".into(),
            ))
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// One in-flight request at a time, with cooperative cancellation.
    ///
    /// `start` returns [`CasBackendError::Busy`] while a request is live;
    /// `poll` returns the finished result (or `None`); `cancel` kills the child
    /// and waits for the worker only for a bounded time.
    pub struct CasRunner {
        backend: Arc<dyn CasBackend>,
        token: Option<CancelToken>,
        handle: Option<JoinHandle<Result<String, CasBackendError>>>,
    }

    impl CasRunner {
        pub fn new(backend: Box<dyn CasBackend>) -> Self {
            let backend: Arc<dyn CasBackend> = Arc::from(backend);
            let token = backend.cancel_token();
            Self {
                backend,
                token,
                handle: None,
            }
        }

        /// Start `request`, or return [`CasBackendError::Busy`] if one is live.
        pub fn start(&mut self, request: CasRequest) -> Result<(), CasBackendError> {
            if let Some(handle) = &self.handle
                && !handle.is_finished()
            {
                return Err(CasBackendError::Busy);
            }
            if let Some(finished) = self.handle.take() {
                let _ = finished.join();
            }
            if let Some(token) = &self.token {
                token.reset();
            }
            let backend = self.backend.clone();
            let handle = thread::Builder::new()
                .name("cas-request".into())
                .spawn(move || backend.evaluate(&request))
                .map_err(|error| {
                    CasBackendError::Unavailable(format!("could not start the CAS worker: {error}"))
                })?;
            self.handle = Some(handle);
            Ok(())
        }

        /// Join a finished request without blocking a running one.
        pub fn poll(&mut self) -> Option<Result<String, CasBackendError>> {
            let handle = self.handle.as_ref()?;
            if !handle.is_finished() {
                return None;
            }
            let handle = self.handle.take()?;
            Some(join_request(handle))
        }

        /// Cancel the live request and wait only for a bounded time.
        pub fn cancel(&mut self) -> Option<Result<String, CasBackendError>> {
            let handle = self.handle.take()?;
            if !handle.is_finished()
                && let Some(token) = &self.token
            {
                token.cancel();
            }
            Some(match join_request_within(handle, WORKER_STOP_TIMEOUT) {
                Some(result) => result,
                None => Err(CasBackendError::Unavailable(format!(
                    "the CAS worker did not stop within {} ms and was detached",
                    WORKER_STOP_TIMEOUT.as_millis()
                ))),
            })
        }

        /// Whether a request is still in flight.
        pub fn is_running(&self) -> bool {
            self.handle
                .as_ref()
                .is_some_and(|handle| !handle.is_finished())
        }
    }

    impl Drop for CasRunner {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                if !handle.is_finished()
                    && let Some(token) = &self.token
                {
                    token.cancel();
                }
                // Detach rather than block if the worker outlives the bound.
                let _ = join_request_within(handle, WORKER_STOP_TIMEOUT);
            }
        }
    }

    fn join_request_within(
        handle: JoinHandle<Result<String, CasBackendError>>,
        timeout: Duration,
    ) -> Option<Result<String, CasBackendError>> {
        let deadline = Instant::now() + timeout;
        while !handle.is_finished() {
            if Instant::now() >= deadline {
                // Dropping `handle` detaches the worker instead of waiting.
                return None;
            }
            thread::sleep(Duration::from_millis(10));
        }
        Some(join_request(handle))
    }

    fn join_request(
        handle: JoinHandle<Result<String, CasBackendError>>,
    ) -> Result<String, CasBackendError> {
        handle.join().unwrap_or_else(|_| {
            Err(CasBackendError::Unavailable(
                "the CAS worker panicked".into(),
            ))
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{CancelToken, CasRunner};

/// Test seam: exercise the bounded stdout reader without a child process.
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub fn read_capped_for_test<R: std::io::Read>(reader: R, cap: usize) -> std::io::Result<Vec<u8>> {
    native::read_capped_for_test(reader, cap)
}

/// Test seam: the fixed `sympy` program text, so the suite can pin its
/// `point`/`bounds` handling without a real sympy install.
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub const SYMPY_PROGRAM_FOR_TEST: &str = native::SYMPY_PROGRAM;

/// Test seam: the fixed `sage` program text.
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub const SAGE_PROGRAM_FOR_TEST: &str = native::SAGE_PROGRAM;
