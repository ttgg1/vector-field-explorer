//! Validated all-eight source model and scaled field evaluation facade.
//!
//! This pure CPU module exposes the approved public model, preparation and
//! evaluation seam. Task 1 dispatches the elementary point and infinite-wire
//! families; nonzero later-task geometries report `Unavailable`.

mod evaluate;
mod model;
mod numeric;
mod quadrature;

pub use evaluate::{Diagnostics, Estimate, Evaluator, FamilyResult, KERNEL_ID, Sample, StopReason};
pub use model::{
    AccuracyRequest, EvalOptions, FieldFamily, InputError, InvalidOption, MAX_CELLS_PER_FAMILY,
    MAX_DEPTH, MAX_QUERIES, MAX_SOURCES, MAX_WORK_PER_FAMILY, PrepareError, Source, SourceKind,
    SourceParameter, SourceSet, WorkLimitField, WorkLimits,
};
