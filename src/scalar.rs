//! Right-hand-side scalar expressions, separate from vector fields.

#[derive(Clone, Copy, Debug)]
pub enum ScalarContext {
    Curve,
    Surface,
}

#[derive(Clone, Debug)]
pub struct ScalarExpr {
    expr: meval::Expr,
    time_dependent: bool,
}

#[derive(Debug)]
pub struct ScalarError {
    source: String,
    context: ScalarContext,
    cause: meval::Error,
}

impl std::fmt::Display for ScalarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let allowed = match self.context {
            ScalarContext::Curve => "x",
            ScalarContext::Surface => "x, y",
        };
        write!(
            f,
            "scalar parse error in `{}`: {}; allowed variables: {allowed}",
            self.source, self.cause
        )
    }
}

impl std::error::Error for ScalarError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

impl ScalarExpr {
    /// Validate syntax and bindings without evaluating an arbitrary finite point.
    pub fn parse(source: &str, context: ScalarContext) -> Result<Self, ScalarError> {
        let error = |cause| ScalarError {
            source: source.to_owned(),
            context,
            cause,
        };
        let expr = source.parse::<meval::Expr>().map_err(error)?;
        // bind checks variables/functions/arity, but does not evaluate the RHS.
        // Try the static binding first: an expression that binds without `t` is
        // static, while an expression such as `sin(t)*x` fails here and then
        // succeeds once `t` is allowed.
        let static_bind = match context {
            ScalarContext::Curve => expr.clone().bind("x").map(|_| ()),
            ScalarContext::Surface => expr.clone().bind2("x", "y").map(|_| ()),
        };
        if static_bind.is_ok() {
            return Ok(Self {
                expr,
                time_dependent: false,
            });
        }
        let static_error = static_bind.expect_err("checked immediately above");
        let time_bind = match context {
            ScalarContext::Curve => expr.clone().bind2("x", "t").map(|_| ()),
            ScalarContext::Surface => expr.clone().bind3("x", "y", "t").map(|_| ()),
        };
        // Report the original static error so a curve still says only `x` is
        // allowed rather than advertising `t` for an unrelated bad source.
        time_bind.map_err(|_| error(static_error))?;
        Ok(Self {
            expr,
            time_dependent: true,
        })
    }

    pub fn is_time_dependent(&self) -> bool {
        self.time_dependent
    }

    /// Compatibility evaluation at `t = 0`.
    pub fn eval(&self, x: f64, y: f64) -> Option<f64> {
        self.eval_at(x, y, 0.0)
    }

    /// Return only finite f64 heights at the supplied simulated time.
    pub fn eval_at(&self, x: f64, y: f64, t: f64) -> Option<f64> {
        let value = self
            .expr
            .eval_with_context(([("x", x), ("y", y), ("t", t)], meval::Context::new()))
            .ok()?;
        value.is_finite().then_some(value)
    }
}
