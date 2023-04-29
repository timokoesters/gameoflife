@group(0) @binding(0) var texture: texture_2d<f32>;
@group(1) @binding(0) var<uniform> uniforms: Uniforms;

//https://stackoverflow.com/questions/5149544/can-i-generate-a-random-number-inside-a-pixel-shader
fn random(p: vec2<f32>) -> f32 {
    let K1 = vec2(
        23.14069263277926, // e^pi (Gelfond's constant)
         2.665144142690225 // 2^sqrt(2) (Gelfondâ€“Schneider constant)
    );
    return fract( cos( dot(p,K1) ) * 12345.6789 );
}

struct Uniforms {
    mouse_pos: vec2<f32>,
    seed: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vs_compute(
    @builtin(vertex_index) in_vertex_index: u32,
) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(1 - i32(in_vertex_index)) * 5.0;
    let y = f32(i32(in_vertex_index & 1u) * 2 - 1) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_compute(in: VertexOutput) -> @location(0) vec4<f32> {
    let x = i32(in.clip_position.x);
    let y = i32(in.clip_position.y);

    if x == 50 || y == 50 {
        return vec4(1.0, 0.0, 0.0, 1.0);
    }
    let diff = in.clip_position.xy - uniforms.mouse_pos;
    let dist = dot(diff, diff);

    if dist < 122.0 {
        let r = vec3(random(uniforms.seed), random(2.0 * uniforms.seed), random(3.0 * uniforms.seed));
        let rlength = length(r);

        return vec4(r / rlength, 1.0);
    }

    var sum = vec3(0.0, 0.0, 0.0);
    sum += textureLoad(texture, vec2(x - 1, y), 0).rgb;
    sum += textureLoad(texture, vec2(x + 1, y), 0).rgb;
    sum += textureLoad(texture, vec2(x, y + 1), 0).rgb;
    sum += textureLoad(texture, vec2(x, y - 1), 0).rgb;
    sum += textureLoad(texture, vec2(x - 1, y - 1), 0).rgb;
    sum += textureLoad(texture, vec2(x - 1, y + 1), 0).rgb;
    sum += textureLoad(texture, vec2(x + 1, y - 1), 0).rgb;
    sum += textureLoad(texture, vec2(x + 1, y + 1), 0).rgb;
    let sumlength = length(sum);

    let current = textureLoad(texture, vec2<i32>(x, y), 0).rgb;
    if dot(current, current) <= 0.5 {
        if sumlength >= 3.0 - 0.1 && sumlength <= 3.0 + 0.1 {
            let color = sum/sumlength;
            return vec4(color, 1.0);
        } else {
            return vec4(0.0, 0.0, 0.0, 1.0);
        }
    } else {
        if sumlength >= 2.0 - 0.1 && sumlength <= 3.0 + 0.1 {
            return vec4(current, 1.0);
        } else {
            return vec4(0.0, 0.0, 0.0, 1.0);
        }
    }
}

@vertex
fn vs_main(
    @builtin(vertex_index) in_vertex_index: u32,
) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(1 - i32(in_vertex_index)) * 5.0;
    let y = f32(i32(in_vertex_index & 1u) * 2 - 1) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let x = i32(in.clip_position.x);
    let y = i32(in.clip_position.y);

    return vec4(textureLoad(texture, vec2(x, y), 0).rgb, 1.0);
}
