//! Pure appearance profiles (issue #85 slice A1).
//!
//! An [`AppearanceProfile`] bundles the #82 viewport style, the #84 layer and
//! family overrides and the #83 colour mapping under a readable name. Profiles
//! apply to [`State`] atomically, and a strict versioned JSON document carries
//! them between scenes and machines.
//!
//! This module is deliberately pure: it does not touch the Appearance panel,
//! settings/on-disk storage, scene documents, the renderer or sampling.
//!
//! # Colour precedence
//!
//! [`ProfileColor`] keeps both a builtin [`Colormap`] and an optional
//! [`ColorRamp`]. When `ramp` is `Some`, it wins: [`State::color_mapping`]
//! reports the custom palette and the colormap field is informational. Both
//! fields survive a document round trip.

use serde::{Deserialize, Serialize};

use crate::color_mapping::ColorQuantity;
use crate::color_ramp::{ColorRamp, ColorStop, RampKind, Rgb, ScaleMode};
use crate::colormap::Colormap;
use crate::layer_style::LayerStyles;
use crate::viewmodel::State;
use crate::viewport_style::ViewportStyle;

/// The only appearance-profile document version this module writes or accepts.
pub const PROFILE_FORMAT_VERSION: u32 = 1;

/// Largest accepted profile name, in UTF-8 bytes.
pub const MAX_PROFILE_NAME_BYTES: usize = 48;

/// Name of the shipped default profile.
pub const DEFAULT_PROFILE_NAME: &str = "Dark laboratory";

/// The colour mapping part of a profile.
///
/// A present [`ColorRamp`] overrides [`Colormap`]; see the module-level colour
/// precedence note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileColor {
    /// Builtin colormap, used when [`Self::ramp`] is `None`.
    pub colormap: Colormap,
    /// Custom ramp override; when present it wins over [`Self::colormap`].
    pub ramp: Option<ColorRamp>,
    /// Value-to-ramp scaling.
    pub mode: ScaleMode,
    /// What the colour represents.
    pub quantity: ColorQuantity,
}

/// A named viewport, layer and colour appearance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceProfile {
    /// Trimmed, non-empty, at most [`MAX_PROFILE_NAME_BYTES`] UTF-8 bytes.
    pub name: String,
    /// Reference geometry and viewport background.
    pub viewport: ViewportStyle,
    /// Per-layer and per-family overrides.
    pub layer_styles: LayerStyles,
    /// Palette, scaling and quantity.
    pub color: ProfileColor,
}

impl AppearanceProfile {
    /// Rejects the first invalid name, viewport, layer style, ramp or mode.
    ///
    /// Validation never repairs a record; use [`Self::sanitized`] for that.
    pub fn validate(&self) -> Result<(), String> {
        validate_name(&self.name)?;
        self.viewport.validate()?;
        self.layer_styles.validate()?;
        if let Some(ramp) = &self.color.ramp {
            ramp.validate()?;
        }
        self.color.mode.validate()?;
        Ok(())
    }

    /// Repairs a profile read from storage instead of panicking.
    ///
    /// Names are trimmed, stripped of control characters and truncated on a
    /// character boundary; an empty result takes [`DEFAULT_PROFILE_NAME`].
    /// Viewport and layer fields use their own sanitizers, an invalid ramp
    /// falls back to [`ColorRamp::default`], and an invalid scale mode falls
    /// back to linear. The output always validates.
    pub fn sanitized(self) -> Self {
        Self {
            name: sanitize_name(&self.name),
            viewport: self.viewport.sanitized(),
            layer_styles: self.layer_styles.sanitized(),
            color: ProfileColor {
                colormap: self.color.colormap,
                ramp: self.color.ramp.map(ColorRamp::sanitized),
                mode: sanitized_mode(self.color.mode),
                quantity: self.color.quantity,
            },
        }
    }

    /// Capture the live appearance under `name`.
    ///
    /// Reads exactly the fields [`Self::apply`] writes, so applying the result
    /// reproduces the current look. The name is stored verbatim; validating it
    /// is the caller's responsibility (a profile created here still validates
    /// for any valid name).
    pub fn capture(name: impl Into<String>, state: &State) -> Self {
        Self {
            name: name.into(),
            viewport: state.viewport_style,
            layer_styles: *state.layer_styles(),
            color: ProfileColor {
                colormap: state.colormap,
                ramp: state.color_ramp.clone(),
                mode: state.color_mode,
                quantity: state.color_quantity,
            },
        }
    }

