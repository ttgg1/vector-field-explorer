// GPU Instanced rendering helper
use wgpu::util::DeviceExt;
use wgpu::{Buffer, BufferDescriptor, BufferUsages, Device};

/// Holds a buffer of instance data for instanced drawing.
pub struct Instancer {
    pub buffer: Buffer,
    /// Number of instances currently in the buffer.
    pub count: u32,
}

impl Instancer {
    /// Create a new empty instance buffer.
    pub fn new(device: &Device, max_instances: u32) -> Self {
        let size = (max_instances * std::mem::size_of::<[f32; 4]>() as u32) as wgpu::BufferAddress;
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("instancer buffer"),
            size,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buffer, count: 0 }
    }

    /// Update instance data.
    pub fn update(&mut self, device: &Device, queue: &wgpu::Queue, data: &[f32]) {
        let size = (data.len() * std::mem::size_of::<f32>() as usize) as wgpu::BufferAddress;
        if size > 0 {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(data));
            self.count = (data.len() / 4) as u32; // assuming vec4 per instance
        }
    }
}
