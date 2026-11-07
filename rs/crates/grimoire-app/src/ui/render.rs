//! GPU side of the main window: the cell grid goes into an offscreen float texture, then the CRT pass
//! (bloom + scanlines) draws it to the window.

use super::font::{procedural, Atlas, Fonts};
use super::screen::Screen;
use super::theme::BACKGROUND;
use crate::scenes::fullscreen_pipeline;

const OFFSCREEN: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const ATLAS_SIZE: u32 = 1024;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    rect: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    kind: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GridUniforms {
    size: [f32; 2],
    atlas_size: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CrtUniforms {
    resolution: [f32; 2],
    texel: [f32; 2],
    direction: [f32; 2],
    time: f32,
    glow: f32,
    scale: f32,
    _pad: [f32; 7],
}

/// Per-frame effect inputs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Effects {
    pub time: f32,
    pub glow: f32,
}

struct Targets {
    size: (u32, u32),
    scene: wgpu::TextureView,
    /// Bind groups for the blur-across, blur-down and composite passes.
    groups: [wgpu::BindGroup; 3],
    half: [wgpu::TextureView; 2],
}

pub struct UiRenderer {
    pub fonts: Fonts,
    scale: f32,
    atlas: Atlas,
    atlas_texture: wgpu::Texture,
    grid_pipeline: wgpu::RenderPipeline,
    grid_group: wgpu::BindGroup,
    grid_uniforms: wgpu::Buffer,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    crt_layout: wgpu::BindGroupLayout,
    blur_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    crt_uniforms: [wgpu::Buffer; 3],
    sampler: wgpu::Sampler,
    targets: Option<Targets>,
    scratch: Vec<Instance>,
}

impl UiRenderer {
    pub fn new(device: &wgpu::Device, output: wgpu::TextureFormat, scale: f32) -> UiRenderer {
        let fonts = Fonts::new(scale);
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas"),
            size: wgpu::Extent3d { width: ATLAS_SIZE, height: ATLAS_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("atlas"), ..Default::default() });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("crt"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };

