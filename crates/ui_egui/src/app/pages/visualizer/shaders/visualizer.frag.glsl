#version 330
in vec3 v_color;
in vec3 v_world_position;
uniform float u_brightness;
uniform float u_alpha;
uniform vec3 u_color;
uniform float u_use_vertex_color;
uniform vec3 u_light_direction;
uniform float u_use_lighting;
out vec4 color;
void main() {
    vec3 base = mix(u_color, v_color, u_use_vertex_color);
    float light = 1.0;
    if (u_use_lighting > 0.5) {
        vec3 normal = normalize(cross(dFdx(v_world_position), dFdy(v_world_position)));
        if (!gl_FrontFacing) {
            normal = -normal;
        }
        float diffuse = max(dot(normal, normalize(u_light_direction)), 0.0);
        light = 0.68 + 0.32 * diffuse;
    }
    color = vec4(base * u_brightness * light, u_alpha);
}
