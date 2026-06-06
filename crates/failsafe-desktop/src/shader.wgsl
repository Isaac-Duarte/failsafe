@group(0) @binding(0) var frame_texture: texture_2d<f32>;
@group(0) @binding(1) var frame_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    switch (in_vertex_index) {
        case 0u: {
            out.position = vec4<f32>(-1.0, -1.0, 0.0, 1.0);
            out.tex_coords = vec2<f32>(0.0, 1.0);
        }
        case 1u: {
            out.position = vec4<f32>(1.0, -1.0, 0.0, 1.0);
            out.tex_coords = vec2<f32>(1.0, 1.0);
        }
        case 2u: {
            out.position = vec4<f32>(-1.0, 1.0, 0.0, 1.0);
            out.tex_coords = vec2<f32>(0.0, 0.0);
        }
        case 3u: {
            out.position = vec4<f32>(1.0, -1.0, 0.0, 1.0);
            out.tex_coords = vec2<f32>(1.0, 1.0);
        }
        case 4u: {
            out.position = vec4<f32>(-1.0, 1.0, 0.0, 1.0);
            out.tex_coords = vec2<f32>(0.0, 0.0);
        }
        default: {
            out.position = vec4<f32>(1.0, 1.0, 0.0, 1.0);
            out.tex_coords = vec2<f32>(1.0, 0.0);
        }
    }

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(frame_texture, frame_sampler, in.tex_coords);
}
