//! Native one-shot plot configuration and window-free export dispatch.
use std::{ffi::OsString, path::PathBuf};

use clap::{Parser, ValueEnum};
use serde::Deserialize;

#[path = "cli_scene.rs"]
mod scene;
pub use scene::{OutputFrame, PreparedScene, output_frame, prepare_scene};

#[path = "cli_svg.rs"]
mod svg;
pub use svg::scene_to_cli_svg;

#[path = "cli_gpu.rs"]
mod gpu;
pub use gpu::{acquire_device_with, offscreen_error_to_cli, render_offscreen, validate_gpu_limits};

use crate::{
    arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D},
    camera::OrbitCamera,
    colormap::Colormap,
    domain::Domain,
    field::FieldExpr,
    fly_camera::FlyCamera,
    particles::{ParticleAppearance, ParticleColorMode},
    plane::{PlaneAxis, PlaneView},
    viewmodel::{ArrowLengthMode, State, StreamlineSeedMode},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    Parse(String),
    Config(String),
    Validation(String),
    Formula(String),
    Adapter(String),
    Device(String),
    Render(String),
    Readback(String),
    Write(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(s)
            | Self::Config(s)
            | Self::Validation(s)
            | Self::Formula(s)
            | Self::Adapter(s)
            | Self::Device(s)
            | Self::Render(s)
            | Self::Readback(s)
            | Self::Write(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for CliError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[value(name = "2d")]
    #[serde(rename = "2d")]
    TwoD,
    #[value(name = "3d")]
    #[serde(rename = "3d")]
    ThreeD,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum LengthMode {
    Proportional,
    Uniform,
    Magnitude,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Map {
    Viridis,
    Turbo,
    Mono,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Style3d {
    Cone,
    Cylinder,
    Flat,
    Streamline,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Style2d {
    Flat,
    Streamline,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum SeedMode {
    Auto,
    Click,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    X,
    Y,
    Z,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ParticleColor {
    Magnitude,
    Speed,
    Fixed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum CameraKind {
    Orbit,
    Fly,
}

// `Missing` is deliberately different from an explicit JSON null (automatic).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ColorRangePatch {
    #[default]
    Missing,
    Null,
    Value([f32; 2]),
}
fn range_patch<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ColorRangePatch, D::Error> {
    Option::<[f32; 2]>::deserialize(d).map(|v| match v {
        Some(pair) => ColorRangePatch::Value(pair),
        None => ColorRangePatch::Null,
    })
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DomainPatch {
    pub min: Option<[f32; 3]>,
    pub max: Option<[f32; 3]>,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ArrowParamsPatch {
    pub head_length: Option<f32>,
    pub head_width: Option<f32>,
    pub shaft_radius: Option<f32>,
    pub sides: Option<u32>,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlanePatch {
    pub enabled: Option<bool>,
    pub axis: Option<Axis>,
    pub index: Option<u32>,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppearancePatch {
    pub opacity: Option<f32>,
    pub color_mode: Option<ParticleColor>,
    pub fixed_color: Option<[u8; 3]>,
    pub trail_len: Option<u32>,
    pub trail_fade: Option<f32>,
    pub trail_width: Option<f32>,
    pub loop_on_stagnation: Option<bool>,
    pub stagnation_seconds: Option<f32>,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CameraPatch {
    pub target: Option<[f32; 3]>,
    pub kind: Option<CameraKind>,
    pub position: Option<[f32; 3]>,
    pub yaw: Option<f32>,
    pub pitch: Option<f32>,
    pub distance: Option<f32>,
    pub fov_y_degrees: Option<f32>,
    pub ortho_zoom: Option<f32>,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReferencesPatch {
    pub grid_and_domain_box: Option<bool>,
    pub world_axes: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CliConfig {
    pub output: Option<PathBuf>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub mode: Option<Mode>,
    pub formula: Option<String>,
    pub domain: Option<DomainPatch>,
    pub density: Option<u32>,
    pub arrows: Option<bool>,
    pub streamlines: Option<bool>,
    pub particles: Option<bool>,
    pub arrow_scale: Option<f32>,
    pub arrow_opacity: Option<f32>,
    pub plane_dim_opacity: Option<f32>,
    pub arrow_length_mode: Option<LengthMode>,
    pub magnitude_scale: Option<f32>,
    pub proportional_exponent: Option<f32>,
    pub colormap: Option<Map>,
    #[serde(default, deserialize_with = "range_patch")]
    pub color_range: ColorRangePatch,
    pub arrow_style_3d: Option<Style3d>,
    pub arrow_style_2d: Option<Style2d>,
    pub arrow_params: Option<ArrowParamsPatch>,
    pub streamline_seed_mode: Option<SeedMode>,
    pub plane: Option<PlanePatch>,
    pub particle_count: Option<u32>,
    pub particle_size: Option<f32>,
    pub particle_appearance: Option<AppearancePatch>,
    pub camera: Option<CameraPatch>,
    pub references: Option<ReferencesPatch>,
    pub background: Option<[u8; 4]>,
    pub seed: Option<u64>,
    pub start_time_seconds: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub fixed_step_seconds: Option<f64>,
}

fn comma_array<T: std::str::FromStr, const N: usize>(s: &str) -> Result<[T; N], String> {
    let values = s
        .split(',')
        .map(str::parse::<T>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("invalid comma-separated tuple: {s}"))?;
    values
        .try_into()
        .map_err(|_| format!("expected {N} comma-separated values: {s}"))
}
fn cli_range(s: &str) -> Result<ColorRangePatch, String> {
    if s == "auto" {
        Ok(ColorRangePatch::Null)
    } else {
        comma_array(s).map(ColorRangePatch::Value)
    }
}

#[derive(Debug, Clone, Default, Parser)]
#[command(version, about = "Export a vector-field plot without opening a window")]
pub struct CliFlags {
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long)]
    pub width: Option<u32>,
    #[arg(long)]
    pub height: Option<u32>,
    #[arg(long)]
    pub mode: Option<Mode>,
    #[arg(long)]
    pub formula: Option<String>,
    #[arg(long, allow_hyphen_values = true, value_parser = comma_array::<f32, 3>)]
    pub domain_min: Option<[f32; 3]>,
    #[arg(long, allow_hyphen_values = true, value_parser = comma_array::<f32, 3>)]
    pub domain_max: Option<[f32; 3]>,
    #[arg(long)]
    pub density: Option<u32>,
    #[arg(long)]
    pub arrows: Option<bool>,
    #[arg(long)]
    pub streamlines: Option<bool>,
    #[arg(long)]
    pub particles: Option<bool>,
    #[arg(long)]
    pub arrow_scale: Option<f32>,
    #[arg(long)]
    pub arrow_opacity: Option<f32>,
    #[arg(long)]
    pub plane_dim_opacity: Option<f32>,
    #[arg(long)]
    pub arrow_length_mode: Option<LengthMode>,
    #[arg(long)]
    pub magnitude_scale: Option<f32>,
    #[arg(long)]
    pub proportional_exponent: Option<f32>,
    #[arg(long)]
    pub colormap: Option<Map>,
    #[arg(long, allow_hyphen_values = true, value_parser = cli_range)]
    pub color_range: Option<ColorRangePatch>,
    #[arg(long)]
    pub arrow_style_3d: Option<Style3d>,
    #[arg(long)]
    pub arrow_style_2d: Option<Style2d>,
    #[arg(long)]
    pub head_length: Option<f32>,
    #[arg(long)]
    pub head_width: Option<f32>,
    #[arg(long)]
    pub shaft_radius: Option<f32>,
    #[arg(long)]
    pub arrow_sides: Option<u32>,
    #[arg(long)]
    pub streamline_seed_mode: Option<SeedMode>,
    #[arg(long)]
    pub plane_enabled: Option<bool>,
    #[arg(long)]
    pub plane_axis: Option<Axis>,
    #[arg(long)]
    pub plane_index: Option<u32>,
    #[arg(long)]
    pub particle_count: Option<u32>,
    #[arg(long)]
    pub particle_size: Option<f32>,
    #[arg(long)]
    pub particle_opacity: Option<f32>,
    #[arg(long)]
    pub particle_color: Option<ParticleColor>,
    #[arg(long, value_parser = comma_array::<u8, 3>)]
    pub particle_color_rgb: Option<[u8; 3]>,
    #[arg(long)]
    pub trail_len: Option<u32>,
    #[arg(long)]
    pub trail_fade: Option<f32>,
    #[arg(long)]
    pub trail_width: Option<f32>,
    #[arg(long)]
    pub loop_on_stagnation: Option<bool>,
    #[arg(long)]
    pub stagnation_seconds: Option<f32>,
    #[arg(long, allow_hyphen_values = true, value_parser = comma_array::<f32, 3>)]
    pub camera_target: Option<[f32; 3]>,
    #[arg(long)]
    pub camera_kind: Option<CameraKind>,
    #[arg(long, allow_hyphen_values = true, value_parser = comma_array::<f32, 3>)]
    pub camera_position: Option<[f32; 3]>,
    #[arg(long, allow_negative_numbers = true)]
    pub camera_yaw: Option<f32>,
    #[arg(long, allow_negative_numbers = true)]
    pub camera_pitch: Option<f32>,
    #[arg(long)]
    pub camera_distance: Option<f32>,
    #[arg(long)]
    pub camera_fov_deg: Option<f32>,
    #[arg(long)]
    pub camera_ortho_zoom: Option<f32>,
    #[arg(long)]
    pub references_grid_and_box: Option<bool>,
    #[arg(long)]
    pub references_world_axes: Option<bool>,
    #[arg(long, value_parser = comma_array::<u8, 4>)]
    pub background_rgba: Option<[u8; 4]>,
    #[arg(long)]
    pub seed: Option<u64>,
    #[arg(long)]
    pub start_time: Option<f64>,
    #[arg(long)]
    pub duration: Option<f64>,
    #[arg(long)]
    pub fixed_step: Option<f64>,
}

pub fn parse_flags(args: impl IntoIterator<Item = OsString>) -> Result<CliFlags, CliError> {
    CliFlags::try_parse_from(args).map_err(|e| CliError::Parse(e.to_string()))
}

/// Run a one-shot invocation, including successful help/version requests.
/// The native entry point retains GUI ownership when there are no arguments.
pub fn run_from(args: impl IntoIterator<Item = OsString>) -> Result<(), CliError> {
    let flags = match CliFlags::try_parse_from(args) {
        Ok(flags) => flags,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            return error
                .print()
                .map_err(|e| CliError::Write(format!("write CLI help/version: {e}")));
        }
        Err(error) => return Err(CliError::Parse(error.to_string())),
    };
    let config = flags
        .config
        .as_ref()
        .map(|path| {
            let json = std::fs::read_to_string(path)
                .map_err(|e| CliError::Config(format!("read config {}: {e}", path.display())))?;
            parse_config(&json)
                .map_err(|e| CliError::Config(format!("config {}: {e}", path.display())))
        })
        .transpose()?;
    run_export(resolve_config(config, flags)?)
}

/// Prepare one final scene and save it to the explicit path, without GUI state.
pub fn run_export(options: CliOptions) -> Result<(), CliError> {
    validate_effective(&options)?;
    let prepared = prepare_scene(&options, output_frame(&options)?)?;
    if output_extension(&options.output)?.eq_ignore_ascii_case("svg") {
        std::fs::write(&options.output, scene_to_cli_svg(&prepared)?)
            .map_err(|e| CliError::Write(format!("write SVG {}: {e}", options.output.display())))
    } else {
        let pixels = render_offscreen(&prepared)?;
        image::save_buffer_with_format(
            &options.output,
            &pixels,
            options.width,
            options.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|e| CliError::Write(format!("write PNG {}: {e}", options.output.display())))
    }
}

fn reject_nulls(value: &serde_json::Value, path: &str) -> Result<(), CliError> {
    if let serde_json::Value::Object(map) = value {
        for (key, child) in map {
            if child.is_null() && !(path.is_empty() && key == "color_range") {
                return Err(CliError::Config(format!(
                    "null is not allowed at {path}{key}"
                )));
            }
            reject_nulls(child, &format!("{path}{key}."))?;
        }
    }
    Ok(())
}
pub fn parse_config(json: &str) -> Result<CliConfig, CliError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| CliError::Config(e.to_string()))?;
    reject_nulls(&value, "")?;
    serde_json::from_value(value).map_err(|e| CliError::Config(e.to_string()))
}

#[derive(Debug, Clone)]
pub struct CameraOptions {
    pub kind: CameraKind,
    pub target: [f32; 3],
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub fov_y_degrees: f32,
    pub ortho_zoom: f32,
    /// Explicitly supplied members, needed to reject mode-inapplicable controls.
    pub specified: CameraSpecified,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CameraSpecified {
    pub target: bool,
    pub position: bool,
    pub yaw: bool,
    pub pitch: bool,
    pub distance: bool,
    pub fov_y_degrees: bool,
    pub ortho_zoom: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ReferencesOptions {
    pub grid_and_domain_box: bool,
    pub world_axes: bool,
}

#[derive(Debug, Clone)]
pub struct CliOptions {
    pub output: PathBuf,
    pub width: u32,
    pub height: u32,
    pub mode: Mode,
    pub formula: String,
    pub domain: Domain,
    pub density: u32,
    pub arrows: bool,
    pub streamlines: bool,
    pub particles: bool,
    pub arrow_scale: f32,
    pub arrow_opacity: f32,
    pub plane_dim_opacity: f32,
    pub arrow_length_mode: ArrowLengthMode,
    pub magnitude_scale: f32,
    pub proportional_exponent: f32,
    pub colormap: Colormap,
    pub color_range: Option<[f32; 2]>,
    pub arrow_style_3d: ArrowStyle,
    pub arrow_style_2d: ArrowStyle2D,
    pub arrow_params: ArrowParams,
    pub streamline_seed_mode: StreamlineSeedMode,
    pub plane: PlaneView,
    pub particle_count: u32,
    pub particle_size: f32,
    pub particle_appearance: ParticleAppearance,
    pub camera: CameraOptions,
    pub references: ReferencesOptions,
    pub background: [u8; 4],
    pub seed: u64,
    pub start_time_seconds: f64,
    pub duration_seconds: f64,
    pub fixed_step_seconds: f64,
}

macro_rules! overlay { ($base:expr, $other:expr, $($field:ident),+ $(,)?) => {
    $(if $other.$field.is_some() { $base.$field = $other.$field; })+
}; }

fn range(name: &str, v: f32, min: f32, max: f32) -> Result<(), CliError> {
    if !v.is_finite() || !(min..=max).contains(&v) {
        return Err(CliError::Validation(format!(
            "{name} must be finite and in {min}..={max}"
        )));
    }
    Ok(())
}
fn number(name: &str, v: Option<f32>, min: f32, max: f32) -> Result<(), CliError> {
    if let Some(v) = v {
        range(name, v, min, max)?;
    }
    Ok(())
}
fn time_number(name: &str, v: Option<f64>, min: f64, max: f64) -> Result<(), CliError> {
    if let Some(v) = v
        && (!v.is_finite() || !(min..=max).contains(&v))
    {
        return Err(CliError::Validation(format!(
            "{name} must be finite and in {min}..={max}"
        )));
    }
    Ok(())
}
fn integer(name: &str, v: Option<u32>, min: u32, max: u32) -> Result<(), CliError> {
    if let Some(v) = v
        && !(min..=max).contains(&v)
    {
        return Err(CliError::Validation(format!(
            "{name} must be in {min}..={max}"
        )));
    }
    Ok(())
}
fn triple(name: &str, v: Option<[f32; 3]>, bounded: bool) -> Result<(), CliError> {
    if let Some(v) = v {
        for x in v {
            range(
                name,
                x,
                if bounded { -100.0 } else { -f32::MAX },
                if bounded { 100.0 } else { f32::MAX },
            )?;
        }
    }
    Ok(())
}
fn formula(src: &str) -> Result<FieldExpr, CliError> {
    let field = FieldExpr::parse(src).map_err(|e| CliError::Formula(e.to_string()))?;
    let text = src.trim();
    let text = text
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(text);
    let mut depth = 0;
    let mut start = 0;
    let mut components = Vec::new();
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                components.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    components.push(&text[start..]);
    for component in components {
        let _bound = component
            .trim()
            .parse::<meval::Expr>()
            .map_err(|e| CliError::Formula(e.to_string()))?
            .bind4("x", "y", "z", "t")
            .map_err(|e| CliError::Formula(format!("{component}: {e}")))?;
    }
    Ok(field)
}

fn output_extension(path: &std::path::Path) -> Result<&str, CliError> {
    if path.as_os_str().is_empty() || path.file_name().is_none() {
        return Err(CliError::Validation("output needs a file path".into()));
    }
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if !ext.eq_ignore_ascii_case("png") && !ext.eq_ignore_ascii_case("svg") {
        return Err(CliError::Validation(
            "output extension must be .svg or .png".into(),
        ));
    }
    Ok(ext)
}

pub fn validate_intrinsic_config(c: &CliConfig) -> Result<(), CliError> {
    if let Some(output) = &c.output {
        output_extension(output)?;
    }
    integer("width", c.width, 1, 4096)?;
    integer("height", c.height, 1, 4096)?;
    integer("density", c.density, 4, 200)?;
    number("arrow_scale", c.arrow_scale, 0.1, 4.0)?;
    number("arrow_opacity", c.arrow_opacity, 0.05, 1.0)?;
    number("plane_dim_opacity", c.plane_dim_opacity, 0.0, 1.0)?;
    number("magnitude_scale", c.magnitude_scale, 0.01, 10.0)?;
    number("proportional_exponent", c.proportional_exponent, 0.25, 4.0)?;
    integer("particle_count", c.particle_count, 0, 4000)?;
    number("particle_size", c.particle_size, 0.25, 3.0)?;
    time_number("start_time_seconds", c.start_time_seconds, 0.0, 3600.0)?;
    time_number("duration_seconds", c.duration_seconds, 0.0, 3600.0)?;
    time_number("fixed_step_seconds", c.fixed_step_seconds, 0.0001, 0.25)?;
    if let Some(d) = &c.domain {
        triple("domain.min", d.min, true)?;
        triple("domain.max", d.max, true)?;
        if let (Some(min), Some(max)) = (d.min, d.max)
            && !vec3(min).cmplt(vec3(max)).all()
        {
            return Err(CliError::Validation(
                "domain min must be less than max on every axis".into(),
            ));
        }
    }
    if let (Some(start), Some(duration)) = (c.start_time_seconds, c.duration_seconds)
        && start + duration > 3600.0
    {
        return Err(CliError::Validation(
            "start plus duration exceeds 3600 seconds".into(),
        ));
    }
    if let Some(a) = &c.arrow_params {
        number("head_length", a.head_length, 0.1, 0.5)?;
        number("head_width", a.head_width, 0.02, 0.3)?;
        number("shaft_radius", a.shaft_radius, 0.01, 0.12)?;
        integer("sides", a.sides, 4, 16)?;
    }
    if let Some(p) = &c.particle_appearance {
        number("particle opacity", p.opacity, 0.05, 1.0)?;
        integer("trail_len", p.trail_len, 0, 32)?;
        number("trail_fade", p.trail_fade, 0.0, 1.0)?;
        number("trail_width", p.trail_width, 0.05, 2.0)?;
        number("stagnation_seconds", p.stagnation_seconds, 0.1, 5.0)?;
    }
    if let Some(cam) = &c.camera {
        triple("camera.target", cam.target, false)?;
        triple("camera.position", cam.position, false)?;
        number("camera.yaw", cam.yaw, -f32::MAX, f32::MAX)?;
        number("camera.pitch", cam.pitch, -1.55, 1.55)?;
        number("camera.distance", cam.distance, 0.5, 20.0)?;
        number("camera.ortho_zoom", cam.ortho_zoom, 0.05, 50.0)?;
        if let Some(fov) = cam.fov_y_degrees
            && (!fov.is_finite() || fov <= 0.0 || fov >= 180.0)
        {
            return Err(CliError::Validation(
                "camera.fov_y_degrees must be between 0 and 180".into(),
            ));
        }
    }
    if let ColorRangePatch::Value([lo, hi]) = c.color_range
        && (!lo.is_finite() || !hi.is_finite() || lo >= hi)
    {
        return Err(CliError::Validation(
            "color_range needs finite min < max".into(),
        ));
    }
    if let Some(src) = &c.formula {
        formula(src)?;
    }
    Ok(())
}

impl From<CliFlags> for CliConfig {
    fn from(f: CliFlags) -> Self {
        Self {
            output: f.output,
            width: f.width,
            height: f.height,
            mode: f.mode,
            formula: f.formula,
            domain: Some(DomainPatch {
                min: f.domain_min,
                max: f.domain_max,
            }),
            density: f.density,
            arrows: f.arrows,
            streamlines: f.streamlines,
            particles: f.particles,
            arrow_scale: f.arrow_scale,
            arrow_opacity: f.arrow_opacity,
            plane_dim_opacity: f.plane_dim_opacity,
            arrow_length_mode: f.arrow_length_mode,
            magnitude_scale: f.magnitude_scale,
            proportional_exponent: f.proportional_exponent,
            colormap: f.colormap,
            color_range: f.color_range.unwrap_or_default(),
            arrow_style_3d: f.arrow_style_3d,
            arrow_style_2d: f.arrow_style_2d,
            arrow_params: Some(ArrowParamsPatch {
                head_length: f.head_length,
                head_width: f.head_width,
                shaft_radius: f.shaft_radius,
                sides: f.arrow_sides,
            }),
            streamline_seed_mode: f.streamline_seed_mode,
            plane: Some(PlanePatch {
                enabled: f.plane_enabled,
                axis: f.plane_axis,
                index: f.plane_index,
            }),
            particle_count: f.particle_count,
            particle_size: f.particle_size,
            particle_appearance: Some(AppearancePatch {
                opacity: f.particle_opacity,
                color_mode: f.particle_color,
                fixed_color: f.particle_color_rgb,
                trail_len: f.trail_len,
                trail_fade: f.trail_fade,
                trail_width: f.trail_width,
                loop_on_stagnation: f.loop_on_stagnation,
                stagnation_seconds: f.stagnation_seconds,
            }),
            camera: Some(CameraPatch {
                target: f.camera_target,
                kind: f.camera_kind,
                position: f.camera_position,
                yaw: f.camera_yaw,
                pitch: f.camera_pitch,
                distance: f.camera_distance,
                fov_y_degrees: f.camera_fov_deg,
                ortho_zoom: f.camera_ortho_zoom,
            }),
            references: Some(ReferencesPatch {
                grid_and_domain_box: f.references_grid_and_box,
                world_axes: f.references_world_axes,
            }),
            background: f.background_rgba,
            seed: f.seed,
            start_time_seconds: f.start_time,
            duration_seconds: f.duration,
            fixed_step_seconds: f.fixed_step,
        }
    }
}

fn merge(mut c: CliConfig, f: CliConfig) -> CliConfig {
    overlay!(
        c,
        f,
        output,
        width,
        height,
        mode,
        formula,
        density,
        arrows,
        streamlines,
        particles,
        arrow_scale,
        arrow_opacity,
        plane_dim_opacity,
        arrow_length_mode,
        magnitude_scale,
        proportional_exponent,
        colormap,
        arrow_style_3d,
        arrow_style_2d,
        streamline_seed_mode,
        particle_count,
        particle_size,
        background,
        seed,
        start_time_seconds,
        duration_seconds,
        fixed_step_seconds
    );
    if f.color_range != ColorRangePatch::Missing {
        c.color_range = f.color_range;
    }
    if let Some(p) = f.domain {
        let d = c.domain.get_or_insert_with(DomainPatch::default);
        overlay!(d, p, min, max);
    }
    if let Some(p) = f.arrow_params {
        let d = c.arrow_params.get_or_insert_with(ArrowParamsPatch::default);
        overlay!(d, p, head_length, head_width, shaft_radius, sides);
    }
    if let Some(p) = f.plane {
        let d = c.plane.get_or_insert_with(PlanePatch::default);
        overlay!(d, p, enabled, axis, index);
    }
    if let Some(p) = f.particle_appearance {
        let d = c
            .particle_appearance
            .get_or_insert_with(AppearancePatch::default);
        overlay!(
            d,
            p,
            opacity,
            color_mode,
            fixed_color,
            trail_len,
            trail_fade,
            trail_width,
            loop_on_stagnation,
            stagnation_seconds
        );
    }
    if let Some(p) = f.camera {
        let d = c.camera.get_or_insert_with(CameraPatch::default);
        overlay!(
            d,
            p,
            target,
            kind,
            position,
            yaw,
            pitch,
            distance,
            fov_y_degrees,
            ortho_zoom
        );
    }
    if let Some(p) = f.references {
        let d = c.references.get_or_insert_with(ReferencesPatch::default);
        overlay!(d, p, grid_and_domain_box, world_axes);
    }
    c
}

fn vec3(a: [f32; 3]) -> glam::Vec3 {
    glam::Vec3::from_array(a)
}

fn resolved_camera(patch: CameraPatch, domain: Domain) -> CameraOptions {
    let orbit = OrbitCamera {
        target: domain.center(),
        ..OrbitCamera::default()
    };
    let fly = FlyCamera::from_orbit(&orbit, FlyCamera::default().sensitivity);
    let kind = patch.kind.unwrap_or(CameraKind::Orbit);
    let specified = CameraSpecified {
        target: patch.target.is_some(),
        position: patch.position.is_some(),
        yaw: patch.yaw.is_some(),
        pitch: patch.pitch.is_some(),
        distance: patch.distance.is_some(),
        fov_y_degrees: patch.fov_y_degrees.is_some(),
        ortho_zoom: patch.ortho_zoom.is_some(),
    };
    CameraOptions {
        kind,
        target: patch.target.unwrap_or(orbit.target.to_array()),
        position: patch.position.unwrap_or(fly.position.to_array()),
        yaw: patch.yaw.unwrap_or(if kind == CameraKind::Fly {
            fly.yaw
        } else {
            orbit.yaw
        }),
        pitch: patch.pitch.unwrap_or(if kind == CameraKind::Fly {
            fly.pitch
        } else {
            orbit.pitch
        }),
        distance: patch.distance.unwrap_or(orbit.distance),
        fov_y_degrees: patch.fov_y_degrees.unwrap_or(orbit.fov_y.to_degrees()),
        ortho_zoom: patch.ortho_zoom.unwrap_or(orbit.ortho_zoom),
        specified,
    }
}

pub fn resolve_config(config: Option<CliConfig>, flags: CliFlags) -> Result<CliOptions, CliError> {
    let config = config.unwrap_or_default();
    validate_intrinsic_config(&config)?;
    let flags = CliConfig::from(flags);
    validate_intrinsic_config(&flags)?;
    let c = merge(config, flags);
    let mode = c.mode.unwrap_or(Mode::TwoD);
    let mut state = State::new(mode == Mode::ThreeD);
    let formula = c.formula.unwrap_or_else(|| state.formula.clone());
    formula_field(&formula)?;
    state.set_formula(formula.clone());
    let density = c.density.unwrap_or(state.density);
    // Check before calling the clamping setter; 3D mode with a flat field is 2D sampling.
    let max = if state.is_3d() { 40 } else { 200 };
    integer("density", Some(density), 4, max)?;
    state.set_density(density);
    let d = c.domain.unwrap_or_default();
    let domain = Domain::new(
        vec3(d.min.unwrap_or(state.domain.min.to_array())),
        vec3(d.max.unwrap_or(state.domain.max.to_array())),
    );
    if !(domain.min.cmplt(domain.max).all()) {
        return Err(CliError::Validation(
            "domain min must be less than max on every axis".into(),
        ));
    }
    state.set_domain(domain.min, domain.max);
    let p = c.plane.unwrap_or_default();
    let axis = match p.axis.unwrap_or(Axis::Z) {
        Axis::X => PlaneAxis::X,
        Axis::Y => PlaneAxis::Y,
        Axis::Z => PlaneAxis::Z,
    };
    let index = p
        .index
        .unwrap_or_else(|| axis.middle_index(state.plane_grid()));
    if index >= axis.count(state.plane_grid()) {
        return Err(CliError::Validation(
            "plane.index exceeds final grid axis".into(),
        ));
    }
    let plane = PlaneView {
        enabled: p.enabled.unwrap_or(false),
        axis,
        index,
    };
    let a = c.arrow_params.unwrap_or_default();
    let defaults = state.arrow_params;
    let appearance = c.particle_appearance.unwrap_or_default();
    let mut particle_appearance = state.particle_appearance;
    if let Some(v) = appearance.opacity {
        particle_appearance.opacity = v;
    }
    if let Some(v) = appearance.color_mode {
        particle_appearance.color_mode = match v {
            ParticleColor::Magnitude => ParticleColorMode::Magnitude,
            ParticleColor::Speed => ParticleColorMode::Speed,
            ParticleColor::Fixed => ParticleColorMode::Fixed,
        };
    }
    if let Some(v) = appearance.fixed_color {
        particle_appearance.fixed_color = [v[0], v[1], v[2], 255];
    }
    if let Some(v) = appearance.trail_len {
        particle_appearance.trail_len = v;
    }
    if let Some(v) = appearance.trail_fade {
        particle_appearance.trail_fade = v;
    }
    if let Some(v) = appearance.trail_width {
        particle_appearance.trail_width = v;
    }
    if let Some(v) = appearance.loop_on_stagnation {
        particle_appearance.loop_on_stagnation = v;
    }
    if let Some(v) = appearance.stagnation_seconds {
        particle_appearance.stagnation_seconds = v;
    }
    let camera = resolved_camera(c.camera.unwrap_or_default(), domain);
    let refs = c.references.unwrap_or_default();
    let o = CliOptions {
        output: c
            .output
            .ok_or_else(|| CliError::Validation("output path is required".into()))?,
        width: c.width.unwrap_or(1280),
        height: c.height.unwrap_or(800),
        mode,
        formula,
        domain,
        density,
        arrows: c.arrows.unwrap_or(state.arrows),
        streamlines: c.streamlines.unwrap_or(state.streamlines),
        particles: c.particles.unwrap_or(state.particles),
        arrow_scale: c.arrow_scale.unwrap_or(state.arrow_scale),
        arrow_opacity: c.arrow_opacity.unwrap_or(state.arrow_opacity),
        plane_dim_opacity: c.plane_dim_opacity.unwrap_or(state.plane_dim_opacity),
        arrow_length_mode: match c.arrow_length_mode.unwrap_or(LengthMode::Proportional) {
            LengthMode::Proportional => ArrowLengthMode::Proportional,
            LengthMode::Uniform => ArrowLengthMode::Uniform,
            LengthMode::Magnitude => ArrowLengthMode::Magnitude,
        },
        magnitude_scale: c.magnitude_scale.unwrap_or(state.magnitude_scale),
        proportional_exponent: c
            .proportional_exponent
            .unwrap_or(state.proportional_exponent),
        colormap: match c.colormap.unwrap_or(Map::Viridis) {
            Map::Viridis => Colormap::Viridis,
            Map::Turbo => Colormap::Turbo,
            Map::Mono => Colormap::Mono,
        },
        color_range: match c.color_range {
            ColorRangePatch::Missing | ColorRangePatch::Null => None,
            ColorRangePatch::Value(v) => Some(v),
        },
        arrow_style_3d: match c.arrow_style_3d.unwrap_or(Style3d::Cone) {
            Style3d::Cone => ArrowStyle::Cone,
            Style3d::Cylinder => ArrowStyle::Cylinder,
            Style3d::Flat => ArrowStyle::Flat,
            Style3d::Streamline => ArrowStyle::Streamline,
        },
        arrow_style_2d: match c.arrow_style_2d.unwrap_or(Style2d::Flat) {
            Style2d::Flat => ArrowStyle2D::Flat,
            Style2d::Streamline => ArrowStyle2D::Streamline,
        },
        arrow_params: ArrowParams {
            head_length: a.head_length.unwrap_or(defaults.head_length),
            head_width: a.head_width.unwrap_or(defaults.head_width),
            shaft_radius: a.shaft_radius.unwrap_or(defaults.shaft_radius),
            sides: a.sides.unwrap_or(defaults.sides),
        },
        streamline_seed_mode: match c.streamline_seed_mode.unwrap_or(SeedMode::Auto) {
            SeedMode::Auto => StreamlineSeedMode::Auto,
            SeedMode::Click => StreamlineSeedMode::Click,
        },
        plane,
        particle_count: c.particle_count.unwrap_or(state.particle_count),
        particle_size: c.particle_size.unwrap_or(state.particle_size),
        particle_appearance,
        camera,
        references: ReferencesOptions {
            grid_and_domain_box: refs.grid_and_domain_box.unwrap_or(state.show_grid),
            world_axes: refs.world_axes.unwrap_or(state.show_coordinate_system),
        },
        background: c.background.unwrap_or([27, 27, 27, 255]),
        seed: c.seed.unwrap_or(0x123456789ABCDEF0),
        start_time_seconds: c.start_time_seconds.unwrap_or(0.0),
        duration_seconds: c.duration_seconds.unwrap_or(0.0),
        fixed_step_seconds: c.fixed_step_seconds.unwrap_or(1.0 / 60.0),
    };
    validate_effective(&o)?;
    Ok(o)
}

fn formula_field(src: &str) -> Result<FieldExpr, CliError> {
    formula(src)
}

pub fn validate_effective(o: &CliOptions) -> Result<(), CliError> {
    integer("width", Some(o.width), 1, 4096)?;
    integer("height", Some(o.height), 1, 4096)?;
    if u64::from(o.width) * u64::from(o.height) > 16_777_216 {
        return Err(CliError::Validation(
            "pixel count exceeds 16,777,216".into(),
        ));
    }
    let ext = output_extension(&o.output)?;
    let mut state = State::new(o.mode == Mode::ThreeD);
    formula_field(&o.formula)?;
    state.set_formula(o.formula.clone());
    integer(
        "density",
        Some(o.density),
        4,
        if state.is_3d() { 40 } else { 200 },
    )?;
    state.set_density(o.density);
    triple("domain.min", Some(o.domain.min.to_array()), true)?;
    triple("domain.max", Some(o.domain.max.to_array()), true)?;
    if !o.domain.min.cmplt(o.domain.max).all() {
        return Err(CliError::Validation(
            "domain min must be less than max".into(),
        ));
    }
    if o.plane.index >= o.plane.axis.count(state.plane_grid()) {
        return Err(CliError::Validation(
            "plane.index exceeds final grid axis".into(),
        ));
    }
    let c = CliConfig {
        arrow_scale: Some(o.arrow_scale),
        arrow_opacity: Some(o.arrow_opacity),
        plane_dim_opacity: Some(o.plane_dim_opacity),
        magnitude_scale: Some(o.magnitude_scale),
        proportional_exponent: Some(o.proportional_exponent),
        particle_count: Some(o.particle_count),
        particle_size: Some(o.particle_size),
        arrow_params: Some(ArrowParamsPatch {
            head_length: Some(o.arrow_params.head_length),
            head_width: Some(o.arrow_params.head_width),
            shaft_radius: Some(o.arrow_params.shaft_radius),
            sides: Some(o.arrow_params.sides),
        }),
        particle_appearance: Some(AppearancePatch {
            opacity: Some(o.particle_appearance.opacity),
            color_mode: None,
            fixed_color: None,
            trail_len: Some(o.particle_appearance.trail_len),
            trail_fade: Some(o.particle_appearance.trail_fade),
            trail_width: Some(o.particle_appearance.trail_width),
            loop_on_stagnation: Some(o.particle_appearance.loop_on_stagnation),
            stagnation_seconds: Some(o.particle_appearance.stagnation_seconds),
        }),
        start_time_seconds: Some(o.start_time_seconds),
        duration_seconds: Some(o.duration_seconds),
        fixed_step_seconds: Some(o.fixed_step_seconds),
        camera: Some(CameraPatch {
            target: Some(o.camera.target),
            kind: Some(o.camera.kind),
            position: Some(o.camera.position),
            yaw: Some(o.camera.yaw),
            pitch: Some(o.camera.pitch),
            distance: Some(o.camera.distance),
            fov_y_degrees: Some(o.camera.fov_y_degrees),
            ortho_zoom: Some(o.camera.ortho_zoom),
        }),
        color_range: o
            .color_range
            .map_or(ColorRangePatch::Null, ColorRangePatch::Value),
        ..Default::default()
    };
    validate_intrinsic_config(&c)?;
    if o.start_time_seconds + o.duration_seconds > 3600.0 {
        return Err(CliError::Validation(
            "start plus duration exceeds 3600 seconds".into(),
        ));
    }
    let cam = &o.camera;
    let kind = cam.kind;
    let explicit = cam.specified;
    let invalid = match (o.mode, kind) {
        (Mode::TwoD, _) => {
            kind != CameraKind::Orbit
                || explicit.position
                || explicit.yaw
                || explicit.pitch
                || explicit.distance
                || explicit.fov_y_degrees
        }
        (Mode::ThreeD, CameraKind::Orbit) => explicit.position || explicit.ortho_zoom,
        (Mode::ThreeD, CameraKind::Fly) => {
            explicit.distance || explicit.fov_y_degrees || explicit.ortho_zoom || explicit.target
        }
    };
    if invalid {
        return Err(CliError::Validation(
            "camera member is inapplicable to mode/kind".into(),
        ));
    }
    if o.mode == Mode::ThreeD && ext.eq_ignore_ascii_case("svg") {
        return Err(CliError::Validation(
            "3D SVG is unsupported; choose PNG".into(),
        ));
    }
    Ok(())
}
