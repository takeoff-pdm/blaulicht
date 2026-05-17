use std::sync::Arc;

use egui_glow::glow::{self, HasContext};

use super::constants::{FIXTURE_EDGE_COLOR, FONT_ATLAS_H, FONT_ATLAS_W};
use super::text::build_font_atlas_rgba;

pub(super) unsafe fn create_program(gl: &Arc<glow::Context>) -> Result<glow::Program, String> {
    let vertex_shader_source = include_str!("shaders/visualizer.vert.glsl");
    let fragment_shader_source = include_str!("shaders/visualizer.frag.glsl");

    let program = gl
        .create_program()
        .map_err(|e| format!("Program create failed: {e}"))?;
    let vs = compile_shader(gl, glow::VERTEX_SHADER, vertex_shader_source)?;
    let fs = compile_shader(gl, glow::FRAGMENT_SHADER, fragment_shader_source)?;

    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);

    gl.delete_shader(vs);
    gl.delete_shader(fs);

    if !gl.get_program_link_status(program) {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        return Err(format!("Program link failed: {log}"));
    }

    Ok(program)
}

pub(super) unsafe fn create_text_program(gl: &Arc<glow::Context>) -> Result<glow::Program, String> {
    let vertex_shader_source = include_str!("shaders/text.vert.glsl");
    let fragment_shader_source = include_str!("shaders/text.frag.glsl");

    let program = gl
        .create_program()
        .map_err(|e| format!("Text program create failed: {e}"))?;
    let vs = compile_shader(gl, glow::VERTEX_SHADER, vertex_shader_source)?;
    let fs = compile_shader(gl, glow::FRAGMENT_SHADER, fragment_shader_source)?;

    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);

    gl.delete_shader(vs);
    gl.delete_shader(fs);

    if !gl.get_program_link_status(program) {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        return Err(format!("Text program link failed: {log}"));
    }

    Ok(program)
}

pub(super) unsafe fn compile_shader(
    gl: &Arc<glow::Context>,
    shader_type: u32,
    source: &str,
) -> Result<glow::Shader, String> {
    let shader = gl
        .create_shader(shader_type)
        .map_err(|e| format!("Shader create failed: {e}"))?;
    gl.shader_source(shader, source);
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
        let log = gl.get_shader_info_log(shader);
        gl.delete_shader(shader);
        return Err(format!("Shader compile failed: {log}"));
    }
    Ok(shader)
}

pub(super) unsafe fn create_cube(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer, glow::Buffer, i32), String> {
    let vertices: [f32; 48] = [
        -1.0, -1.0, -1.0, 0.2, 0.6, 0.95, // 0
        1.0, -1.0, -1.0, 0.2, 0.6, 0.95, // 1
        1.0, 1.0, -1.0, 0.2, 0.6, 0.95, // 2
        -1.0, 1.0, -1.0, 0.2, 0.6, 0.95, // 3
        -1.0, -1.0, 1.0, 0.4, 0.8, 1.0, // 4
        1.0, -1.0, 1.0, 0.4, 0.8, 1.0, // 5
        1.0, 1.0, 1.0, 0.4, 0.8, 1.0, // 6
        -1.0, 1.0, 1.0, 0.4, 0.8, 1.0, // 7
    ];

    let indices: [u16; 36] = [
        0, 1, 2, 2, 3, 0, // back
        4, 5, 6, 6, 7, 4, // front
        0, 4, 7, 7, 3, 0, // left
        1, 5, 6, 6, 2, 1, // right
        3, 2, 6, 6, 7, 3, // top
        0, 1, 5, 5, 4, 0, // bottom
    ];

    let vao = gl
        .create_vertex_array()
        .map_err(|e| format!("VAO create failed: {e}"))?;
    let vbo = gl
        .create_buffer()
        .map_err(|e| format!("VBO create failed: {e}"))?;
    let ebo = gl
        .create_buffer()
        .map_err(|e| format!("EBO create failed: {e}"))?;

    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck::cast_slice(&vertices),
        glow::STATIC_DRAW,
    );
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
    gl.buffer_data_u8_slice(
        glow::ELEMENT_ARRAY_BUFFER,
        bytemuck::cast_slice(&indices),
        glow::STATIC_DRAW,
    );

    let stride = 6 * std::mem::size_of::<f32>() as i32;
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);

    gl.bind_vertex_array(None);

    Ok((vao, vbo, ebo, indices.len() as i32))
}

