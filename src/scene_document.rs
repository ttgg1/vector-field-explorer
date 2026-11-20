//! Versioned scene document (issue #90 slice B2a).
//!
//! A strict, versioned JSON form for a whole scene: the durable view record,
//! the shared parameter set and a serialisable camera pose. This is a pure
//! model: it does not import `State`, touch storage, or render anything.
//!
//! Import never repairs, clamps or defaults a field. A document that fails
//! validation is reported with the offending field named, so a hand-edited or
//! future file is rejected instead of silently changing what the user sees.

use serde::{Deserialize, Serialize};

use crate::camera::OrbitCamera;
use crate::parameters::ParameterSet;
use crate::persistence::ViewRecord;

/// The only scene document version this module writes or accepts.
pub const SCENE_FORMAT_VERSION: u32 = 1;

/// Longest accepted scene name, in UTF-8 bytes.
pub const MAX_SCENE_NAME_BYTES: usize = 64;

/// Inclusive lower bound for the 2D camera zoom.
const MIN_SCALE: f32 = 0.05;
/// Inclusive upper bound for the 2D camera zoom.
const MAX_SCALE: f32 = 50.0;
/// Inclusive lower bound for the orbit distance, world units.
const MIN_DISTANCE: f32 = 0.05;
/// Inclusive upper bound for the orbit distance, world units.
const MAX_DISTANCE: f32 = 1.0e4;
/// Inclusive lower bound for the vertical field of view, radians.
const MIN_FOV_Y: f32 = 1.0_f32.to_radians();
/// Inclusive upper bound for the vertical field of view, radians.
const MAX_FOV_Y: f32 = 179.0_f32.to_radians();

/// The camera pose stored in a [`SceneDocument`].
///
/// Plain arrays keep `glam` out of the persisted schema. The values mirror the
/// orbit camera's public fields plus the 2D pan/zoom pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneCamera {
    /// 2D pan, in world units.
    pub position: [f32; 2],
    /// 2D zoom factor.
    pub scale: f32,
    /// Orbit yaw, radians.
    pub yaw: f32,
    /// Orbit pitch, radians.
    pub pitch: f32,
    /// Orbit distance, world units.
    pub distance: f32,
    /// Orbit target point, world units.
    pub target: [f32; 3],
    /// Vertical field of view, radians.
    pub fov_y: f32,
    /// 2D orthographic zoom factor.
    pub ortho_zoom: f32,
}

impl Default for SceneCamera {
    fn default() -> Self {
        let orbit = OrbitCamera::default();
        Self {
            position: [0.0, 0.0],
            scale: 1.0,
            yaw: orbit.yaw,
            pitch: orbit.pitch,
            distance: orbit.distance,
            target: orbit.target.to_array(),
            fov_y: orbit.fov_y,
            ortho_zoom: orbit.ortho_zoom,
        }
    }
}

impl SceneCamera {
    /// Check every field and return the first failure, naming the field.
    ///
    /// Non-finite values are rejected before the range checks so a `NaN`
    /// cannot slip through a comparison.
    pub fn validate(&self) -> Result<(), String> {
        let [px, py] = self.position;
        let [tx, ty, tz] = self.target;
        for (field, value) in [
            ("position.x", px),
            ("position.y", py),
            ("scale", self.scale),
            ("yaw", self.yaw),
            ("pitch", self.pitch),
            ("distance", self.distance),
            ("target.x", tx),
            ("target.y", ty),
            ("target.z", tz),
            ("fov_y", self.fov_y),
            ("ortho_zoom", self.ortho_zoom),
        ] {
            if !value.is_finite() {
                return Err(format!("camera {field} must be finite"));
            }
        }

        check_range("scale", self.scale, MIN_SCALE, MAX_SCALE)?;
        check_range("distance", self.distance, MIN_DISTANCE, MAX_DISTANCE)?;
        check_range("fov_y", self.fov_y, MIN_FOV_Y, MAX_FOV_Y)?;
        check_range(
            "ortho_zoom",
            self.ortho_zoom,
            OrbitCamera::MIN_ORTHO_ZOOM,
            OrbitCamera::MAX_ORTHO_ZOOM,
        )?;
        Ok(())
    }
}

fn check_range(field: &str, value: f32, min: f32, max: f32) -> Result<(), String> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!("camera {field} {value} is outside {min}..={max}"))
    }
}

/// A complete, versioned scene document.
///
/// Field order is declaration order, so exports diff readably. Unknown fields
/// are rejected at the document and camera levels only; the nested `view` and
/// `parameters` records stay permissive/defaulted and are validated elsewhere.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneDocument {
    /// Document format version; must be [`SCENE_FORMAT_VERSION`].
    pub version: u32,
    /// Human-readable scene name.
    pub name: String,
    /// The durable view state; validated by `ViewRecord::restore`, not here.
    pub view: ViewRecord,
    /// Shared named parameters.
    pub parameters: ParameterSet,
    /// The saved camera pose.
    pub camera: SceneCamera,
}

impl SceneDocument {
    /// Build a document at the current format version.
    ///
    /// The name is stored exactly as given; validation, not capture, rejects
    /// an empty or over-long name.
    pub fn capture(
        name: impl Into<String>,
        view: ViewRecord,
        parameters: ParameterSet,
        camera: SceneCamera,
    ) -> Self {
        Self {
            version: SCENE_FORMAT_VERSION,
            name: name.into(),
            view,
            parameters,
            camera,
        }
    }

    /// Check the version, name, camera and parameters, first failure only.
    ///
    /// `view` is deliberately not re-validated: restoring a `ViewRecord` owns
    /// its own rules.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != SCENE_FORMAT_VERSION {
            return Err(format!(
                "scene version {} is unsupported; expected version {}",
                self.version, SCENE_FORMAT_VERSION
            ));
        }
        validate_name(&self.name)?;
        self.camera.validate()?;
        self.parameters.validate()?;
        Ok(())
    }

    /// Serialize the document as compact JSON.
    ///
    /// Never returns an empty string: a serialization failure is an `Err`.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|error| format!("invalid scene JSON: {error}"))
    }

    /// Parse and validate a scene document.
    ///
    /// Serde failures map to `invalid scene JSON: {error}`; validation
    /// failures map to `invalid scene: {error}`. Nothing is repaired.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let document: Self =
            serde_json::from_str(text).map_err(|error| format!("invalid scene JSON: {error}"))?;
        document
            .validate()
            .map_err(|error| format!("invalid scene: {error}"))?;
        Ok(document)
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("scene name must not be empty".to_string());
    }
    let bytes = name.len();
    if bytes > MAX_SCENE_NAME_BYTES {
        return Err(format!(
            "scene name is {bytes} bytes; at most {MAX_SCENE_NAME_BYTES} are allowed"
        ));
    }
    if name.chars().any(|character| character.is_ascii_control()) {
        return Err("scene name must not contain ASCII control characters".to_string());
    }
    Ok(())
}

/// Validate and serialize `scene` as a versioned JSON document.
pub fn export_scene(scene: &SceneDocument) -> Result<String, String> {
    scene
        .validate()
        .map_err(|error| format!("invalid scene: {error}"))?;
    scene.to_json()
}

/// Parse and validate a versioned scene document.
///
/// Composes [`SceneDocument::from_json`], which already validates.
pub fn import_scene(text: &str) -> Result<SceneDocument, String> {
    SceneDocument::from_json(text)
}
