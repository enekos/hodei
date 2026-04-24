use glow::HasContext;

// Full-window textured quad. Used to draw the HUD RGBA buffer over the
// already-blitted tile content in the window framebuffer.
const VERTEX_SHADER: &str = r#"#version 330 core
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;
out vec2 v_uv;
void main() {
    gl_Position = vec4(a_pos, 0.0, 1.0);
    // Slint writes its buffer top-down; GL sampling wants bottom-up.
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
    window_width: u32,
    window_height: u32,
}

impl Compositor {
    /// # Safety
    /// Must be called with a valid, current GL context.
    pub unsafe fn new(gl: &glow::Context, width: u32, height: u32) -> Self {
        log::info!("Compositor::new: creating GL resources for {}x{}", width, height);
        let program = Self::create_program(gl);
        let quad_vao = Self::create_quad_vao(gl);
        let hud_texture = Self::create_empty_texture(gl, width as i32, height as i32);

        // Reset to defaults so we hand back a clean context.
        gl.bind_vertex_array(None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.use_program(None);

        log::info!("Compositor::new: GL resources created successfully");
        Self {
            program,
            quad_vao,
            hud_texture,
            window_width: width,
            window_height: height,
        }
    }

    /// Draw the HUD overlay on top of whatever is already in the bound framebuffer.
    ///
    /// Tiles must be blitted into the window framebuffer before calling this.
    ///
    /// # Safety
    /// Must be called with a valid, current GL context and the window FBO bound.
    pub unsafe fn draw_hud(&self, gl: &glow::Context, hud_buffer: &[u8]) {
        let expected = (self.window_width * self.window_height * 4) as usize;
        if hud_buffer.len() != expected {
            log::error!(
                "Compositor::draw_hud: HUD buffer size mismatch! expected={} got={} ({}x{})",
                expected,
                hud_buffer.len(),
                self.window_width,
                self.window_height
            );
        }
        log::trace!(
            "Compositor::draw_hud: uploading {} bytes for {}x{} HUD",
            hud_buffer.len(),
            self.window_width,
            self.window_height
        );

        // Reset GL state that Servo's renderer may have left on.
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::STENCIL_TEST);
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::CULL_FACE);
        gl.depth_mask(false);
        gl.color_mask(true, true, true, true);

        gl.viewport(0, 0, self.window_width as i32, self.window_height as i32);

        gl.use_program(Some(self.program));
        gl.active_texture(glow::TEXTURE0);
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
        log::trace!("Compositor::draw_hud: texture uploaded");

        // Slint outputs premultiplied-alpha RGBA.
        gl.enable(glow::BLEND);
        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);

        gl.bind_vertex_array(Some(self.quad_vao));
        log::trace!("Compositor::draw_hud: drawing 6 vertices (2 triangles)");
        gl.draw_arrays(glow::TRIANGLES, 0, 6);

        gl.bind_vertex_array(None);
        gl.disable(glow::BLEND);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.use_program(None);
        log::trace!("Compositor::draw_hud: draw complete, GL state reset");
    }

    /// # Safety
    /// Requires valid GL context.
    pub unsafe fn resize(&mut self, gl: &glow::Context, width: u32, height: u32) {
        log::info!("Compositor::resize: {}x{} -> {}x{}", self.window_width, self.window_height, width, height);
        self.window_width = width;
        self.window_height = height;
        gl.delete_texture(self.hud_texture);
        self.hud_texture = Self::create_empty_texture(gl, width as i32, height as i32);
        log::debug!("Compositor::resize: HUD texture recreated");
    }

    unsafe fn create_program(gl: &glow::Context) -> glow::Program {
        log::debug!("Compositor::create_program: compiling shaders");
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
                    let log = gl.get_shader_info_log(shader);
                    log::error!("Shader compile error: {}", log);
                    panic!("Shader compile error: {}", log);
                }
                gl.attach_shader(program, shader);
                shader
            })
            .collect();
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            log::error!("Program link error: {}", log);
            panic!("Program link error: {}", log);
        }
        for s in compiled {
            gl.delete_shader(s);
        }
        log::debug!("Compositor::create_program: shader program linked successfully");
        // Bind the sampler once: the shader always uses TEXTURE0.
        gl.use_program(Some(program));
        if let Some(loc) = gl.get_uniform_location(program, "u_texture") {
            gl.uniform_1_i32(Some(&loc), 0);
        }
        gl.use_program(None);
        program
    }

    unsafe fn create_quad_vao(gl: &glow::Context) -> glow::VertexArray {
        log::debug!("Compositor::create_quad_vao: creating fullscreen quad VAO/VBO");
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
        // SAFETY: reinterpret the f32 slice as bytes for OpenGL upload.
        let bytes: &[u8] = core::slice::from_raw_parts(
            vertices.as_ptr() as *const u8,
            std::mem::size_of_val(vertices),
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
        log::debug!("Compositor::create_quad_vao: VAO={:?} VBO={:?} stride={}", vao, vbo, stride);
        vao
    }

    unsafe fn create_empty_texture(gl: &glow::Context, width: i32, height: i32) -> glow::Texture {
        log::debug!("Compositor::create_empty_texture: {}x{}", width, height);
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
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        gl.bind_texture(glow::TEXTURE_2D, None);
        log::debug!("Compositor::create_empty_texture: texture={:?}", texture);
        texture
    }
}