pub(super) unsafe fn create_cylinder(
    gl: &Arc<glow::Context>,
    segments: usize,
) -> Result<(glow::VertexArray, glow::Buffer, glow::Buffer, i32), String> {
    let segs = segments.max(3);
    let mut vertices: Vec<f32> = Vec::with_capacity((segs + 3) * 12);
    let mut indices: Vec<u16> = Vec::new();

    for i in 0..=segs {
        let angle = (i as f32 / segs as f32) * std::f32::consts::TAU;
        let x = angle.cos();
        let z = angle.sin();
        vertices.extend_from_slice(&[x, 1.0, z, 1.0, 1.0, 1.0]);
        vertices.extend_from_slice(&[x, -1.0, z, 1.0, 1.0, 1.0]);
    }

    let ring_vert_count = (segs + 1) * 2;
    let top_center_index = ring_vert_count as u16;
    let bottom_center_index = top_center_index + 1;
    vertices.extend_from_slice(&[0.0, 1.0, 0.0, 1.0, 1.0, 1.0]);
    vertices.extend_from_slice(&[0.0, -1.0, 0.0, 1.0, 1.0, 1.0]);

    for i in 0..segs {
        let top_i = (i * 2) as u16;
        let bottom_i = top_i + 1;
        let top_next = ((i + 1) * 2) as u16;
        let bottom_next = top_next + 1;
        indices.extend_from_slice(&[top_i, bottom_i, top_next]);
        indices.extend_from_slice(&[top_next, bottom_i, bottom_next]);
    }

    for i in 0..segs {
        let top_i = (i * 2) as u16;
        let top_next = ((i + 1) * 2) as u16;
        indices.extend_from_slice(&[top_center_index, top_next, top_i]);
    }

    for i in 0..segs {
        let bottom_i = (i * 2 + 1) as u16;
        let bottom_next = ((i + 1) * 2 + 1) as u16;
        indices.extend_from_slice(&[bottom_center_index, bottom_i, bottom_next]);
    }

    let vao = gl
        .create_vertex_array()
        .map_err(|e| format!("Cylinder VAO create failed: {e}"))?;
    let vbo = gl
        .create_buffer()
        .map_err(|e| format!("Cylinder VBO create failed: {e}"))?;
    let ebo = gl
        .create_buffer()
        .map_err(|e| format!("Cylinder EBO create failed: {e}"))?;

    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck::cast_slice(&vertices),
        glow::STATIC_DRAW,
    );
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
    gl.buffer_data_u8_slice(
        glow::ELEMENT_ARRAY_BUFFER,
        bytemuck::cast_slice(&indices),
        glow::STATIC_DRAW,
    );

    let stride = 6 * std::mem::size_of::<f32>() as i32;
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);

    gl.bind_vertex_array(None);

    Ok((vao, vbo, ebo, indices.len() as i32))
}

pub(super) unsafe fn create_cube_edges(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer, i32), String> {
    let color = FIXTURE_EDGE_COLOR;
    let mut verts: Vec<f32> = Vec::new();

    let mut add_edge = |a: (f32, f32, f32), b: (f32, f32, f32)| {
        verts.extend_from_slice(&[a.0, a.1, a.2, color[0], color[1], color[2]]);
        verts.extend_from_slice(&[b.0, b.1, b.2, color[0], color[1], color[2]]);
    };

    let p0 = (-1.0, -1.0, -1.0);
    let p1 = (1.0, -1.0, -1.0);
    let p2 = (1.0, 1.0, -1.0);
    let p3 = (-1.0, 1.0, -1.0);
    let p4 = (-1.0, -1.0, 1.0);
    let p5 = (1.0, -1.0, 1.0);
    let p6 = (1.0, 1.0, 1.0);
    let p7 = (-1.0, 1.0, 1.0);

    add_edge(p0, p1);
    add_edge(p1, p2);
    add_edge(p2, p3);
    add_edge(p3, p0);

    add_edge(p4, p5);
    add_edge(p5, p6);
    add_edge(p6, p7);
    add_edge(p7, p4);

    add_edge(p0, p4);
    add_edge(p1, p5);
    add_edge(p2, p6);
    add_edge(p3, p7);

    create_line_buffer(gl, &verts)
}

