#version 300 es
precision highp float;
uniform sampler2D u_cells;
uniform sampler2D u_glyphs;
uniform vec2 u_resolution;
uniform vec2 u_grid;
out vec4 color;
void main() {
    vec2 uv = gl_FragCoord.xy / u_resolution;
    vec4 cell = texture(u_cells, uv);
    vec2 local = fract(vec2(uv.x, 1.0 - uv.y) * u_grid);
    float glyph = floor(cell.a * 15.0 + 0.5);
    float ink = texture(u_glyphs, vec2((glyph + local.x) / 16.0, local.y)).r;
    vec3 paper = vec3(0.0015, 0.005, 0.0035) + cell.rgb * 0.024;
    color = vec4(paper + cell.rgb * ink, 1.0);
}
