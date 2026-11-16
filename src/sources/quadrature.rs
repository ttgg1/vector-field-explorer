//! Bounded, geometry-aware adaptive Gauss-Kronrod integration.
//!
//! The partition always covers the whole one-dimensional domain with finite
//! resolved leaves before any accuracy refinement. A leaf is accepted only
//! after the 15-point Gauss rule, the 31-point Kronrod rule and the two half
//! cells' 31-point Kronrod rules agree; the half-cell sum is the estimate and
//! the disagreement becomes an absolute uncertainty indicator.
//!
//! Refinement replaces the leaf with the largest absolute uncertainty, with a
//! deterministic lower-bound tie break. Parent cells are retained until child
//! capacity and work are confirmed, and the algorithm is iterative so no
//! recursion or per-split allocation occurs.

use super::numeric::NumericRange;

/// Positive Kronrod abscissae on `[-1, 1]`; index 15 is the centre.
/// Values from QUADPACK `dqk31` (80-digit Fullerton constants).
const KRONROD_X: [f64; 16] = [
    0.9980022986933971,
    0.9879925180204854,
    0.9677390756791391,
    0.937273392400706,
    0.8972645323440819,
    0.8482065834104272,
    0.790418501442466,
    0.7244177313601701,
    0.650996741297417,
    0.5709721726085388,
    0.4850818636402397,
    0.3941513470775634,
    0.29918000715316884,
    0.20119409399743451,
    0.1011420669187175,
    0.0,
];

/// Weights for [`KRONROD_X`].
const KRONROD_W: [f64; 16] = [
    0.005377479872923349,
    0.015007947329316122,
    0.02546084732671532,
    0.03534636079137585,
    0.04458975132476468,
    0.05348152469092809,
    0.06200956780067064,
    0.06985412131872826,
    0.07684968075772038,
    0.08308050282313302,
    0.08856444305621176,
    0.09312659817082532,
    0.09664272698362368,
    0.09917359872179196,
    0.10076984552387559,
    0.10133000701479154,
];

/// Gauss weights for the seven positive abscissae (the even-index Kronrod
/// nodes) and the centre, from QUADPACK `dqk31`.
const GAUSS_W: [f64; 8] = [
    0.030753241996118154,
    0.07036604748810715,
    0.10715922046717176,
    0.13957067792615432,
    0.16626920581699411,
    0.18616100001556224,
    0.19843148532711163,
    0.20257824192556137,
];

/// One kernel sample in the caller's local two-component basis.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KernelSample {
    pub value: [f64; 2],
    pub underflow: bool,
}

/// Why a complete partition could not be produced or refined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    WorkLimit,
    StorageLimit,
    UnresolvedFeature,
    RoundoffLimited,
    NumericRange,
    Cancelled,
}

/// Caller-supplied physical resolution guard for one cell.
pub(crate) trait ResolutionGuard {
    /// True when the physical parameter width is small enough relative to a
    /// conservative lower bound on the softened support distance.
    fn resolved(&self, lo: f64, hi: f64) -> bool;
}

/// A complete finite partition estimate with optional refinement stop reason.
#[derive(Debug, Clone)]
pub(crate) struct Partition {
    pub value: [f64; 2],
    pub uncertainty: f64,
    pub work_used: u64,
    pub cells_high_water: usize,
    pub stop: Option<Failure>,
    pub underflow: bool,
}

/// Work, storage, depth and absolute accuracy budget for one integration.
#[derive(Debug)]
pub(crate) struct Budget {
    work_limit: u64,
    work_used: u64,
    since_poll: u32,
    cells_limit: usize,
    max_depth: u8,
    target: f64,
    cells_high_water: usize,
}

impl Budget {
    pub(crate) fn new(work_limit: u64, cells_limit: usize, max_depth: u8, target: f64) -> Self {
        Self {
            work_limit,
            work_used: 0,
            since_poll: 0,
            cells_limit,
            max_depth,
            target,
            cells_high_water: 0,
        }
    }

    pub(crate) fn work_used(&self) -> u64 {
        self.work_used
    }

    pub(crate) fn cells_high_water(&self) -> usize {
        self.cells_high_water
    }

    fn remaining(&self) -> u64 {
        self.work_limit - self.work_used
    }

    fn note_cells(&mut self, count: usize) {
        self.cells_high_water = self.cells_high_water.max(count);
    }