pub(super) unsafe fn create_grid(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer, i32), String> {
    let mut verts: Vec<f32> = Vec::new();
    let size = 10.0;
    let step = 1.0;
    let color = [0.26, 0.26, 0.26];

    let mut add_line = |x1: f32, z1: f32, x2: f32, z2: f32| {
        verts.extend_from_slice(&[x1, 0.0, z1, color[0], color[1], color[2]]);
        verts.extend_from_slice(&[x2, 0.0, z2, color[0], color[1], color[2]]);
    };

    let mut t = -size;
    while t <= size + 0.001 {
        add_line(t, -size, t, size);
        add_line(-size, t, size, t);
        t += step;
    }

    create_line_buffer(gl, &verts)
}

pub(super) unsafe fn create_axes(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer, i32), String> {
    let mut verts: Vec<f32> = Vec::new();
    let axis_len = 6.0;
    let add_axis = |verts: &mut Vec<f32>, x: f32, y: f32, z: f32, r: f32, g: f32, b: f32| {
        verts.extend_from_slice(&[0.0, 0.0, 0.0, r, g, b]);
        verts.extend_from_slice(&[x, y, z, r, g, b]);
    };

    add_axis(&mut verts, axis_len, 0.0, 0.0, 0.9, 0.1, 0.1);
    add_axis(&mut verts, 0.0, axis_len, 0.0, 0.1, 0.9, 0.1);
    add_axis(&mut verts, 0.0, 0.0, axis_len, 0.1, 0.3, 0.9);

    create_line_buffer(gl, &verts)
}

pub(super) unsafe fn create_line_buffer(
    gl: &Arc<glow::Context>,
    verts: &[f32],
) -> Result<(glow::VertexArray, glow::Buffer, i32), String> {
    let vao = gl
        .create_vertex_array()
        .map_err(|e| format!("Line VAO create failed: {e}"))?;
    let vbo = gl
        .create_buffer()
        .map_err(|e| format!("Line VBO create failed: {e}"))?;

    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck::cast_slice(verts),
        glow::STATIC_DRAW,
    );

    let stride = 6 * std::mem::size_of::<f32>() as i32;
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);

    gl.bind_vertex_array(None);

    Ok((vao, vbo, (verts.len() / 6) as i32))
}

pub(super) unsafe fn create_dynamic_mesh(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer), String> {
    let vao = gl
        .create_vertex_array()
        .map_err(|e| format!("Dynamic VAO create failed: {e}"))?;
    let vbo = gl
        .create_buffer()
        .map_err(|e| format!("Dynamic VBO create failed: {e}"))?;

    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_size(glow::ARRAY_BUFFER, 0, glow::DYNAMIC_DRAW);

    let stride = 6 * std::mem::size_of::<f32>() as i32;
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);

    gl.bind_vertex_array(None);

    Ok((vao, vbo))
}

pub(super) unsafe fn create_quad(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer, i32), String> {
    let vertices: [f32; 36] = [
        -1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0,
        0.0, -1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0,
        0.0, 0.0,
    ];

    let vao = gl
        .create_vertex_array()
        .map_err(|e| format!("Quad VAO create failed: {e}"))?;
    let vbo = gl
        .create_buffer()
        .map_err(|e| format!("Quad VBO create failed: {e}"))?;

    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck::cast_slice(&vertices),
        glow::STATIC_DRAW,
    );

    let stride = 6 * std::mem::size_of::<f32>() as i32;
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);

    gl.bind_vertex_array(None);

    Ok((vao, vbo, 6))
}

pub(super) unsafe fn create_text_buffers(
    gl: &Arc<glow::Context>,
) -> Result<(glow::VertexArray, glow::Buffer), String> {
    let vao = gl
        .create_vertex_array()
        .map_err(|e| format!("Text VAO create failed: {e}"))?;
    let vbo = gl
        .create_buffer()
        .map_err(|e| format!("Text VBO create failed: {e}"))?;

    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_size(glow::ARRAY_BUFFER, 0, glow::DYNAMIC_DRAW);

    let stride = 5 * std::mem::size_of::<f32>() as i32;
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, 12);

    gl.bind_vertex_array(None);

    Ok((vao, vbo))
}

pub(super) unsafe fn create_font_texture(gl: &Arc<glow::Context>) -> Result<glow::Texture, String> {
    let texture = gl
        .create_texture()
        .map_err(|e| format!("Font texture create failed: {e}"))?;
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));

    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MIN_FILTER,
        glow::NEAREST as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MAG_FILTER,
        glow::NEAREST as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_S,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_T,
        glow::CLAMP_TO_EDGE as i32,
    );

    let data = build_font_atlas_rgba();
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA as i32,
        FONT_ATLAS_W as i32,
        FONT_ATLAS_H as i32,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(Some(data.as_slice())),
    );

    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok(texture)
}