        // Grid pass.
        let grid_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid"),
            entries: &[uniform_entry(0), texture_entry(1), sampler_entry(2)],
        });
        let grid_uniforms = uniform_buffer(device, "grid uniforms", std::mem::size_of::<GridUniforms>());
        let atlas_view = atlas_texture.create_view(&Default::default());
        let grid_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid"),
            layout: &grid_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: grid_uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&nearest) },
            ],
        });
        let grid_module = device.create_shader_module(wgpu::include_wgsl!("grid.wgsl"));
        let grid_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("grid"),
                bind_group_layouts: &[&grid_layout],
                push_constant_ranges: &[],
            })),
            vertex: wgpu::VertexState {
                module: &grid_module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &grid_module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: OFFSCREEN,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // CRT passes.
        let crt_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("crt"),
            entries: &[uniform_entry(0), texture_entry(1), sampler_entry(2), texture_entry(3)],
        });
        let crt_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crt"),
            bind_group_layouts: &[&crt_layout],
            push_constant_ranges: &[],
        });
        let crt_module = device.create_shader_module(wgpu::include_wgsl!("crt.wgsl"));
        let blur_pipeline = fullscreen_pipeline(device, "crt blur", &crt_module, &crt_pipeline_layout, "blur", OFFSCREEN);
        let composite_pipeline =
            fullscreen_pipeline(device, "crt composite", &crt_module, &crt_pipeline_layout, "composite", output);
        let crt_uniforms = std::array::from_fn(|_| uniform_buffer(device, "crt uniforms", std::mem::size_of::<CrtUniforms>()));

        let instance_capacity = 4096;
        UiRenderer {
            fonts,
            scale,
            atlas: Atlas::new(ATLAS_SIZE),
            atlas_texture,
            grid_pipeline,
            grid_group,
            grid_uniforms,
            instances: instance_buffer(device, instance_capacity),
            instance_capacity,
            crt_layout,
            blur_pipeline,
            composite_pipeline,
            crt_uniforms,
            sampler,
            targets: None,
            scratch: Vec::new(),
        }
    }

    /// Re-rasterizes the font for a new display scale (the window moved to a different screen).
    pub fn set_scale(&mut self, scale: f32) {
        if scale != self.scale {
            self.scale = scale;
            self.fonts = Fonts::new(scale);
            self.atlas.clear();
        }
    }

    pub fn cell_size(&self) -> (u32, u32) {
        (self.fonts.cell_w, self.fonts.cell_h)
    }

    /// Columns and rows that fit in a window of this pixel size.
    pub fn grid_size(&self, width: u32, height: u32) -> (u16, u16) {
        let (cw, ch) = self.cell_size();
        ((width / cw).min(u16::MAX as u32) as u16, (height / ch).min(u16::MAX as u32) as u16)
    }

    /// Top-left of the grid in pixels (it's centered, so leftover pixels split evenly).
    pub fn origin(&self, width: u32, height: u32, cols: u16, rows: u16) -> (f32, f32) {
        let (cw, ch) = self.cell_size();
        (
            ((width.saturating_sub(cols as u32 * cw)) / 2) as f32,
            ((height.saturating_sub(rows as u32 * ch)) / 2) as f32,
        )
    }

    /// The cell under a pixel position, if any.
    pub fn cell_at(&self, x: f64, y: f64, width: u32, height: u32, cols: u16, rows: u16) -> Option<(u16, u16)> {
        let (ox, oy) = self.origin(width, height, cols, rows);
        let (cw, ch) = self.cell_size();
        let col = ((x - ox as f64) / cw as f64).floor();
        let row = ((y - oy as f64) / ch as f64).floor();
        (col >= 0.0 && row >= 0.0 && col < cols as f64 && row < rows as f64).then_some((col as u16, row as u16))
    }

    /// Fractional column under `x`, for sliders.
    pub fn column_at(&self, x: f64, width: u32, height: u32, cols: u16, rows: u16) -> f64 {
        let (ox, _) = self.origin(width, height, cols, rows);
        (x - ox as f64) / self.fonts.cell_w as f64
    }

    pub fn render<T: Clone>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        screen: &Screen<T>,
        effects: Effects,
    ) {
        let size = (size.0.max(1), size.1.max(1));
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            self.targets = Some(self.make_targets(device, size));
        }
        self.build_instances(screen, size);
        if self.atlas.is_full() {
            self.atlas.clear();
            self.build_instances(screen, size);
        }
        if self.atlas.dirty {
            queue.write_texture(
                self.atlas_texture.as_image_copy(),
                &self.atlas.pixels,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(ATLAS_SIZE), rows_per_image: None },
                self.atlas_texture.size(),
            );
            self.atlas.dirty = false;
        }
        if self.scratch.len() > self.instance_capacity {
            self.instance_capacity = self.scratch.len().next_power_of_two();
            self.instances = instance_buffer(device, self.instance_capacity);
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&self.scratch));
        let grid = GridUniforms { size: [size.0 as f32, size.1 as f32], atlas_size: [ATLAS_SIZE as f32; 2] };
        queue.write_buffer(&self.grid_uniforms, 0, bytemuck::bytes_of(&grid));

        let half = ((size.0 / 2).max(1), (size.1 / 2).max(1));
        let crt = |source: (u32, u32), direction: [f32; 2]| CrtUniforms {
            resolution: [size.0 as f32, size.1 as f32],
            texel: [1.0 / source.0 as f32, 1.0 / source.1 as f32],
            direction,
            time: effects.time,
            glow: effects.glow,
            scale: self.scale,
            _pad: [0.0; 7],
        };
        queue.write_buffer(&self.crt_uniforms[0], 0, bytemuck::bytes_of(&crt(size, [1.0, 0.0])));
        queue.write_buffer(&self.crt_uniforms[1], 0, bytemuck::bytes_of(&crt(half, [0.0, 1.0])));
        queue.write_buffer(&self.crt_uniforms[2], 0, bytemuck::bytes_of(&crt(size, [0.0, 0.0])));

        let targets = self.targets.as_ref().unwrap();
        {
            let [r, g, b] = BACKGROUND;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("grid"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.scene,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.grid_pipeline);
            pass.set_bind_group(0, &self.grid_group, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            pass.draw(0..6, 0..self.scratch.len() as u32);
        }
        let passes: [(&wgpu::TextureView, &wgpu::RenderPipeline); 3] = [
            (&targets.half[0], &self.blur_pipeline),
            (&targets.half[1], &self.blur_pipeline),
            (target, &self.composite_pipeline),
        ];
        for (i, (view, pipeline)) in passes.into_iter().enumerate() {
            let mut pass = crate::scenes::color_pass(encoder, "crt", view);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &targets.groups[i], &[]);
            pass.draw(0..3, 0..1);
        }
    }

    fn build_instances<T: Clone>(&mut self, screen: &Screen<T>, size: (u32, u32)) {
        self.scratch.clear();
        let (cw, ch) = (self.fonts.cell_w as f32, self.fonts.cell_h as f32);
        let (ox, oy) = self.origin(size.0, size.1, screen.cols, screen.rows);
        let line = (ch / 16.0).round().max(1.0);
        for (i, cell) in screen.cells().iter().enumerate() {
            let x = ox + (i % screen.cols as usize) as f32 * cw;
            let y = oy + (i / screen.cols as usize) as f32 * ch;
            if let Some(bg) = cell.bg {
                self.scratch.push(solid([x, y, cw, ch], bg));
            }
            if cell.ch == ' ' || cell.fg[3] <= 0.0 {
                continue;
            }
            if let Some(blocks) = procedural(cell.ch, cw, ch, line) {
                for ([bx, by, bw, bh], alpha) in blocks {
                    // Round edges, not sizes, so neighbouring cells meet without gaps or overlaps.
                    let (x0, x1) = ((bx * cw).round(), ((bx + bw) * cw).round());
                    let (y0, y1) = ((by * ch).round(), ((by + bh) * ch).round());
                    let rect = [x + x0, y + y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)];
                    let mut color = cell.fg;
                    color[3] *= alpha;
                    self.scratch.push(solid(rect, color));
                }
            } else if let Some(glyph) = self.atlas.glyph(&self.fonts, cell.ch) {
                let [ax, ay, aw, ah] = glyph.atlas.map(|v| v as f32);
                self.scratch.push(Instance {
                    rect: [x + glyph.offset[0], y + glyph.offset[1], aw, ah],
                    uv: [ax, ay, ax + aw, ay + ah],
                    color: cell.fg,
                    kind: [1.0, 0.0, 0.0, 0.0],
                });
            }
        }
    }

    fn make_targets(&self, device: &wgpu::Device, size: (u32, u32)) -> Targets {
        let texture = |label, (width, height): (u32, u32)| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: OFFSCREEN,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let half_size = ((size.0 / 2).max(1), (size.1 / 2).max(1));
        let scene = texture("ui scene", size);
        let half = [texture("ui bloom a", half_size), texture("ui bloom b", half_size)];
        let group = |uniforms: &wgpu::Buffer, image: &wgpu::TextureView, bloom: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("crt"),
                layout: &self.crt_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(image) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(bloom) },
                ],
            })
        };
        let groups = [
            group(&self.crt_uniforms[0], &scene, &half[1]),
            group(&self.crt_uniforms[1], &half[0], &scene),
            group(&self.crt_uniforms[2], &scene, &half[1]),
        ];
        Targets { size, scene, groups, half }
    }
}

fn solid(rect: [f32; 4], color: [f32; 4]) -> Instance {
    Instance { rect, uv: [0.0; 4], color, kind: [0.0; 4] }
}

fn uniform_buffer(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("grid instances"),
        size: (capacity * std::mem::size_of::<Instance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