    /// Validates the whole profile, then applies it to the appearance fields.
    ///
    /// Formula, parameters, sources, camera, layout, UI preferences and every
    /// other session field are untouched. Validation happens before the first
    /// mutation; if a later check ever fails, the previous appearance fields
    /// are restored. A real change requests one scene rebuild, and a new
    /// colormap or ramp also bumps the plot generation so geometry baked with
    /// the previous colours is rebuilt.
    ///
    /// The rollback snapshot deliberately omits `plot.generation`: once
    /// [`Self::apply_validated`] starts mutating it cannot fail (every check
    /// runs first), so the generation bump never needs undoing.
    pub fn apply(&self, state: &mut State) -> Result<(), String> {
        self.validate()?;

        let previous = AppearanceSnapshot::capture(state);
        let result = self.apply_validated(state);
        if let Err(error) = result {
            previous.restore(state);
            return Err(error);
        }

        if previous.differs_from(state) {
            state.mark_dirty();
        }
        Ok(())
    }

    fn apply_validated(&self, state: &mut State) -> Result<(), String> {
        // Re-check the pieces this method writes so the rollback contract is
        // self-contained rather than relying only on `validate`.
        self.viewport.validate()?;
        self.layer_styles.validate()?;
        if let Some(ramp) = &self.color.ramp {
            ramp.validate()?;
        }
        self.color.mode.validate()?;

        // A new palette or ramp invalidates geometry baked with the previous
        // colours, exactly as `State::set_colormap` / `State::set_color_ramp`
        // do. This path assigns both fields directly (it must not call
        // `set_colormap`, which clears the ramp override), so it mirrors the
        // setters' single bump itself.
        let colors_changed =
            state.colormap != self.color.colormap || state.color_ramp != self.color.ramp;

        state.viewport_style = self.viewport;
        state.layer_styles = self.layer_styles;
        // Assign the colormap before the ramp: `set_colormap` is what clears an
        // override, and this path deliberately keeps both fields.
        state.colormap = self.color.colormap;
        state.color_ramp = self.color.ramp.clone();
        state.color_mode = self.color.mode;
        state.color_quantity = self.color.quantity;
        if colors_changed {
            state.plot.generation += 1;
        }
        Ok(())
    }
}

/// The appearance fields [`AppearanceProfile::apply`] owns.
struct AppearanceSnapshot {
    viewport: ViewportStyle,
    layer_styles: LayerStyles,
    colormap: Colormap,
    ramp: Option<ColorRamp>,
    mode: ScaleMode,
    quantity: ColorQuantity,
}

impl AppearanceSnapshot {
    fn capture(state: &State) -> Self {
        Self {
            viewport: state.viewport_style,
            layer_styles: state.layer_styles,
            colormap: state.colormap,
            ramp: state.color_ramp.clone(),
            mode: state.color_mode,
            quantity: state.color_quantity,
        }
    }

    fn restore(self, state: &mut State) {
        state.viewport_style = self.viewport;
        state.layer_styles = self.layer_styles;
        state.colormap = self.colormap;
        state.color_ramp = self.ramp;
        state.color_mode = self.mode;
        state.color_quantity = self.quantity;
    }

    fn differs_from(&self, state: &State) -> bool {
        self.viewport != state.viewport_style
            || self.layer_styles != state.layer_styles
            || self.colormap != state.colormap
            || self.ramp != state.color_ramp
            || self.mode != state.color_mode
            || self.quantity != state.color_quantity
    }
}

/// A strict, versioned JSON document holding appearance profiles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileDocument {
    /// Document format version; must be [`PROFILE_FORMAT_VERSION`].
    pub version: u32,
    /// Profiles in document order.
    pub profiles: Vec<AppearanceProfile>,
}

impl ProfileDocument {
    /// Validates the version, every profile and the uniqueness of names.
    fn validate(&self) -> Result<(), String> {
        if self.version != PROFILE_FORMAT_VERSION {
            return Err(format!(
                "profile version {} is not supported; expected version {}",
                self.version, PROFILE_FORMAT_VERSION
            ));
        }
        let mut seen = std::collections::HashSet::with_capacity(self.profiles.len());
        for profile in &self.profiles {
            profile.validate()?;
            if !seen.insert(profile.name.as_str()) {
                return Err(format!(
                    "duplicate profile name {:?}; profile names must be unique",
                    profile.name
                ));
            }
        }
        Ok(())
    }

    /// Serializes a validated document as compact JSON with pinned field order:
    /// `version`, `profiles`; per profile `name`, `viewport`, `layer_styles`,
    /// `color`; per colour `colormap`, `ramp`, `mode`, `quantity`.
    pub fn to_json(&self) -> Result<String, String> {
        self.validate()
            .map_err(|error| format!("invalid appearance profiles: {error}"))?;
        serde_json::to_string(self)
            .map_err(|error| format!("invalid appearance profile JSON: {error}"))
    }

