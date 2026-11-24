use pelhaim_board::{DisplayList, Rgba};

use crate::tessellate::{Vertex, linear_premul, tessellate};

/// Triangle edges get their smoothing from MSAA; rects and segments do their own SDF coverage.
const SAMPLES: u32 = 4;

const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x4,
    2 => Float32x2,
    3 => Float32x2,
    4 => Float32,
];

pub struct Renderer {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertices: Option<wgpu::Buffer>,
    indices: Option<wgpu::Buffer>,
    msaa: Option<(wgpu::TextureView, (u32, u32))>,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Renderer {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/board.wgsl"));
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pelhaim globals"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pelhaim globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pelhaim globals"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pelhaim board"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pelhaim board"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &ATTRIBUTES,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Renderer {
            format,
            pipeline,
            globals,
            bind_group,
            vertices: None,
            indices: None,
            msaa: None,
        }
    }

    /// Draws `dl` into `target`, which must be a single-sample view of the renderer's format
    /// sized `viewport_px`. `scale` maps display-list units to device pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        dl: &DisplayList,
        viewport_px: (u32, u32),
        scale: f32,
        clear: Rgba,
    ) {
        let (vs, is, batches) = tessellate(dl, scale, viewport_px);
        let (w, h) = viewport_px;
        let msaa = self.msaa_view(device, viewport_px);
        let globals = [w as f32, h as f32, 0.0, 0.0];
        queue.write_buffer(&self.globals, 0, bytemuck::cast_slice(&globals));
        let vbuf = upload(
            device,
            queue,
            &mut self.vertices,
            bytemuck::cast_slice(&vs),
            wgpu::BufferUsages::VERTEX,
        );
        let ibuf = upload(
            device,
            queue,
            &mut self.indices,
            bytemuck::cast_slice(&is),
            wgpu::BufferUsages::INDEX,
        );

        let [r, g, b, a] = linear_premul(clear).map(f64::from);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("pelhaim board"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pelhaim board"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa,
                    depth_slice: None,
                    resolve_target: Some(target),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let (Some(vbuf), Some(ibuf)) = (vbuf, ibuf) {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                for batch in &batches {
                    let [x, y, sw, sh] = batch.scissor.unwrap_or([0, 0, w, h]);
                    if sw == 0 || sh == 0 {
                        continue;
                    }
                    pass.set_scissor_rect(x, y, sw, sh);
                    pass.draw_indexed(batch.range.clone(), 0, 0..1);
                }
            }
        }
        queue.submit([encoder.finish()]);
    }

    fn msaa_view(&mut self, device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
        if let Some((view, s)) = &self.msaa
            && *s == size
        {
            return view.clone();
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pelhaim msaa"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        self.msaa = Some((view.clone(), size));
        view
    }
}

/// Writes `bytes` into `slot`, growing it to the next power of two when it is too small.
/// Returns `None` for empty data, which wgpu cannot bind.
fn upload<'a>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    slot: &'a mut Option<wgpu::Buffer>,
    bytes: &[u8],
    usage: wgpu::BufferUsages,
) -> Option<&'a wgpu::Buffer> {
    if bytes.is_empty() {
        return None;
    }
    let len = bytes.len() as wgpu::BufferAddress;
    if slot.as_ref().is_none_or(|b| b.size() < len) {
        *slot = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pelhaim geometry"),
            size: len.next_power_of_two(),
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
    let buf = slot.as_ref()?;
    queue.write_buffer(buf, 0, bytes);
    Some(buf)
}
