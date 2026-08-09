#version 330
layout (location = 0) in vec3 a_pos;
layout (location = 1) in vec3 a_color;
uniform mat4 u_mvp;
uniform mat4 u_model;
out vec3 v_color;
out vec3 v_world_position;
void main() {
    v_color = a_color;
    v_world_position = (u_model * vec4(a_pos, 1.0)).xyz;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