    /// Parses and validates a document, rejecting unknown fields.
    ///
    /// The version is checked from a minimal probe before the profiles are
    /// deserialized, so a future document reports its version even when the
    /// profile body would not parse. Semantic failures use the
    /// `invalid appearance profiles: ` prefix; serde failures use
    /// `invalid appearance profile JSON: `.
    pub fn from_json(text: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct VersionProbe {
            version: u32,
        }

        let probe: VersionProbe = serde_json::from_str(text)
            .map_err(|error| format!("invalid appearance profile JSON: {error}"))?;
        if probe.version != PROFILE_FORMAT_VERSION {
            return Err(format!(
                "invalid appearance profiles: profile version {} is not supported; \
                 expected version {PROFILE_FORMAT_VERSION}",
                probe.version
            ));
        }

        let document: Self = serde_json::from_str(text)
            .map_err(|error| format!("invalid appearance profile JSON: {error}"))?;
        document
            .validate()
            .map_err(|error| format!("invalid appearance profiles: {error}"))?;
        Ok(document)
    }
}

/// The shipped looks, in display order.
///
/// `Dark laboratory` is first and is identical to the current default
/// appearance. The editor themes convert their sRGB hex colours to the linear
/// RGB that [`ViewportStyle`] stores; their ramp stops stay sRGB bytes.
pub fn builtins() -> Vec<AppearanceProfile> {
    vec![
        AppearanceProfile {
            name: DEFAULT_PROFILE_NAME.to_owned(),
            viewport: ViewportStyle::default(),
            layer_styles: LayerStyles::default(),
            color: default_color(),
        },
        AppearanceProfile {
            name: "White paper".to_owned(),
            viewport: ViewportStyle::white_paper(),
            layer_styles: LayerStyles::default(),
            color: default_color(),
        },
        AppearanceProfile {
            name: "High contrast".to_owned(),
            viewport: ViewportStyle::high_contrast(),
            layer_styles: LayerStyles::default(),
            color: default_color(),
        },
        theme_profile(
            "Tokyo Night",
            ThemeColors {
                background: "#1a1b26",
                grid: "#2f334d",
                box_color: "#414868",
                axes: ["#f7768e", "#9ece6a", "#7aa2f7"],
                ramp: &["#7aa2f7", "#bb9af7", "#7dcfff"],
            },
        ),
        theme_profile(
            "Catppuccin Mocha",
            ThemeColors {
                background: "#1e1e2e",
                grid: "#313244",
                box_color: "#45475a",
                axes: ["#f38ba8", "#a6e3a1", "#89b4fa"],
                ramp: &["#89b4fa", "#cba6f7", "#f5c2e7"],
            },
        ),
        theme_profile(
            "Catppuccin Latte",
            ThemeColors {
                background: "#eff1f5",
                grid: "#dce0e8",
                box_color: "#bcc0cc",
                axes: ["#d20f39", "#40a02b", "#1e66f5"],
                ramp: &["#1e66f5", "#8839ef", "#ea76cb"],
            },
        ),
        theme_profile(
            "Nord",
            ThemeColors {
                background: "#2e3440",
                grid: "#3b4252",
                box_color: "#434c5e",
                axes: ["#bf616a", "#a3be8c", "#81a1c1"],
                ramp: &["#5e81ac", "#88c0d0", "#8fbcbb"],
            },
        ),
        theme_profile(
            "Gruvbox Dark",
            ThemeColors {
                background: "#282828",
                grid: "#3c3836",
                box_color: "#504945",
                axes: ["#fb4934", "#b8bb26", "#83a598"],
                ramp: &["#d65d0e", "#fabd2f", "#b8bb26"],
            },
        ),
        theme_profile(
            "Solarized Light",
            ThemeColors {
                background: "#fdf6e3",
                grid: "#eee8d5",
                box_color: "#93a1a1",
                axes: ["#dc322f", "#859900", "#268bd2"],
                ramp: &["#268bd2", "#2aa198", "#859900"],
            },
        ),
        // Seaborn's categorical palettes: the six palette colours drive the
        // ramp, with a shared dark neutral viewport and the first three colours
        // as axes. Teaching approximations, not byte-exact reproductions.
        seaborn_categorical(
            "Seaborn Deep",
            &[
                "#4C72B0", "#DD8452", "#55A868", "#C44E52", "#8172B3", "#937860",
            ],
        ),
        seaborn_categorical(
            "Seaborn Muted",
            &[
                "#4878D0", "#EE854A", "#6ACC64", "#D65F5F", "#956CB4", "#8C613C",
            ],
        ),
        seaborn_categorical(
            "Seaborn Pastel",
            &[
                "#A1C9F4", "#FFB482", "#8DE5A1", "#FF9F9B", "#D0BBFF", "#DEBB9B",
            ],
        ),
        seaborn_categorical(
            "Seaborn Bright",
            &[
                "#023EFF", "#FF7C00", "#1AC938", "#E8000B", "#8B2BE2", "#9F4800",
            ],
        ),
        seaborn_categorical(
            "Seaborn Dark",
            &[
                "#001C7F", "#B1400D", "#12711C", "#8C0800", "#591E71", "#592F0D",
            ],
        ),
        seaborn_categorical(
            "Seaborn Colorblind",
            &[
                "#0173B2", "#DE8F05", "#029E73", "#D55E00", "#CC78BC", "#CA9161",
            ],
        ),
        // Seaborn sequential approximations, dark to light. On the shared dark
        // neutral viewport the light end of each ramp supplies the axes, so the
        // reference lines stay legible.
        seaborn_sequential(
            "Seaborn Rocket",
            &["#03051A", "#781C81", "#C33D5B", "#EF8F59", "#FAE6C6"],
        ),
        seaborn_sequential(
            "Seaborn Mako",
            &["#0B0405", "#21386F", "#2A7899", "#63C3B4", "#DEF5E5"],
        ),
        seaborn_sequential(
            "Seaborn Crest",
            &["#0B1D2A", "#21466B", "#3E7C8C", "#7FB69E", "#C7E9C0"],
        ),
        seaborn_sequential(
            "Seaborn Flare",
            &["#1B0C41", "#6A1F7A", "#C33D5B", "#F08A62", "#FCE3C3"],
        ),
        // Popular editor schemes with matching viewport colours and honest
        // four-stop ramps.
        theme_profile(
            "Dracula",
            ThemeColors {
                background: "#282a36",
                grid: "#44475a",
                box_color: "#6272a4",
                axes: ["#ff5555", "#50fa7b", "#8be9fd"],
                ramp: &["#ff79c6", "#bd93f9", "#8be9fd", "#50fa7b"],
            },
        ),
        theme_profile(
            "One Dark",
            ThemeColors {
                background: "#282c34",
                grid: "#3b4048",
                box_color: "#565c64",
                axes: ["#e06c75", "#98c379", "#61afef"],
                ramp: &["#61afef", "#c678dd", "#56b6c2", "#98c379"],
            },
        ),
        theme_profile(
            "Monokai",
            ThemeColors {
                background: "#272822",
                grid: "#3e3d32",
                box_color: "#75715e",
                axes: ["#f92672", "#a6e22e", "#66d9ef"],
                ramp: &["#f92672", "#fd971f", "#e6db74", "#a6e22e"],
            },
        ),
        theme_profile(
            "Solarized Dark",
            ThemeColors {
                background: "#002b36",
                grid: "#073642",
                box_color: "#586e75",
                axes: ["#dc322f", "#859900", "#268bd2"],
                ramp: &["#268bd2", "#2aa198", "#b58900", "#cb4b16"],
            },
        ),
        theme_profile(
            "Gruvbox Light",
            ThemeColors {
                background: "#fbf1c7",
                grid: "#ebdbb2",
                box_color: "#bdae93",
                axes: ["#cc241d", "#98971a", "#458588"],
                ramp: &["#b16286", "#d79921", "#458588", "#689d6a"],
            },
        ),
    ]
}

