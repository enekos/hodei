use glow::HasContext;
use crate::types::Rect;

const VERTEX_SHADER: &str = r#"#version 330 core
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;
uniform vec4 u_rect; // (x, y, w, h) in 0..1 window coords
out vec2 v_uv;
void main() {
    vec2 pos = a_pos * 0.5 + 0.5;
    pos = u_rect.xy + pos * u_rect.zw;
    pos = pos * 2.0 - 1.0;
    gl_Position = vec4(pos, 0.0, 1.0);
    v_uv = vec2(a_uv.x, 1.0 - a_uv.y);
}
"#;

const FRAGMENT_SHADER: &str = r#"#version 330 core
in vec2 v_uv;
uniform sampler2D u_texture;
out vec4 frag_color;
void main() {
    frag_color = texture(u_texture, v_uv);
}
"#;

pub struct Compositor {
    program: glow::Program,
    quad_vao: glow::VertexArray,
    hud_texture: glow::Texture,
    rect_loc: Option<glow::UniformLocation>,
    tex_loc: Option<glow::UniformLocation>,
    window_width: u32,
    window_height: u32,
}

impl Compositor {
    /// # Safety
    /// Must be called with a valid, current GL context.
    pub unsafe fn new(gl: &glow::Context, width: u32, height: u32) -> Self {
        let program = Self::create_program(gl);
        let quad_vao = Self::create_quad_vao(gl);
        let hud_texture = Self::create_empty_texture(gl, width as i32, height as i32);
        let rect_loc = gl.get_uniform_location(program, "u_rect");
        let tex_loc = gl.get_uniform_location(program, "u_texture");

        Self {
            program,
            quad_vao,
            hud_texture,
            rect_loc,
            tex_loc,
            window_width: width,
            window_height: height,
        }
    }

    /// Draw all tiles and the HUD overlay.
    ///
    /// # Safety
    /// Must be called with a valid, current GL context.
    pub unsafe fn draw(
        &self,
        gl: &glow::Context,
        tiles: &[(Rect, glow::Texture)],
        hud_buffer: &[u8],
    ) {
        gl.viewport(0, 0, self.window_width as i32, self.window_height as i32);
        gl.clear_color(0.05, 0.05, 0.1, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);

        gl.use_program(Some(self.program));
        gl.uniform_1_i32(self.tex_loc.as_ref(), 0);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_vertex_array(Some(self.quad_vao));

        // Draw each tile
        gl.disable(glow::BLEND);
        let wf = self.window_width as f32;
        let hf = self.window_height as f32;
        for (rect, texture) in tiles {
            let norm_x = rect.x / wf;
            let norm_y = rect.y / hf;
            let norm_w = rect.width / wf;
            let norm_h = rect.height / hf;
            gl.uniform_4_f32(self.rect_loc.as_ref(), norm_x, norm_y, norm_w, norm_h);
            gl.bind_texture(glow::TEXTURE_2D, Some(*texture));
            gl.draw_arrays(glow::TRIANGLES, 0, 6);
        }

        // Draw HUD overlay with alpha blending
        gl.enable(glow::BLEND);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

        gl.bind_texture(glow::TEXTURE_2D, Some(self.hud_texture));
        gl.tex_sub_image_2d(
            glow::TEXTURE_2D,
            0,
            0,
            0,
            self.window_width as i32,
            self.window_height as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(hud_buffer)),
        );
        gl.uniform_4_f32(self.rect_loc.as_ref(), 0.0, 0.0, 1.0, 1.0);
        gl.draw_arrays(glow::TRIANGLES, 0, 6);

        gl.disable(glow::BLEND);
        gl.bind_vertex_array(None);
    }

    /// # Safety
    /// Requires valid GL context.
    pub unsafe fn resize(&mut self, gl: &glow::Context, width: u32, height: u32) {
        self.window_width = width;
        self.window_height = height;
        gl.delete_texture(self.hud_texture);
        self.hud_texture = Self::create_empty_texture(gl, width as i32, height as i32);
    }

    unsafe fn create_program(gl: &glow::Context) -> glow::Program {
        let program = gl.create_program().expect("create program");
        let shaders = [
            (glow::VERTEX_SHADER, VERTEX_SHADER),
            (glow::FRAGMENT_SHADER, FRAGMENT_SHADER),
        ];
        let compiled: Vec<glow::Shader> = shaders
            .iter()
            .map(|&(ty, src)| {
                let shader = gl.create_shader(ty).expect("create shader");
                gl.shader_source(shader, src);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    panic!("Shader compile error: {}", gl.get_shader_info_log(shader));
                }
                gl.attach_shader(program, shader);
                shader
            })
            .collect();
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            panic!("Program link error: {}", gl.get_program_info_log(program));
        }
        for s in compiled {
            gl.delete_shader(s);
        }
        program
    }

    unsafe fn create_quad_vao(gl: &glow::Context) -> glow::VertexArray {
        #[rustfmt::skip]
        let vertices: &[f32] = &[
            // pos       uv
            -1.0, -1.0,  0.0, 0.0,
             1.0, -1.0,  1.0, 0.0,
             1.0,  1.0,  1.0, 1.0,
            -1.0, -1.0,  0.0, 0.0,
             1.0,  1.0,  1.0, 1.0,
            -1.0,  1.0,  0.0, 1.0,
        ];
        let bytes: &[u8] = core::slice::from_raw_parts(
            vertices.as_ptr() as *const u8,
            vertices.len() * core::mem::size_of::<f32>(),
        );
        let vao = gl.create_vertex_array().expect("create VAO");
        gl.bind_vertex_array(Some(vao));
        let vbo = gl.create_buffer().expect("create VBO");
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
        let stride = 4 * core::mem::size_of::<f32>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, 8);
        gl.bind_vertex_array(None);
        vao
    }

    unsafe fn create_empty_texture(gl: &glow::Context, width: i32, height: i32) -> glow::Texture {
        let texture = gl.create_texture().expect("create texture");
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D, 0, glow::RGBA as i32,
            width, height, 0,
            glow::RGBA, glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        texture
    }
}
