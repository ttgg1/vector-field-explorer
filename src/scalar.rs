//! Static right-hand-side scalar expressions, separate from vector fields.

#[derive(Clone, Copy, Debug)]
pub enum ScalarContext {
    Curve,
    Surface,
}

#[derive(Clone, Debug)]
pub struct ScalarExpr {
    expr: meval::Expr,
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
        match context {
            ScalarContext::Curve => expr.clone().bind("x").map(|_| ()),
            ScalarContext::Surface => expr.clone().bind2("x", "y").map(|_| ()),
        }
        .map_err(error)?;
        Ok(Self { expr })
    }

    /// Return only finite f64 heights; GPU representability is checked by sampling.
    pub fn eval(&self, x: f64, y: f64) -> Option<f64> {
        let value = self
            .expr
            .eval_with_context(([("x", x), ("y", y)], meval::Context::new()))
            .ok()?;
        value.is_finite().then_some(value)
    }
}