/// A seaborn categorical look: the six palette colours in order with the
/// shared dark neutral viewport and the first three colours as axes.
fn seaborn_categorical(name: &str, palette: &'static [&'static str]) -> AppearanceProfile {
    debug_assert_eq!(palette.len(), 6, "categorical palettes have six colours");
    theme_profile(
        name,
        ThemeColors {
            background: "#1c1c1c",
            grid: "#3a3a3a",
            box_color: "#555555",
            axes: [palette[0], palette[1], palette[2]],
            ramp: palette,
        },
    )
}

/// A seaborn sequential look: a dark-to-light ramp on the shared dark neutral
/// viewport, with the light end of the ramp as the axes.
fn seaborn_sequential(name: &str, ramp: &'static [&'static str]) -> AppearanceProfile {
    debug_assert!(
        ramp.len() >= 3,
        "sequential ramps have at least three stops"
    );
    let count = ramp.len();
    theme_profile(
        name,
        ThemeColors {
            background: "#1c1c1c",
            grid: "#3a3a3a",
            box_color: "#555555",
            axes: [ramp[count - 3], ramp[count - 2], ramp[count - 1]],
            ramp,
        },
    )
}

fn default_color() -> ProfileColor {
    ProfileColor {
        colormap: Colormap::Viridis,
        ramp: None,
        mode: ScaleMode::Linear,
        quantity: ColorQuantity::Magnitude,
    }
}

