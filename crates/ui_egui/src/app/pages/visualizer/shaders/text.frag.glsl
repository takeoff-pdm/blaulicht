#version 330
in vec2 v_uv;
uniform sampler2D u_tex;
uniform vec4 u_color;
out vec4 color;
void main() {
    float alpha = texture(u_tex, v_uv).a;
    color = vec4(u_color.rgb, u_color.a * alpha);
}
