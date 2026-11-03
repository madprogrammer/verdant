#version 300 es
precision highp float;
precision highp int;
uniform sampler2D u_image;
uniform vec2 u_resolution;
uniform float u_time;
uniform float u_columns;
uniform float u_brightness;
out vec4 color;

float hash(vec2 p) {
    vec3 q = fract(vec3(p.xyx) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

void main() {
    // All scene calculations use a fixed character grid, not individual pixels.
    float rows = max(1.0, floor(u_columns * u_resolution.y / u_resolution.x / 1.65));
    vec2 grid = vec2(u_columns, rows);
    vec2 uv = vec2(gl_FragCoord.x, rows - gl_FragCoord.y) / grid;
    vec2 cell = floor(uv * grid);
    vec2 st = (cell + 0.5) / grid;
    float aspect = u_resolution.x / u_resolution.y;
    vec2 crop = vec2(min(aspect / (16.0/9.0), 1.0), min((16.0/9.0) / aspect, 1.0));
    vec2 p = (st - 0.5) * crop + 0.5;
    float t = u_time;
    float seed = hash(cell);

    // Soft masks keep the trunk and moon steady while the crown breathes.
    float crown = (1.0 - smoothstep(0.09, 0.14, abs(p.x - 0.515)))
                * smoothstep(0.47, 0.56, p.y) * (1.0 - smoothstep(0.72, 0.80, p.y));
    vec2 sampleUV = p;
    sampleUV.x += crown * (sin(t * 0.65 + p.y * 18.0) + 0.35 * sin(t * 1.1 + p.x * 33.0)) * 0.0015;
    // Slow cloud movement; fade out well before the tree and lunar disc.
    float moonDistance = length((p - vec2(0.318, 0.125)) * vec2(1.7778, 1.0));
    float sky = 1.0 - smoothstep(0.38, 0.57, p.y);
    sampleUV.x += sky * smoothstep(0.08, 0.19, moonDistance) * sin(t * 0.065) * 0.012;
    vec3 source = texture(u_image, sampleUV).rgb;
    float luminance = dot(source, vec3(0.23, 0.65, 0.12));
    float leaf = smoothstep(0.008, 0.10, source.g - max(source.r, source.b));
    float field = smoothstep(0.77, 0.82, p.y);
    float wind = sin(p.x * 31.0 + p.y * 17.0 - t * 0.8) * sin(p.y * 39.0 + t * 0.35);
    luminance *= 1.0 + leaf * wind * 0.08;
    luminance += crown * leaf * 0.085;

    float fog = pow(0.5 + 0.5 * sin(p.x * 9.0 + sin(p.y * 13.0 + t * 0.13) - t * 0.09), 3.0);
    fog *= exp(-pow((p.y - 0.735) * 10.0, 2.0)) * 0.024;
    float tone = clamp(pow(luminance, 0.82) * 1.12 + fog, 0.0, 1.0);
    // Stable per-cell grain, with very slow, spatially coherent glyph changes.
    float breathe = sin(t * 0.36 + p.x * 17.0 + p.y * 11.0) * 0.012;
    float level = clamp(tone + (seed - 0.5) * 0.055 + breathe * leaf, 0.0, 0.999);
    float glyph = floor(level * 16.0);

    // Sparse, gently wandering fireflies; one candidate in each large tile.
    vec2 tile = floor(st * vec2(19.0, 10.0));
    float flySeed = hash(tile + 91.0);
    vec2 flyPos = (tile + vec2(0.5 + 0.29 * sin(t * 0.23 + flySeed * 61.0),
                                      0.5 + 0.30 * cos(t * 0.19 + flySeed * 37.0))) / vec2(19.0, 10.0);
    vec2 flyDelta = (st - flyPos) * grid;
    float flyOn = step(0.84, flySeed) * smoothstep(0.4, 0.7, st.y)
                * (1.0 - smoothstep(0.87, 0.98, st.y));
    float pulse = pow(0.5 + 0.5 * sin(t * 0.7 + flySeed * 91.0), 2.0);
    float fly = flyOn * pulse * (1.0 - smoothstep(0.0, 1.15, length(flyDelta)));
    if (fly > 0.16) glyph = 7.0; // '+'

    vec3 green = mix(vec3(0.10, 0.38, 0.19), vec3(0.30, 0.90, 0.32), leaf);
    green = mix(green, vec3(0.63, 1.0, 0.72), smoothstep(0.61, 0.94, luminance));
    float strength = (0.12 + pow(tone, 0.92) * 0.95) * mix(1.0, 0.74, field);
    vec3 lit = green * strength;
    lit = mix(lit, vec3(0.63, 0.96, 0.36), fly * 0.92);
    vec3 result = lit;
    float vignette = 1.0 - 0.23 * smoothstep(0.25, 0.76, length((uv - 0.5) * vec2(1.0, 0.85)));
    color = vec4(result * vignette * u_brightness, glyph / 15.0);
}
