#version 330
layout (location = 0) in vec3 a_pos;
layout (location = 1) in vec3 a_color;
layout (location = 2) in mat4 a_instance_model;
layout (location = 6) in vec3 a_instance_color;
uniform mat4 u_mvp;
uniform mat4 u_model;
uniform float u_use_instancing;
out vec3 v_color;
out vec3 v_world_position;
void main() {
    mat4 model = u_use_instancing > 0.5 ? a_instance_model : u_model;
    vec4 world_position = model * vec4(a_pos, 1.0);
    v_color = u_use_instancing > 0.5 ? a_instance_color : a_color;
    v_world_position = world_position.xyz;
    gl_Position = u_use_instancing > 0.5
        ? u_mvp * world_position
        : u_mvp * vec4(a_pos, 1.0);
}
