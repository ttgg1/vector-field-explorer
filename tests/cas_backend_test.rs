//! #167 fixture tests for the native CAS backend bridge.
//!
//! Every backend is a short shell script written to a per-test temp directory
//! and reached through an explicit `PATH`; no real qalc/sage/sympy is required,
//! nothing touches the network, and no test mutates the process environment.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use vector_field_explorer::cas_backend::{
    BuiltinBackend, CasBackend, CasBackendError, CasBackendKind, CasBackendOp, CasCapability,
    CasRequest, CasRunner, MAX_EXPRESSION_BYTES, MAX_STDOUT_BYTES, SAGE_PROGRAM_FOR_TEST,
    SYMPY_PROGRAM_FOR_TEST, detect_candidates, read_capped_for_test, select_backend,
};

fn install(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fixture");
    let mut permissions = fs::metadata(&path).expect("fixture metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("mark fixture executable");
    path
}

fn fixture_dir(body: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    // The version probe must succeed, otherwise selection deliberately falls
    // back to the built-in evaluator (see the probe-failure test).
    install(
        dir.path(),
        "qalc",
        &format!("if [ \"$1\" = \"-v\" ]; then echo 'qalc 5.5'; exit 0; fi\n{body}"),
    );
    dir
}

fn request(operation: CasBackendOp) -> CasRequest {
    CasRequest {
        operation,
        expression: "2+2".into(),
        variable: "x".into(),
        point: None,
    }
}

fn qalc(path: &Path) -> Box<dyn CasBackend> {
    select_backend(CasBackendKind::Qalc, Some(path.as_os_str()))
}

#[test]
fn detect_reports_missing_binaries_honestly() {
    let candidates = detect_candidates(Some(OsStr::new("")));
    assert!(
        candidates.is_empty(),
        "an empty PATH must produce no CAS candidates, got {candidates:?}"
    );
}

#[test]
fn detect_keeps_documented_preference_order() {
    let dir = tempfile::tempdir().expect("temp dir");
    install(
        dir.path(),
        "qalc",
        "if [ \"$1\" = \"-v\" ]; then echo 'qalc 5.5'; exit 0; fi\ncat >/dev/null",
    );
    install(
        dir.path(),
        "sage",
        "if [ \"$1\" = \"--version\" ]; then echo 'SageMath 10.4'; exit 0; fi\ncat >/dev/null",
    );
    install(
        dir.path(),
        "python3",
        "if [ \"$1\" = \"-c\" ]; then echo '1.13.0'; exit 0; fi\ncat >/dev/null",
    );

    let kinds: Vec<CasBackendKind> = detect_candidates(Some(dir.path().as_os_str()))
        .iter()
        .map(|candidate| candidate.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            CasBackendKind::Qalc,
            CasBackendKind::Sage,
            CasBackendKind::Sympy
        ]
    );
}

#[test]
fn successful_request_returns_trimmed_single_line() {
    let dir = fixture_dir("cat >/dev/null\nprintf '  42  \\n'");
    let backend = qalc(dir.path());
    assert_eq!(backend.capabilities().kind, CasBackendKind::Qalc);
    assert!(
        backend
            .capabilities()
            .capabilities
            .contains(&CasCapability::Evaluate)
    );

    let output = backend.evaluate(&request(CasBackendOp::Evaluate)).unwrap();
    assert_eq!(output, "42");
}

#[test]
fn timeout_kills_the_child_and_reports_the_limit() {
    let dir = fixture_dir("cat >/dev/null\nsleep 30");
    let backend = qalc(dir.path());

    let started = Instant::now();
    let error = backend
        .evaluate(&request(CasBackendOp::Evaluate))
        .expect_err("the fixture never finishes");
    assert_eq!(error, CasBackendError::Timeout { seconds: 5 });
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "timeout must kill the child near the 5 s limit"
    );
}

#[test]
fn oversized_stdout_is_rejected_and_kills_the_child() {
    let dir = fixture_dir("cat >/dev/null\nawk 'BEGIN{for(i=0;i<70000;i++)printf \"a\"}'\nexit 0");
    let backend = qalc(dir.path());

    let error = backend
        .evaluate(&request(CasBackendOp::Evaluate))
        .expect_err("oversized output must be rejected");
    assert_eq!(
        error,
        CasBackendError::OutputTooLarge {
            limit: MAX_STDOUT_BYTES
        }
    );
}

