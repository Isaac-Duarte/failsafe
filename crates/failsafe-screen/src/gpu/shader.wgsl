struct Params {
    src_width: u32,
    src_height: u32,
    dst_width: u32,
    dst_height: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var input_tex: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> output_rgb: array<u32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.dst_width || gid.y >= params.dst_height) {
        return;
    }

    let u = (f32(gid.x) + 0.5) / f32(params.dst_width);
    let v = (f32(gid.y) + 0.5) / f32(params.dst_height);
    let src_x = min(u32(u * f32(params.src_width)), params.src_width - 1u);
    let src_y = min(u32(v * f32(params.src_height)), params.src_height - 1u);
    let rgba = textureLoad(input_tex, vec2<i32>(i32(src_x), i32(src_y)), 0);

    let r = u32(clamp(rgba.r, 0.0, 1.0) * 255.0);
    let g = u32(clamp(rgba.g, 0.0, 1.0) * 255.0);
    let b = u32(clamp(rgba.b, 0.0, 1.0) * 255.0);
    let idx = gid.y * params.dst_width + gid.x;
    output_rgb[idx] = (r << 16u) | (g << 8u) | b;
}
