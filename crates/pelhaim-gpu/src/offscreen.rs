use anyhow::Context;
use pelhaim_board::{DisplayList, Rgba};

use crate::renderer::Renderer;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn adapter() -> anyhow::Result<wgpu::Adapter> {
    // `_from_env` honours WGPU_BACKEND / WGPU_ADAPTER_NAME, so a run can pin an adapter such as lavapipe.
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .context("no wgpu adapter")
}

/// The adapter a window would get, as "name (backend)", or `None` when there is no GPU.
pub fn adapter_info() -> Option<String> {
    let info = adapter().ok()?.get_info();
    Some(format!("{} ({:?})", info.name, info.backend))
}

/// Renders `dl` into a `w`×`h` texture and reads it back as tightly packed RGBA8 sRGB rows.
pub fn render_offscreen(
    dl: &DisplayList,
    w: u32,
    h: u32,
    scale: f32,
    clear: Rgba,
) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(w > 0 && h > 0, "empty offscreen size {w}x{h}");
    let adapter = adapter()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("pelhaim offscreen"),
        ..Default::default()
    }))?;
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pelhaim offscreen"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut renderer = Renderer::new(&device, FORMAT);
    renderer.render(&device, &queue, &view, dl, (w, h), scale, clear);

    let row = w * 4;
    let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pelhaim readback"),
        size: u64::from(padded) * u64::from(h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);

    let (tx, rx) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    rx.recv().context("readback callback dropped")??;
    let mapped = readback.get_mapped_range(..)?;
    let mut out = Vec::with_capacity((row * h) as usize);
    for line in mapped.chunks_exact(padded as usize) {
        out.extend_from_slice(&line[..row as usize]);
    }
    Ok(out)
}