#[test]
fn non_zero_exit_reports_bounded_stderr() {
    let dir = fixture_dir("cat >/dev/null\nprintf 'boom' >&2\nexit 3");
    let backend = qalc(dir.path());

    let error = backend
        .evaluate(&request(CasBackendOp::Evaluate))
        .expect_err("a failing fixture must be reported");
    assert_eq!(
        error.to_string(),
        "the CAS backend exited with status 3: boom",
        "Display must show the real exit code, not `Some(3)`"
    );
    match error {
        CasBackendError::NonZero { status, stderr } => {
            assert_eq!(status, Some(3));
            assert_eq!(stderr, "boom");
        }
        other => panic!("expected NonZero, got {other:?}"),
    }
}

#[test]
fn malformed_output_is_reported_without_panicking() {
    let empty_dir = fixture_dir("cat >/dev/null");
    let error = qalc(empty_dir.path())
        .evaluate(&request(CasBackendOp::Evaluate))
        .expect_err("empty output is malformed");
    assert!(matches!(error, CasBackendError::MalformedOutput(_)));

    let control_dir = fixture_dir("cat >/dev/null\nprintf 'a\\001b\\n'");
    let error = qalc(control_dir.path())
        .evaluate(&request(CasBackendOp::Evaluate))
        .expect_err("control characters are malformed");
    assert!(matches!(error, CasBackendError::MalformedOutput(_)));
}

#[test]
fn cancel_stops_an_in_flight_request() {
    let dir = fixture_dir("cat >/dev/null\nsleep 30");
    let mut runner = CasRunner::new(qalc(dir.path()));
    runner
        .start(request(CasBackendOp::Evaluate))
        .expect("start");

    thread::sleep(Duration::from_millis(150));
    let started = Instant::now();
    let result = runner.cancel().expect("a request was in flight");
    assert_eq!(result, Err(CasBackendError::Cancelled));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "cancel must kill the child promptly"
    );
}

#[test]
fn unavailable_selection_falls_back_to_builtin_with_a_reason() {
    let backend = select_backend(CasBackendKind::Qalc, Some(OsStr::new("")));
    let capabilities = backend.capabilities();
    assert_eq!(
        capabilities.kind,
        CasBackendKind::Builtin,
        "a missing qalc must fall back to the built-in evaluator"
    );
    let reason = capabilities
        .reason
        .as_deref()
        .expect("the fallback must explain itself");
    assert!(reason.contains("qalc was not found"), "{reason}");
    assert!(reason.contains("built-in"), "{reason}");

    assert_eq!(
        backend.evaluate(&request(CasBackendOp::Evaluate)),
        Err(CasBackendError::Unsupported(CasCapability::Evaluate))
    );
}

#[test]
fn invalid_requests_are_rejected_before_spawning() {
    let dir = tempfile::tempdir().expect("temp dir");
    let marker = dir.path().join("spawned");
    install(
        dir.path(),
        "qalc",
        &format!("touch '{}'\ncat >/dev/null", marker.display()),
    );
    let backend = qalc(dir.path());

    let mut long = request(CasBackendOp::Evaluate);
    long.expression = "1".repeat(MAX_EXPRESSION_BYTES + 1);
    assert!(matches!(
        backend.evaluate(&long),
        Err(CasBackendError::InvalidRequest(_))
    ));

    let mut bad_variable = request(CasBackendOp::Evaluate);
    bad_variable.variable = "x y".into();
    assert!(matches!(
        backend.evaluate(&bad_variable),
        Err(CasBackendError::InvalidRequest(_))
    ));

    assert!(
        !marker.exists(),
        "validation must reject the request before any process starts"
    );
}

#[test]
fn builtin_capabilities_are_honest() {
    let builtin = BuiltinBackend::new(None);
    let capabilities = builtin.capabilities();
    assert_eq!(capabilities.kind, CasBackendKind::Builtin);
    for capability in [
        CasCapability::Differentiate,
        CasCapability::Gradient,
        CasCapability::Divergence,
        CasCapability::Curl,
    ] {
        assert!(
            capabilities.capabilities.contains(&capability),
            "the built-in evaluator supports {capability:?}"
        );
    }
    for capability in [
        CasCapability::DifferentiateN,
        CasCapability::Integrate,
        CasCapability::Solve,
        CasCapability::Simplify,
        CasCapability::Evaluate,
        CasCapability::Limits,
    ] {
        assert!(
            !capabilities.capabilities.contains(&capability),
            "the built-in evaluator must not claim {capability:?}"
        );
    }

    let derivative = builtin
        .evaluate(&CasRequest {
            operation: CasBackendOp::Differentiate,
            expression: "x^2".into(),
            variable: "x".into(),
            point: None,
        })
        .expect("differentiate");
    let value = derivative
        .parse::<meval::Expr>()
        .expect("meval expression")
        .bind("x")
        .expect("bind x")(3.0);
    assert!((value - 6.0).abs() < 1e-9, "d/dx x^2 at 3 was {value}");

    let gradient = builtin
        .evaluate(&CasRequest {
            operation: CasBackendOp::Gradient { dimension: 2 },
            expression: "x^2+y^2".into(),
            variable: "x".into(),
            point: None,
        })
        .expect("gradient");
    assert_eq!(
        gradient.matches(',').count(),
        1,
        "two components: {gradient}"
    );

    let divergence = builtin
        .evaluate(&CasRequest {
            operation: CasBackendOp::Divergence { dimension: 2 },
            expression: "x,y".into(),
            variable: "x".into(),
            point: None,
        })
        .expect("divergence");
    assert!(!divergence.is_empty());

    assert_eq!(
        builtin.evaluate(&CasRequest {
            operation: CasBackendOp::Integrate { bounds: None },
            expression: "x".into(),
            variable: "x".into(),
            point: None,
        }),
        Err(CasBackendError::Unsupported(CasCapability::Integrate))
    );
    // `DifferentiateN` has no built-in capability, so the error path — not a
    // hidden implementation arm — must report it consistently.
    assert_eq!(
        builtin.evaluate(&CasRequest {
            operation: CasBackendOp::DifferentiateN { order: 2 },
            expression: "x^2".into(),
            variable: "x".into(),
            point: None,
        }),
        Err(CasBackendError::Unsupported(CasCapability::DifferentiateN))
    );
}

