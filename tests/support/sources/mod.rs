//! Independent source references for the source-evaluator tests.
//!
//! The finite-line helpers implement the approved design's *analytic endpoint
//! primitive* directly. The committed fixture matrix is generated offline by
//! `generate.py` with mpmath at two decimal precisions and read here only as
//! data. Neither path calls production numeric, quadrature or electric code.
//!
//! The primitive, with `c = r^2 + a^2`, `u = z + L/2` and `v = z - L/2`, is
//!
//! ```text
//! Er = Q r / (L c) * [u / sqrt(c + u^2) - v / sqrt(c + v^2)]
//! Ez = Q / L      * [1 / sqrt(c + v^2) - 1 / sqrt(c + u^2)]
//! ```
//!
//! Far from the segment both `u` and `v` share a sign and the raw differences
//! cancel. The reference rationalizes those differences before evaluating; the
//! opposite-sign branch (the query projects inside the segment) uses the direct
//! sum, which is already stable.
//!
//! Fixture provenance: the committed [`FAR_SAME_SIGN_REFERENCES`] values were
//! evaluated with `mpmath` 1.4.1 at 60 decimal digits on 2026-11-16 using the
//! formulas above and true binary64 inputs. The earlier unsplit fixed-rule
//! experiment missed the narrow displaced line by `4.639761e-10` against a value
//! near `1`; that number is retained here as provenance only, never as this
//! solver's expected value.

use glam::DVec3;

/// One committed source fixture row.
///
/// `charge` is the signed electric charge or magnetic current. `dimension` is
/// the line length or the loop/ring/disc/sphere radius. `length` and `turns`
/// are used only by solenoid rows.
#[derive(Debug, Clone)]
pub struct Fixture {
    pub id: String,
    pub kind: String,
    pub center: DVec3,
    pub axis: DVec3,
    pub charge: f64,
    pub dimension: f64,
    pub query: DVec3,
    pub softening: f64,
    pub reference: DVec3,
    pub uncertainty: f64,
    pub length: f64,
    pub turns: u32,
    pub method: String,
}

/// Parse every committed fixture row.
pub fn fixtures() -> Vec<Fixture> {
    include_str!("fixtures.csv")
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(parse_fixture)
        .collect()
}

/// Fixtures restricted to one source kind (`point`, `line`, `ring`, `disc`,
/// `sphere`, `loop`, `solenoid`, `wire`).
pub fn fixtures_of(kind: &str) -> impl Iterator<Item = Fixture> {
    fixtures().into_iter().filter(move |row| row.kind == kind)
}

fn parse_fixture(line: &str) -> Fixture {
    let fields: Vec<&str> = line.split(',').collect();
    assert_eq!(fields.len(), 21, "malformed fixture row: {line}");
    Fixture {
        id: fields[0].to_owned(),
        kind: fields[1].to_owned(),
        center: DVec3::new(bits(fields[2]), bits(fields[3]), bits(fields[4])),
        axis: DVec3::new(bits(fields[5]), bits(fields[6]), bits(fields[7])),
        charge: bits(fields[8]),
        dimension: bits(fields[9]),
        query: DVec3::new(bits(fields[10]), bits(fields[11]), bits(fields[12])),
        softening: bits(fields[13]),
        reference: DVec3::new(
            decimal(fields[14]),
            decimal(fields[15]),
            decimal(fields[16]),
        ),
        uncertainty: decimal(fields[17]),
        length: bits(fields[18]),
        turns: fields[19]
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("bad turns: {}", fields[19])),
        method: fields[20].to_owned(),
    }
}

fn bits(text: &str) -> f64 {
    let digits = text.trim().trim_start_matches("0x");
    let raw = u64::from_str_radix(digits, 16).unwrap_or_else(|_| panic!("bad bits: {text}"));
    f64::from_bits(raw)
}

fn decimal(text: &str) -> f64 {
    text.parse()
        .unwrap_or_else(|_| panic!("bad reference value: {text}"))
}

/// Independent softened finite-line endpoint components in source-local
/// cylindrical coordinates.
///
/// `radial` is the perpendicular distance to the line axis and `axial` is the
/// projection onto it. Returns `(Er, Ez)`.
pub fn line_components(
    radial: f64,
    axial: f64,
    length: f64,
    charge: f64,
    softening: f64,
) -> (f64, f64) {
    let c = radial * radial + softening * softening;
    let u = axial + 0.5 * length;
    let v = axial - 0.5 * length;
    let a = (c + v * v).sqrt();
    let b = (c + u * u).sqrt();
    let same_sign = (u > 0.0 && v > 0.0) || (u < 0.0 && v < 0.0);

    let ez = if same_sign {
        // 1/a - 1/b = (b^2 - a^2) / (a b (a + b)) = 2 z L / (a b (a + b))
        charge / length * (2.0 * axial * length / (a * b * (a + b)))
    } else {
        charge / length * (1.0 / a - 1.0 / b)
    };

    let er = if radial == 0.0 {
        0.0
    } else if same_sign {
        // u/b - v/a = c (u^2 - v^2) / (a b (u a + v b))
        charge * radial * 2.0 * axial / (a * b * (u * a + v * b))
    } else {
        charge * radial / (length * c) * (u / b - v / a)
    };

    (er, ez)
}

/// One committed high-precision finite-line endpoint reference.
#[derive(Debug, Clone, Copy)]
pub struct EndpointReference {
    pub radial: f64,
    pub axial: f64,
    pub length: f64,
    pub charge: f64,
    pub softening: f64,
    pub er: f64,
    pub ez: f64,
}

/// Far queries whose two endpoint parameters share a sign and therefore need
/// rationalized differences in the analytic primitive.
///
/// Values are correctly rounded binary64 renderings of 60-digit `mpmath`
/// results. See the module provenance note.
pub const FAR_SAME_SIGN_REFERENCES: &[EndpointReference] = &[
    EndpointReference {
        radial: 0.3,
        axial: 5.0,
        length: 2.0,
        charge: 1.0,
        softening: 0.1,
        er: 0.002586638084183439,
        ez: 0.04139336340748093,
    },
    EndpointReference {
        radial: 0.5,
        axial: 12.0,
        length: 2.0,
        charge: -1.5,
        softening: 0.1,
        er: -0.0004389050323313704,
        ez: -0.010460702695692043,
    },
    EndpointReference {
        radial: 1.0,
        axial: -7.0,
        length: 2.0,
        charge: 2.0,
        softening: 0.1,
        er: 0.005882081270175357,
        ez: -0.04035158117169962,
    },
];
