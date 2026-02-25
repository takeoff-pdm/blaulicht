#version 330
in vec3 v_color;
uniform float u_brightness;
uniform vec3 u_color;
uniform float u_use_vertex_color;
out vec4 color;
void main() {
    vec3 base = mix(u_color, v_color, u_use_vertex_color);
    color = vec4(base * u_brightness, 1.0);
}
