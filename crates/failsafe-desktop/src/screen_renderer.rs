use std::io::Cursor;
use std::sync::Mutex;

use bytemuck::{Pod, Zeroable};
use image::ImageReader;
use tauri::WebviewWindow;

#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
#[repr(C)]
struct ViewportUniform {
    bounds: [f32; 4],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViewportRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ViewportRect {
    fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }
}

pub struct ScreenRenderer {
    queue: wgpu::Queue,
    device: wgpu::Device,
    sampler: wgpu::Sampler,
    surface: wgpu::Surface<'static>,
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    config: Mutex<wgpu::SurfaceConfiguration>,
    surface_ops: Mutex<()>,
    pending_frame: Mutex<Option<PendingFrame>>,
    viewport: Mutex<Option<ViewportRect>>,
    active: Mutex<bool>,
}

#[derive(Clone)]
struct PendingFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl ScreenRenderer {
    pub async fn new(window: WebviewWindow) -> Result<Self, String> {
        let size = window
            .inner_size()
            .map_err(|error| error.to_string())?;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| error.to_string())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| "no compatible GPU adapter found".to_owned())?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("failsafe-screen-viewer"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("screen_viewer_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("screen_viewer_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .unwrap_or(capabilities.formats[0]);
        let alpha_mode = capabilities
            .alpha_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::CompositeAlphaMode::PreMultiplied)
            .or_else(|| capabilities.alpha_modes.first().copied())
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("screen_viewer_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: swapchain_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Ok(Self {
            queue,
            device,
            sampler,
            surface,
            render_pipeline,
            bind_group_layout,
            config: Mutex::new(config),
            surface_ops: Mutex::new(()),
            pending_frame: Mutex::new(None),
            viewport: Mutex::new(None),
            active: Mutex::new(false),
        })
    }

    pub fn set_active(&self, active: bool) {
        *self.active.lock().expect("active lock") = active;
    }

    /// Must run on the main thread (surface acquire/present).
    pub fn deactivate_and_clear(&self) {
        *self.active.lock().expect("active lock") = false;
        *self.pending_frame.lock().expect("pending frame lock") = None;
        let _ = self.clear();
    }

    pub fn set_viewport(&self, viewport: ViewportRect) {
        if viewport.is_valid() {
            *self.viewport.lock().expect("viewport lock") = Some(viewport);
        }
    }

    pub fn resize(&self, width: u32, height: u32) {
        let mut config = self.config.lock().expect("surface config lock");
        if width == 0 || height == 0 {
            return;
        }
        config.width = width;
        config.height = height;
        self.surface.configure(&self.device, &config);
    }

    pub fn submit_jpeg_and_render(&self, jpeg: &[u8]) -> Result<(), String> {
        let image = ImageReader::new(Cursor::new(jpeg))
            .with_guessed_format()
            .map_err(|error| error.to_string())?
            .decode()
            .map_err(|error| error.to_string())?;
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();
        *self.pending_frame.lock().expect("pending frame lock") = Some(PendingFrame {
            width,
            height,
            rgba: rgba.into_raw(),
        });
        self.render()
    }

    pub fn render(&self) -> Result<(), String> {
        if !*self.active.lock().expect("active lock") {
            return Ok(());
        }

        let viewport = *self
            .viewport
            .lock()
            .expect("viewport lock")
            .as_ref()
            .ok_or_else(|| "viewport bounds not set".to_owned())?;
        if !viewport.is_valid() {
            return Ok(());
        }

        let frame = self
            .pending_frame
            .lock()
            .expect("pending frame lock")
            .clone();
        let Some(frame) = frame else {
            return Ok(());
        };

        let config = self.config.lock().expect("surface config lock");
        let viewport_uniform = ViewportUniform {
            bounds: compute_draw_bounds(
                viewport,
                config.width,
                config.height,
                frame.width,
                frame.height,
            ),
        };
        drop(config);

        let viewport_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen_viewer_viewport"),
            size: std::mem::size_of::<ViewportUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(
            &viewport_buffer,
            0,
            bytemuck::bytes_of(&viewport_uniform),
        );

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("screen_viewer_frame"),
            size: wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * frame.width),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("screen_viewer_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: viewport_buffer.as_entire_binding(),
                },
            ],
        });

        let _surface_guard = self
            .surface_ops
            .lock()
            .map_err(|_| "surface lock poisoned".to_owned())?;

        let output = match self.acquire_surface_texture() {
            Ok(output) => output,
            Err(None) => return Ok(()),
            Err(Some(error)) => return Err(error),
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("screen_viewer_encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("screen_viewer_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            render_pass.set_scissor_rect(
                viewport.x,
                viewport.y,
                viewport.width,
                viewport.height,
            );
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }

    fn clear(&self) -> Result<(), String> {
        let _surface_guard = self
            .surface_ops
            .lock()
            .map_err(|_| "surface lock poisoned".to_owned())?;

        let output = match self.acquire_surface_texture() {
            Ok(output) => output,
            Err(None) => return Ok(()),
            Err(Some(error)) => return Err(error),
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("screen_viewer_clear"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("screen_viewer_clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }

    fn acquire_surface_texture(
        &self,
    ) -> Result<wgpu::SurfaceTexture, Option<String>> {
        match self.surface.get_current_texture() {
            Ok(texture) => Ok(texture),
            Err(wgpu::SurfaceError::Outdated) => {
                let config = self.config.lock().expect("surface config lock");
                self.surface.configure(&self.device, &config);
                Err(None)
            }
            Err(wgpu::SurfaceError::Timeout) => Err(None),
            Err(error) => Err(Some(error.to_string())),
        }
    }
}

fn compute_draw_bounds(
    viewport: ViewportRect,
    surface_width: u32,
    surface_height: u32,
    frame_width: u32,
    frame_height: u32,
) -> [f32; 4] {
    let frame_aspect = frame_width as f32 / frame_height as f32;
    let viewport_width = viewport.width as f32;
    let viewport_height = viewport.height as f32;
    let viewport_aspect = viewport_width / viewport_height;

    let (draw_width, draw_height) = if frame_aspect > viewport_aspect {
        (viewport_width, viewport_width / frame_aspect)
    } else {
        (viewport_height * frame_aspect, viewport_height)
    };

    let x = viewport.x as f32 + (viewport_width - draw_width) / 2.0;
    let y = viewport.y as f32 + (viewport_height - draw_height) / 2.0;

    let left = (x / surface_width as f32) * 2.0 - 1.0;
    let right = ((x + draw_width) / surface_width as f32) * 2.0 - 1.0;
    let top = y;
    let bottom = y + draw_height;
    let ndc_top = 1.0 - (top / surface_height as f32) * 2.0;
    let ndc_bottom = 1.0 - (bottom / surface_height as f32) * 2.0;

    [left, ndc_bottom, right, ndc_top]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_draw_bounds_letterboxes_tall_frame() {
        let bounds = compute_draw_bounds(
            ViewportRect {
                x: 100,
                y: 200,
                width: 800,
                height: 600,
            },
            1200,
            900,
            1920,
            1080,
        );
        assert!(bounds[0] < bounds[2]);
        assert!(bounds[1] < bounds[3]);
    }
}
