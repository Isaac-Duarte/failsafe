use bytemuck::{Pod, Zeroable};
use tracing::debug;

const WORKGROUP_SIZE: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    src_width: u32,
    src_height: u32,
    dst_width: u32,
    dst_height: u32,
}

pub struct GpuPreprocessor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    max_width: u32,
    cached_src_width: u32,
    cached_src_height: u32,
    cached_dst_width: u32,
    cached_dst_height: u32,
    input_texture: Option<wgpu::Texture>,
    output_buffer: Option<wgpu::Buffer>,
    staging_buffer: Option<wgpu::Buffer>,
    params_buffer: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,
}

impl GpuPreprocessor {
    pub fn new(max_width: u32) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok_or_else(|| "no compatible GPU adapter found".to_owned())?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("failsafe-screen-preprocessor"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .map_err(|error| error.to_string())?;

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("preprocess_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("preprocess_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("preprocess_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        debug!(max_width, "GPU screen preprocessor initialized");

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            max_width,
            cached_src_width: 0,
            cached_src_height: 0,
            cached_dst_width: 0,
            cached_dst_height: 0,
            input_texture: None,
            output_buffer: None,
            staging_buffer: None,
            params_buffer: None,
            bind_group: None,
        })
    }

    pub fn preprocess_rgba_to_rgb(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(Vec<u8>, u32, u32), String> {
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(format!(
                "rgba buffer length {} does not match {width}x{height}",
                rgba.len()
            ));
        }

        let (dst_width, dst_height) = output_dimensions(width, height, self.max_width);
        if dst_width == width && dst_height == height {
            return Ok((rgba_to_rgb_cpu(rgba), width, height));
        }

        self.ensure_resources(width, height, dst_width, dst_height)?;

        let input_texture = self.input_texture.as_ref().expect("input texture");
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: input_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let params = Params {
            src_width: width,
            src_height: height,
            dst_width,
            dst_height,
        };
        self.queue.write_buffer(
            self.params_buffer.as_ref().expect("params buffer"),
            0,
            bytemuck::bytes_of(&params),
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("preprocess_encoder"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("preprocess_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, self.bind_group.as_ref().expect("bind group"), &[]);
            let groups_x = dst_width.div_ceil(WORKGROUP_SIZE);
            let groups_y = dst_height.div_ceil(WORKGROUP_SIZE);
            pass.dispatch_workgroups(groups_x, groups_y, 1);
        }

        let output_buffer = self.output_buffer.as_ref().expect("output buffer");
        let staging_buffer = self.staging_buffer.as_ref().expect("staging buffer");
        let output_bytes = (dst_width as usize) * (dst_height as usize) * 4;
        encoder.copy_buffer_to_buffer(output_buffer, 0, staging_buffer, 0, output_bytes as u64);
        self.queue.submit(Some(encoder.finish()));

        let buffer_slice = staging_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        receiver
            .recv()
            .map_err(|_| "GPU readback channel closed".to_owned())?
            .map_err(|error| error.to_string())?;

        let mapped = buffer_slice.get_mapped_range();
        let packed = bytemuck::cast_slice::<u8, u32>(&mapped[..output_bytes]);
        let rgb = unpack_rgb(packed, dst_width, dst_height);
        drop(mapped);
        staging_buffer.unmap();

        Ok((rgb, dst_width, dst_height))
    }

    fn ensure_resources(
        &mut self,
        src_width: u32,
        src_height: u32,
        dst_width: u32,
        dst_height: u32,
    ) -> Result<(), String> {
        if self.cached_src_width == src_width
            && self.cached_src_height == src_height
            && self.cached_dst_width == dst_width
            && self.cached_dst_height == dst_height
        {
            return Ok(());
        }

        let input_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preprocess_input"),
            size: wgpu::Extent3d {
                width: src_width,
                height: src_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let output_pixel_count = dst_width as u64 * dst_height as u64;
        let output_bytes = output_pixel_count * 4;
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preprocess_output"),
            size: output_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preprocess_staging"),
            size: output_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preprocess_params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let input_view = input_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("preprocess_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output_buffer.as_entire_binding(),
                },
            ],
        });

        self.cached_src_width = src_width;
        self.cached_src_height = src_height;
        self.cached_dst_width = dst_width;
        self.cached_dst_height = dst_height;
        self.input_texture = Some(input_texture);
        self.output_buffer = Some(output_buffer);
        self.staging_buffer = Some(staging_buffer);
        self.params_buffer = Some(params_buffer);
        self.bind_group = Some(bind_group);

        Ok(())
    }
}

fn output_dimensions(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    if width <= max_width {
        return (width, height);
    }
    let dst_width = max_width;
    let dst_height = ((height as u64 * max_width as u64) / width as u64)
        .try_into()
        .unwrap_or(height);
    (dst_width, dst_height.max(1))
}

fn rgba_to_rgb_cpu(rgba: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for pixel in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
    }
    rgb
}

fn unpack_rgb(packed: &[u32], width: u32, height: u32) -> Vec<u8> {
    let pixel_count = width as usize * height as usize;
    let mut rgb = Vec::with_capacity(pixel_count * 3);
    for value in packed.iter().take(pixel_count) {
        rgb.push(((value >> 16) & 0xFF) as u8);
        rgb.push(((value >> 8) & 0xFF) as u8);
        rgb.push((value & 0xFF) as u8);
    }
    rgb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_dimensions_preserves_aspect_ratio() {
        let (w, h) = output_dimensions(3840, 2160, 1920);
        assert_eq!(w, 1920);
        assert_eq!(h, 1080);
    }

    #[test]
    fn output_dimensions_skips_downscale_when_small_enough() {
        let (w, h) = output_dimensions(1280, 720, 1920);
        assert_eq!((w, h), (1280, 720));
    }

    #[test]
    fn gpu_preprocessor_downscales_large_frame_when_available() {
        let Ok(mut gpu) = GpuPreprocessor::new(1920) else {
            return;
        };

        let width = 3840u32;
        let height = 2160u32;
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                let index = ((y * width + x) * 4) as usize;
                rgba[index] = (x % 256) as u8;
                rgba[index + 1] = (y % 256) as u8;
                rgba[index + 2] = 128;
                rgba[index + 3] = 255;
            }
        }

        let (rgb, out_width, out_height) = gpu
            .preprocess_rgba_to_rgb(&rgba, width, height)
            .expect("gpu preprocess");
        assert_eq!((out_width, out_height), (1920, 1080));
        assert_eq!(rgb.len(), 1920 * 1080 * 3);
        assert_eq!(rgb[2], 128);
        assert!(rgb[0] < 2);
        assert!(rgb[1] < 2);
    }
}