#[test]
fn one_in_flight_request_is_enforced() {
    let dir = fixture_dir("cat >/dev/null\nsleep 30");
    let mut runner = CasRunner::new(qalc(dir.path()));
    runner
        .start(request(CasBackendOp::Evaluate))
        .expect("start");
    assert_eq!(
        runner.start(request(CasBackendOp::Evaluate)),
        Err(CasBackendError::Busy)
    );
    runner.cancel();
}

#[test]
fn fallback_reason_is_reported_verbatim() {
    let builtin = BuiltinBackend::new(Some("wasm build".into()));
    assert_eq!(builtin.capabilities().reason.as_deref(), Some("wasm build"));
}

#[test]
fn kind_tokens_match_the_persisted_form() {
    for (kind, token) in [
        (CasBackendKind::Qalc, "qalc"),
        (CasBackendKind::Sage, "sage"),
        (CasBackendKind::Sympy, "sympy"),
        (CasBackendKind::Builtin, "builtin"),
    ] {
        let value = serde_json::to_value(kind).expect("serialize");
        assert_eq!(value, serde_json::json!(token));
        let parsed: CasBackendKind = serde_json::from_value(value).expect("deserialize");
        assert_eq!(parsed, kind);
    }
    assert_eq!(CasBackendKind::detect_order().len(), 3);
    assert_eq!(CasBackendKind::ALL.len(), 4);
    assert_eq!(CasBackendKind::default(), CasBackendKind::Qalc);
}

/// The direct child exits at once but a background grandchild keeps the stdout
/// write end open. The reader therefore never sees EOF.
const GRANDCHILD_FIXTURE: &str = "cat >/dev/null\nsleep 3 &\nprintf '42\\n'";

/// A stand-in for the fixed `sympy` program: it reports the probe version and
/// otherwise echoes the `CAS1` control fields it received on one line.
const SYMPY_FIELD_FIXTURE: &str = r#"if [ "$1" = "-c" ]; then
  if [ "$2" = "import sympy; print(sympy.__version__)" ]; then
    echo '1.13.0'
    exit 0
  fi
  op= point= bounds=
  while IFS= read -r line; do
    case "$line" in
      "op "*) op=${line#op } ;;
      "point "*) point=${line#point } ;;
      "bounds "*) bounds=${line#bounds } ;;
    esac
  done
  printf '%s|%s|%s\n' "$op" "$point" "$bounds"
  exit 0
fi
exit 2"#;

#[test]
fn evaluate_is_bounded_when_a_grandchild_holds_stdout_open() {
    let dir = fixture_dir(GRANDCHILD_FIXTURE);
    let backend = qalc(dir.path());

    let started = Instant::now();
    let result = backend.evaluate(&request(CasBackendOp::Evaluate));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "evaluate must not wait for a surviving grandchild's pipe to close"
    );
    match result {
        Err(CasBackendError::Unavailable(reason)) => {
            assert!(reason.contains("stdout"), "{reason}")
        }
        other => panic!("expected a bounded, honest error, got {other:?}"),
    }
}