/// One theme's sRGB hex values.
struct ThemeColors {
    background: &'static str,
    grid: &'static str,
    box_color: &'static str,
    axes: [&'static str; 3],
    /// Two to sixteen sRGB hex stops sampled at evenly spaced positions.
    ramp: &'static [&'static str],
}

fn theme_profile(name: &str, colors: ThemeColors) -> AppearanceProfile {
    let defaults = ViewportStyle::default();
    AppearanceProfile {
        name: name.to_owned(),
        viewport: ViewportStyle {
            background: linear_rgb(colors.background),
            grid_color: linear_rgba(colors.grid),
            box_color: linear_rgba(colors.box_color),
            axis_x_color: linear_rgba(colors.axes[0]),
            axis_y_color: linear_rgba(colors.axes[1]),
            axis_z_color: linear_rgba(colors.axes[2]),
            // Widths stay at the current defaults.
            ..defaults
        },
        layer_styles: LayerStyles::default(),
        color: ProfileColor {
            // The ramp wins, but the builtin field stays Viridis for a
            // meaningful document if the ramp is later cleared.
            colormap: Colormap::Viridis,
            ramp: Some(ramp_from_hex(colors.ramp)),
            mode: ScaleMode::Linear,
            quantity: ColorQuantity::Magnitude,
        },
    }
}

/// Build a sequential ramp from any 2–16 hex stops, evenly spaced from `0.0`
/// to `1.0` inclusive. Every shipped ramp is long enough to validate.
fn ramp_from_hex(colors: &[&str]) -> ColorRamp {
    debug_assert!(
        (ColorRamp::MIN_STOPS..=ColorRamp::MAX_STOPS).contains(&colors.len()),
        "shipped ramps carry 2 to {} stops",
        ColorRamp::MAX_STOPS
    );
    let last = colors.len().saturating_sub(1).max(1) as f32;
    ColorRamp {
        stops: colors
            .iter()
            .enumerate()
            .map(|(index, text)| ColorStop {
                position: index as f32 / last,
                color: parse_hex(text),
            })
            .collect(),
        reversed: false,
        kind: RampKind::Sequential,
    }
}

fn parse_hex(text: &str) -> Rgb {
    Rgb::from_hex(text).expect("shipped theme colours are valid six-digit hex")
}

fn linear_rgb(text: &str) -> [f32; 3] {
    let color = parse_hex(text);
    color.0.map(srgb_to_linear)
}

fn linear_rgba(text: &str) -> [f32; 4] {
    let [red, green, blue] = linear_rgb(text);
    [red, green, blue, 1.0]
}

/// Converts one 8-bit sRGB channel to linear RGB.
fn srgb_to_linear(byte: u8) -> f32 {
    let channel = f32::from(byte) / 255.0;
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("profile name must not be empty".to_owned());
    }
    if name.trim() != name {
        return Err("profile name must be trimmed".to_owned());
    }
    if name.len() > MAX_PROFILE_NAME_BYTES {
        return Err(format!(
            "profile name is {} bytes; the limit is {MAX_PROFILE_NAME_BYTES}",
            name.len()
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("profile name must not contain control characters".to_owned());
    }
    Ok(())
}

fn sanitize_name(name: &str) -> String {
    let mut cleaned: String = name
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    cleaned = cleaned.trim().to_owned();
    if cleaned.is_empty() {
        return DEFAULT_PROFILE_NAME.to_owned();
    }
    if cleaned.len() > MAX_PROFILE_NAME_BYTES {
        let mut end = MAX_PROFILE_NAME_BYTES;
        while !cleaned.is_char_boundary(end) {
            end -= 1;
        }
        cleaned.truncate(end);
        cleaned = cleaned.trim_end().to_owned();
        if cleaned.is_empty() {
            return DEFAULT_PROFILE_NAME.to_owned();
        }
    }
    cleaned
}

fn sanitized_mode(mode: ScaleMode) -> ScaleMode {
    match mode {
        ScaleMode::Linear => ScaleMode::Linear,
        ScaleMode::SignedLog { half_width } => {
            if !half_width.is_finite() || half_width <= 0.0 {
                ScaleMode::Linear
            } else {
                ScaleMode::SignedLog {
                    half_width: half_width.min(1.0e6),
                }
            }
        }
    }
}
