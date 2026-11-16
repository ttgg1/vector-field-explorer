//! Independent offscreen capture owner for scalar plots.
//!
//! Capture has its own pipelines, uniform/geometry buffers, color/depth targets
//! and readback buffer. It never touches the live [`PlotDrawState`] resources, so
//! a screenshot requested before the next live prepare cannot overwrite the
//! uniforms or target that a queued viewport draw will consume.
//!
//! Native and wasm share the pure preflight and submission structure; native
//! waits for the map and wasm completes through the browser event loop with no
//! main-thread blocking poll.

use eframe::wgpu;

use crate::{plot_camera::PlotFrame, plot_scene::PlotScene, screenshot};

#[cfg(not(target_arch = "wasm32"))]
use crate::renderer::OffscreenError;

use super::{PlotFormatCapabilities, PlotPassResources, PlotPipelines, record_plot_pass};

/// Bytes per readback row required by `copy_texture_to_buffer`.
const COPY_ROW_ALIGNMENT: u32 = 256;

/// Device limits that bound a capture request before any allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureLimits {
    pub max_texture_dimension_2d: u32,
    pub max_buffer_size: u64,
}

impl CaptureLimits {
    pub fn from_device(device: &wgpu::Device) -> Self {
        Self {
            max_texture_dimension_2d: device.limits().max_texture_dimension_2d,
            max_buffer_size: device.limits().max_buffer_size,
        }
    }
}

/// Reject an unsupported capture before creating a texture or buffer:
/// non-PNG-readable formats, zero/oversized dimensions and aligned-row or
/// readback-size overflow are errors, not validation panics.
pub fn validate_capture_extent(
    format: wgpu::TextureFormat,
    extent: [u32; 2],
    limits: CaptureLimits,
) -> Result<(), String> {
    screenshot::png_readback_check(format)?;
    let [width, height] = extent;
    if width == 0 || height == 0 {
        return Err("capture extent must be nonzero".into());
    }
    if width > limits.max_texture_dimension_2d || height > limits.max_texture_dimension_2d {
        return Err(format!(
            "capture extent {}x{} exceeds the device texture limit {}",
            width, height, limits.max_texture_dimension_2d
        ));
    }
    let row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| "capture row stride overflows".to_string())?;
    let padded = row_bytes
        .checked_add(COPY_ROW_ALIGNMENT - 1)
        .ok_or_else(|| "capture row alignment overflows".to_string())?
        / COPY_ROW_ALIGNMENT
        * COPY_ROW_ALIGNMENT;
    let readback_bytes = u64::from(padded)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "capture readback size overflows".to_string())?;
    if readback_bytes > limits.max_buffer_size {
        return Err(format!(
            "capture readback {readback_bytes} bytes exceeds the device buffer limit {}",
            limits.max_buffer_size
        ));
    }
    Ok(())
}

/// A capture submission owns all of its GPU resources until readback completes.
pub struct PlotCapture {
    format: wgpu::TextureFormat,
    pass: PlotPassResources,
}

impl PlotCapture {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        capabilities: PlotFormatCapabilities,
    ) -> Result<Self, String> {
        screenshot::png_readback_check(format)?;
        capabilities.validate_capture()?;
        let pipelines = PlotPipelines::new(device, format, 1);
        let pass = PlotPassResources::new(device, &pipelines);
        Ok(Self { format, pass })
    }

    /// Create the independent capture color/depth pair, encode the shared plot
    /// pass into a fresh encoder and submit it with a readback copy.
    #[allow(clippy::too_many_arguments)]
    fn submit(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &PlotScene,
        frame: PlotFrame,
        extent: [u32; 2],
        clear: [f64; 4],
    ) -> Result<wgpu::Buffer, String> {
        let [width, height] = extent;
        let limits = CaptureLimits::from_device(device);
        validate_capture_extent(self.format, extent, limits)?;

        self.pass.write_globals(queue, frame);
        self.pass.upload_scene(device, scene);

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vfe plot capture color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vfe plot capture depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        let padded_row = screenshot::padded_row_bytes(width);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vfe plot capture readback"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vfe plot capture encoder"),
        });
        record_plot_pass(
            &mut encoder,
            &self.pass,
            scene,
            &color_view,
            &depth_view,
            clear,
        );
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        Ok(readback)
    }

    /// Render the plot scene offscreen and read it back synchronously (native).
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &PlotScene,
        frame: PlotFrame,
        extent: [u32; 2],
        clear: [f64; 4],
    ) -> Result<Vec<u8>, OffscreenError> {
        let readback = self
            .submit(device, queue, scene, frame, extent, clear)
            .map_err(OffscreenError::Setup)?;

        let width = extent[0];
        let height = extent[1];
        let slice = readback.slice(..);
        let (mapped_tx, mapped_rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = mapped_tx.send(result);
        });
        let result = (|| {
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(crate::renderer::ReadbackError::Poll)?;
            crate::renderer::readback_map_status(
                mapped_rx
                    .recv()
                    .map_err(crate::renderer::ReadbackError::CallbackLost)?,
            )?;
            let pixels = {
                let mapped = slice
                    .get_mapped_range()
                    .map_err(crate::renderer::ReadbackError::Range)?;
                screenshot::strip_row_padding(&mapped, width, height)
            };
            screenshot::to_rgba(self.format, pixels).map_err(crate::renderer::ReadbackError::Decode)
        })();
        readback.unmap();
        result.map_err(OffscreenError::Readback)
    }

    /// Submit a capture and complete through the browser event loop (wasm).
    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::too_many_arguments)]
    pub fn render_async(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &PlotScene,
        frame: PlotFrame,
        extent: [u32; 2],
        clear: [f64; 4],
        completion: impl FnOnce(Result<Vec<u8>, String>) + Send + 'static,
    ) -> Result<(), String> {
        let readback = self.submit(device, queue, scene, frame, extent, clear)?;
        let buffer = readback.clone();
        let format = self.format;
        let width = extent[0];
        let height = extent[1];
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                completion(crate::renderer::readback_result(
                    result, &buffer, format, width, height,
                ));
            });
        Ok(())
    }
}