    /// Consume one counted work unit and poll cancellation at most every 32
    /// counted units.
    fn spend_one(&mut self, cancelled: &dyn Fn() -> bool) -> Result<(), Failure> {
        if self.work_used >= self.work_limit {
            return Err(Failure::WorkLimit);
        }
        self.work_used += 1;
        self.since_poll += 1;
        if self.since_poll >= 32 {
            self.since_poll = 0;
            if cancelled() {
                return Err(Failure::Cancelled);
            }
        }
        Ok(())
    }

    fn poll(&mut self, cancelled: &dyn Fn() -> bool) -> Result<(), Failure> {
        self.since_poll = 0;
        if cancelled() {
            Err(Failure::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Probe {
    value: [f64; 2],
    gauss: [f64; 2],
    abs_primitive: f64,
    underflow: bool,
}

/// One resident partition cell.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Cell {
    lo: f64,
    hi: f64,
    depth: u8,
    probe: Probe,
    left: Probe,
    right: Probe,
    value: [f64; 2],
    indicator: f64,
    roundoff: f64,
    uncertainty: f64,
    underflow: bool,
}

impl Cell {
    fn unresolved(lo: f64, hi: f64, depth: u8) -> Self {
        Self {
            lo,
            hi,
            depth,
            probe: Probe::default(),
            left: Probe::default(),
            right: Probe::default(),
            value: [0.0; 2],
            indicator: 0.0,
            roundoff: 0.0,
            uncertainty: 0.0,
            underflow: false,
        }
    }

    fn inherited(lo: f64, hi: f64, depth: u8, probe: Probe) -> Self {
        Self {
            probe,
            ..Self::unresolved(lo, hi, depth)
        }
    }
}

/// Cover `domain` completely, then refine the largest absolute uncertainty
/// until `budget.target` is met or a limit/roundoff floor stops progress.
///
/// `scratch` must already have capacity for `budget.cells_limit` cells. No
/// allocation occurs here; capacity is checked before every parent replacement.
pub(crate) fn integrate<F, G>(
    domain: (f64, f64),
    seeds: &[f64],
    guard: &G,
    kernel: &mut F,
    budget: &mut Budget,
    scratch: &mut Vec<Cell>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Partition, Failure>
where
    F: FnMut(f64) -> Result<KernelSample, NumericRange>,
    G: ResolutionGuard,
{
    let (lo, hi) = domain;
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return Err(Failure::UnresolvedFeature);
    }
    budget.poll(cancelled)?;
    scratch.clear();
    // Seed the partition at caller-supplied support locations (already sorted)
    // plus the domain endpoints. This puts a narrow feature on a cell boundary
    // instead of relying on midpoint luck.
    let mut start = lo;
    for &seed in seeds {
        if !seed.is_finite() || seed <= start || seed >= hi {
            continue;
        }
        if scratch.len() >= budget.cells_limit {
            return Err(Failure::UnresolvedFeature);
        }
        budget.spend_one(cancelled)?;
        scratch.push(Cell::unresolved(start, seed, 0));
        start = seed;
    }
    if scratch.len() >= budget.cells_limit {
        return Err(Failure::UnresolvedFeature);
    }
    scratch.push(Cell::unresolved(start, hi, 0));
    budget.note_cells(scratch.len());

    // Resolution phase: expand every unresolved cell before evaluating any
    // quadrature, retaining the full domain at all times.
    let mut index = 0;
    while index < scratch.len() {
        let (cell_lo, cell_hi, depth) = {
            let cell = &scratch[index];
            (cell.lo, cell.hi, cell.depth)
        };
        if guard.resolved(cell_lo, cell_hi) {
            index += 1;
            continue;
        }
        if depth >= budget.max_depth {
            return Err(Failure::UnresolvedFeature);
        }
        if scratch.len() >= budget.cells_limit {
            return Err(Failure::UnresolvedFeature);
        }
        let mid = midpoint(cell_lo, cell_hi);
        let Some(mid) = mid else {
            return Err(Failure::UnresolvedFeature);
        };
        budget.spend_one(cancelled)?;
        scratch[index] = Cell::unresolved(cell_lo, mid, depth + 1);
        scratch.insert(index + 1, Cell::unresolved(mid, cell_hi, depth + 1));
        budget.note_cells(scratch.len());
    }

    // Initial complete resolved partition.
    for cell in scratch.iter_mut() {
        budget.poll(cancelled)?;
        initialise(cell, budget, kernel, cancelled)?;
    }

    let mut sums = [Neumaier::default(); 2];
    let mut uncertainty = Neumaier::default();
    for cell in scratch.iter() {
        for (index, component) in cell.value.into_iter().enumerate() {
            sums[index].add(component);
        }
        uncertainty.add(cell.uncertainty);
    }

    // Refinement phase.
    let mut stop = None;
    loop {
        let current_uncertainty = uncertainty.total();
        if !current_uncertainty.is_finite() {
            return Err(Failure::NumericRange);
        }
        if current_uncertainty <= budget.target {
            break;
        }
        let Some(best) = select_largest(scratch) else {
            stop = Some(Failure::RoundoffLimited);
            break;
        };
        let cell = scratch[best];
        if cell.indicator <= 0.5 * cell.roundoff || cell.uncertainty == 0.0 {
            stop = Some(Failure::RoundoffLimited);
            break;
        }
        if cell.depth >= budget.max_depth {
            stop = Some(Failure::UnresolvedFeature);
            break;
        }
        let Some(mid) = midpoint(cell.lo, cell.hi) else {
            stop = Some(Failure::RoundoffLimited);
            break;
        };
        if scratch.len() >= budget.cells_limit {
            stop = Some(Failure::StorageLimit);
            break;
        }
        // One split charge plus two 62-sample child completions.
        if budget.remaining() < 125 {
            stop = Some(Failure::WorkLimit);
            break;
        }

        for (index, component) in cell.value.into_iter().enumerate() {
            sums[index].add(-component);
        }
        uncertainty.add(-cell.uncertainty);

        let left = Cell::inherited(cell.lo, mid, cell.depth + 1, cell.left);
        let right = Cell::inherited(mid, cell.hi, cell.depth + 1, cell.right);
        budget.spend_one(cancelled)?;
        scratch[best] = left;
        scratch.push(right);
        budget.note_cells(scratch.len());

        let last = scratch.len() - 1;
        complete(&mut scratch[best], budget, kernel, cancelled)?;
        complete(&mut scratch[last], budget, kernel, cancelled)?;

        for child in [scratch[best], scratch[last]] {
            for (index, component) in child.value.into_iter().enumerate() {
                sums[index].add(component);
            }
            uncertainty.add(child.uncertainty);
        }
    }

    let value = [sums[0].total(), sums[1].total()];
    let uncertainty = uncertainty.total();
    let underflow = scratch.iter().any(|cell| cell.underflow);
    if !value[0].is_finite() || !value[1].is_finite() || !uncertainty.is_finite() {
        return Err(Failure::NumericRange);
    }

    Ok(Partition {
        value,
        uncertainty,
        work_used: budget.work_used(),
        cells_high_water: budget.cells_high_water(),
        stop,
        underflow,
    })
}

fn initialise<F>(
    cell: &mut Cell,
    budget: &mut Budget,
    kernel: &mut F,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), Failure>
where
    F: FnMut(f64) -> Result<KernelSample, NumericRange>,
{
    cell.probe = probe(cell.lo, cell.hi, budget, kernel, cancelled)?;
    complete(cell, budget, kernel, cancelled)
}

fn complete<F>(
    cell: &mut Cell,
    budget: &mut Budget,
    kernel: &mut F,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), Failure>
where
    F: FnMut(f64) -> Result<KernelSample, NumericRange>,
{
    let Some(mid) = midpoint(cell.lo, cell.hi) else {
        return Err(Failure::UnresolvedFeature);
    };
    cell.left = probe(cell.lo, mid, budget, kernel, cancelled)?;
    cell.right = probe(mid, cell.hi, budget, kernel, cancelled)?;
    cell.value = [
        cell.left.value[0] + cell.right.value[0],
        cell.left.value[1] + cell.right.value[1],
    ];
    let gauss_difference = norm2([
        cell.probe.value[0] - cell.probe.gauss[0],
        cell.probe.value[1] - cell.probe.gauss[1],
    ]);
    let half_difference = norm2([
        cell.probe.value[0] - cell.value[0],
        cell.probe.value[1] - cell.value[1],
    ]);
    cell.indicator = 8.0 * gauss_difference.max(half_difference);
    cell.roundoff = 64.0 * f64::EPSILON * cell.probe.abs_primitive;
    cell.underflow = cell.probe.underflow || cell.left.underflow || cell.right.underflow;
    if cell.underflow {
        cell.roundoff += 64.0 * f64::from_bits(1);
    }
    cell.uncertainty = cell.indicator + cell.roundoff;
    if !cell.indicator.is_finite() || !cell.roundoff.is_finite() || !cell.uncertainty.is_finite() {
        return Err(Failure::NumericRange);
    }
    Ok(())
}

fn probe<F>(
    lo: f64,
    hi: f64,
    budget: &mut Budget,
    kernel: &mut F,
    cancelled: &dyn Fn() -> bool,
) -> Result<Probe, Failure>
where
    F: FnMut(f64) -> Result<KernelSample, NumericRange>,
{
    if hi <= lo {
        return Err(Failure::UnresolvedFeature);
    }
    let mid = lo + 0.5 * (hi - lo);
    let half = 0.5 * (hi - lo);
    let mut output = Probe::default();

    let centre = sample(kernel, mid, budget, cancelled)?;
    add_weighted(&mut output, KRONROD_W[15], centre.value);
    output.abs_primitive += KRONROD_W[15] * (centre.value[0].abs() + centre.value[1].abs());
    output.underflow |= centre.underflow;
    output.gauss[0] += GAUSS_W[7] * centre.value[0];
    output.gauss[1] += GAUSS_W[7] * centre.value[1];

    for j in 1..=7 {
        let index = 2 * j - 1;
        let offset = half * KRONROD_X[index];
        let left = sample(kernel, mid - offset, budget, cancelled)?;
        let right = sample(kernel, mid + offset, budget, cancelled)?;
        let sum = [
            left.value[0] + right.value[0],
            left.value[1] + right.value[1],
        ];
        add_weighted(&mut output, KRONROD_W[index], sum);
        output.gauss[0] += GAUSS_W[j - 1] * sum[0];
        output.gauss[1] += GAUSS_W[j - 1] * sum[1];
        output.abs_primitive += KRONROD_W[index]
            * (left.value[0].abs()
                + left.value[1].abs()
                + right.value[0].abs()
                + right.value[1].abs());
        output.underflow |= left.underflow || right.underflow;
    }
    for j in 1..=8 {
        let index = 2 * j - 2;
        let offset = half * KRONROD_X[index];
        let left = sample(kernel, mid - offset, budget, cancelled)?;
        let right = sample(kernel, mid + offset, budget, cancelled)?;
        let sum = [
            left.value[0] + right.value[0],
            left.value[1] + right.value[1],
        ];
        add_weighted(&mut output, KRONROD_W[index], sum);
        output.abs_primitive += KRONROD_W[index]
            * (left.value[0].abs()
                + left.value[1].abs()
                + right.value[0].abs()
                + right.value[1].abs());
        output.underflow |= left.underflow || right.underflow;
    }

    output.value[0] *= half;
    output.value[1] *= half;
    output.gauss[0] *= half;
    output.gauss[1] *= half;
    output.abs_primitive *= half.abs();
    if !output.value[0].is_finite()
        || !output.value[1].is_finite()
        || !output.gauss[0].is_finite()
        || !output.gauss[1].is_finite()
        || !output.abs_primitive.is_finite()
    {
        return Err(Failure::NumericRange);
    }
    Ok(output)
}

fn sample<F>(
    kernel: &mut F,
    point: f64,
    budget: &mut Budget,
    cancelled: &dyn Fn() -> bool,
) -> Result<KernelSample, Failure>
where
    F: FnMut(f64) -> Result<KernelSample, NumericRange>,
{
    budget.spend_one(cancelled)?;
    kernel(point).map_err(|NumericRange::Unrepresentable| Failure::NumericRange)
}

fn add_weighted(probe: &mut Probe, weight: f64, value: [f64; 2]) {
    probe.value[0] += weight * value[0];
    probe.value[1] += weight * value[1];
}

fn midpoint(lo: f64, hi: f64) -> Option<f64> {
    let mid = lo + 0.5 * (hi - lo);
    if mid > lo && mid < hi && mid.is_finite() {
        Some(mid)
    } else {
        None
    }
}

fn norm2(value: [f64; 2]) -> f64 {
    value[0].hypot(value[1])
}

fn select_largest(cells: &[Cell]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (index, cell) in cells.iter().enumerate() {
        match best {
            None => best = Some(index),
            Some(current) => {
                let incumbent = &cells[current];
                if cell.uncertainty > incumbent.uncertainty
                    || (cell.uncertainty == incumbent.uncertainty && cell.lo < incumbent.lo)
                {
                    best = Some(index);
                }
            }
        }
    }
    best
}

#[derive(Debug, Clone, Copy, Default)]
struct Neumaier {
    sum: f64,
    compensation: f64,
}

impl Neumaier {
    fn add(&mut self, value: f64) {
        let next = self.sum + value;
        if self.sum.abs() >= value.abs() {
            self.compensation += (self.sum - next) + value;
        } else {
            self.compensation += (value - next) + self.sum;
        }
        self.sum = next;
    }

    fn total(self) -> f64 {
        self.sum + self.compensation
    }
}
