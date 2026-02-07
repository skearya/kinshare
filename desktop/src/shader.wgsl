struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, 3.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0)
    );

    let position = positions[vertex_index];

    return VertexOutput(vec4(position.xy, 0.0, 1.0), position.xy);
}

@group(0) @binding(0)
var screen_texture: texture_2d<f32>;
@group(0) @binding(1)
var screen_sampler: sampler;

struct StandardUniform {
    screen_size: vec2<f32>,
    kindle_size: vec2<f32>,
};

@group(1) @binding(0)
var<uniform> standard: StandardUniform;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let screen_aspect_ratio = standard.kindle_size.x / standard.kindle_size.y;
    let display_aspect_ratio = standard.screen_size.x / standard.screen_size.y;

    var uv = vec2(in.clip_position.x / standard.screen_size.x, in.clip_position.y / standard.screen_size.y) - 0.5;

    if (screen_aspect_ratio < display_aspect_ratio) {
        uv.x /= screen_aspect_ratio / display_aspect_ratio;
    } else {
        uv.y /= display_aspect_ratio / screen_aspect_ratio;
    }

    uv += 0.5;

    if uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0 {
        return vec4<f32>(textureSample(screen_texture, screen_sampler, uv).xxx, 1.0);
    }

    let dist = distance(uv, vec2(0.5));

    return vec4<f32>(vec3(dist), 1.0);
}