#[test]
fn cancel_is_bounded_when_a_grandchild_holds_stdout_open() {
    let dir = fixture_dir(GRANDCHILD_FIXTURE);
    let mut runner = CasRunner::new(qalc(dir.path()));
    runner
        .start(request(CasBackendOp::Evaluate))
        .expect("start");
    // Let the direct child exit so the worker is waiting on the reader.
    thread::sleep(Duration::from_millis(150));

    let started = Instant::now();
    let result = runner.cancel().expect("a request was in flight");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "cancel must not join a worker blocked on a surviving grandchild"
    );
    // Cancellation may arrive after the child exited (so the honest outcome is
    // the bounded reader error) or before it (Cancelled); either is bounded.
    let bounded_error = matches!(
        &result,
        Err(CasBackendError::Cancelled | CasBackendError::Unavailable(_))
    );
    assert!(
        bounded_error,
        "expected a bounded, honest cancellation result, got {result:?}"
    );
}

#[test]
fn drop_is_bounded_when_a_grandchild_holds_stdout_open() {
    let dir = fixture_dir(GRANDCHILD_FIXTURE);
    let mut runner = CasRunner::new(qalc(dir.path()));
    runner
        .start(request(CasBackendOp::Evaluate))
        .expect("start");
    thread::sleep(Duration::from_millis(150));

    let started = Instant::now();
    drop(runner);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "dropping the runner must not wait for a surviving grandchild"
    );
}

#[test]
fn sympy_fixture_program_receives_evaluate_point_and_integrate_bounds() {
    let dir = tempfile::tempdir().expect("temp dir");
    install(dir.path(), "python3", SYMPY_FIELD_FIXTURE);
    let backend = select_backend(CasBackendKind::Sympy, Some(dir.path().as_os_str()));
    assert_eq!(backend.capabilities().kind, CasBackendKind::Sympy);

    let mut evaluate = request(CasBackendOp::Evaluate);
    evaluate.point = Some((2.0, 3.0, 4.0));
    assert_eq!(
        backend.evaluate(&evaluate).expect("evaluate"),
        "evaluate|2,3,4|-"
    );

    let integrate = request(CasBackendOp::Integrate {
        bounds: Some(("0".into(), "1".into())),
    });
    assert_eq!(
        backend.evaluate(&integrate).expect("integrate"),
        "integrate|-|0,1"
    );
}

#[test]
fn probe_failed_candidates_stay_listed_but_are_not_selected() {
    for (kind, name, probe_args) in [
        (CasBackendKind::Qalc, "qalc", "-v"),
        (CasBackendKind::Sage, "sage", "--version"),
    ] {
        let dir = tempfile::tempdir().expect("temp dir");
        install(
            dir.path(),
            name,
            &format!(
                "if [ \"$1\" = \"{probe_args}\" ]; then echo 'broken' >&2; exit 7; fi\ncat >/dev/null\nprintf '99\\n'"
            ),
        );

        let candidates = detect_candidates(Some(dir.path().as_os_str()));
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.kind == kind)
            .expect("a probe-failed qalc/sage stays listed");
        assert!(
            candidate.capabilities.is_empty(),
            "{kind:?} must advertise no capabilities on a failed probe: {:?}",
            candidate.capabilities
        );
        let reason = candidate.reason.as_deref().expect("failure reason");
        assert!(reason.contains("probe failed"), "{reason}");

        let backend = select_backend(kind, Some(dir.path().as_os_str()));
        assert_eq!(
            backend.capabilities().kind,
            CasBackendKind::Builtin,
            "a probe-failed {kind:?} must not be auto-selected"
        );
        let reason = backend
            .capabilities()
            .reason
            .as_deref()
            .expect("fallback reason");
        assert!(
            reason.contains("probe failed") && reason.contains("built-in"),
            "{reason}"
        );
        assert_eq!(
            backend.evaluate(&request(CasBackendOp::Evaluate)),
            Err(CasBackendError::Unsupported(CasCapability::Evaluate))
        );
    }
}

#[test]
fn read_errors_are_not_treated_as_clean_eof() {
    struct FailingReader;

    impl std::io::Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("simulated read failure"))
        }
    }

    let result = read_capped_for_test(FailingReader, 16);
    assert!(
        result.is_err(),
        "a read error must not look like clean EOF: {result:?}"
    );
}

#[test]
fn fixed_sympy_program_honours_point_and_bounds() {
    assert!(SYMPY_PROGRAM_FOR_TEST.contains("point.split(\",\")"));
    assert!(SYMPY_PROGRAM_FOR_TEST.contains("expr.subs(substitutions)"));
    assert!(SYMPY_PROGRAM_FOR_TEST.contains("(var, sympify(lower"));
}

#[test]
fn fixed_sage_program_honours_point_and_bounds() {
    assert!(SAGE_PROGRAM_FOR_TEST.contains("point.split(\",\")"));
    assert!(SAGE_PROGRAM_FOR_TEST.contains("expr.subs(substitutions)"));
    assert!(SAGE_PROGRAM_FOR_TEST.contains("var, sage_eval(lower"));
}
