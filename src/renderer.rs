use crate::Options;
use anyhow::{anyhow, bail, Context, Result};
use glow::HasContext;
use khronos_egl as egl;
use std::{ffi::c_void, path::Path};

pub struct Renderer {
    gl: glow::Context,
    egl: egl::DynamicInstance<egl::EGL1_4>,
    display: egl::Display,
    surface: egl::Surface,
    context: egl::Context,
    program: glow::Program,
    ink_program: glow::Program,
    framebuffer: glow::Framebuffer,
    cells: glow::Texture,
    grid_size: (u32, u32),
    ink_resolution: Option<glow::UniformLocation>,
    ink_grid: Option<glow::UniformLocation>,
    vao: glow::VertexArray,
    textures: [glow::Texture; 2],
    time: Option<glow::UniformLocation>,
    resolution: Option<glow::UniformLocation>,
    columns: Option<glow::UniformLocation>,
    brightness: Option<glow::UniformLocation>,
    pub width: u32,
    pub height: u32,
}

impl Renderer {
    pub fn headless(options: &Options) -> Result<Self> {
        // This process creates no Wayland connection in snapshot mode.
        std::env::set_var("EGL_PLATFORM", "surfaceless");
        unsafe {
            Self::new(
                std::ptr::null_mut(),
                None,
                options.width,
                options.height,
                options,
            )
        }
    }

    /// Display and native window must remain valid until this renderer is dropped.
    pub unsafe fn new(
        native_display: *mut c_void,
        window: Option<*mut c_void>,
        width: u32,
        height: u32,
        options: &Options,
    ) -> Result<Self> {
        let egl = egl::DynamicInstance::<egl::EGL1_4>::load_required()
            .context("Loading libEGL.so.1; run through `nix develop` or `nix run`")?;
        let display = egl.get_display(native_display).context("No EGL display")?;
        egl.initialize(display).context("Initializing EGL")?;
        egl.bind_api(egl::OPENGL_ES_API)?;
        let config = egl
            .choose_first_config(
                display,
                &[
                    egl::SURFACE_TYPE,
                    if window.is_some() {
                        egl::WINDOW_BIT
                    } else {
                        egl::PBUFFER_BIT
                    },
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_ES2_BIT,
                    egl::RED_SIZE,
                    8,
                    egl::GREEN_SIZE,
                    8,
                    egl::BLUE_SIZE,
                    8,
                    egl::DEPTH_SIZE,
                    0,
                    egl::STENCIL_SIZE,
                    0,
                    egl::NONE,
                ],
            )?
            .context("No RGB EGL configuration")?;
        let context = egl.create_context(
            display,
            config,
            None,
            &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE],
        )?;
        let surface = if let Some(window) = window {
            egl.create_window_surface(display, config, window, None)?
        } else {
            egl.create_pbuffer_surface(
                display,
                config,
                &[
                    egl::WIDTH,
                    width as i32,
                    egl::HEIGHT,
                    height as i32,
                    egl::NONE,
                ],
            )?
        };
        egl.make_current(display, Some(surface), Some(surface), Some(context))?;
        // Frame callbacks and our timer do the pacing; don't block in swap_buffers.
        egl.swap_interval(display, 0)?;
        let gl = glow::Context::from_loader_function(|s| {
            egl.get_proc_address(s)
                .map_or(std::ptr::null(), |f| f as *const c_void)
        });
        eprintln!(
            "GPU: {} · {}",
            gl.get_parameter_string(glow::RENDERER),
            gl.get_parameter_string(glow::VERSION)
        );
        let program = compile_program(&gl, include_str!("../shaders/scene.frag"))?;
        let ink_program = compile_program(&gl, include_str!("../shaders/ink.frag"))?;
        gl.use_program(Some(program));
        let vao = gl.create_vertex_array().map_err(|e| anyhow!(e))?;
        gl.bind_vertex_array(Some(vao));
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::BLEND);
        let source = match &options.image {
            Some(path) => {
                image::open(path).with_context(|| format!("Opening {}", path.display()))?
            }
            None => image::load_from_memory(include_bytes!("../assets/tree.jpg"))?,
        }
        .thumbnail(1280, 1280)
        .to_rgba8();
        let photo = gl.create_texture().map_err(|e| anyhow!(e))?;
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(photo));
        texture_parameters(&gl, glow::LINEAR);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA as i32,
            source.width() as i32,
            source.height() as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(source.as_raw())),
        );
        gl.uniform_1_i32(gl.get_uniform_location(program, "u_image").as_ref(), 0);
        let atlas = gl.create_texture().map_err(|e| anyhow!(e))?;
        gl.active_texture(glow::TEXTURE1);
        gl.bind_texture(glow::TEXTURE_2D, Some(atlas));
        texture_parameters(&gl, glow::NEAREST);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::R8 as i32,
            128,
            12,
            0,
            glow::RED,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&glyph_atlas())),
        );
        gl.uniform_1_i32(gl.get_uniform_location(program, "u_glyphs").as_ref(), 1);
        let time = gl.get_uniform_location(program, "u_time");
        let resolution = gl.get_uniform_location(program, "u_resolution");
        let columns = gl.get_uniform_location(program, "u_columns");
        let brightness = gl.get_uniform_location(program, "u_brightness");
        let framebuffer = gl.create_framebuffer().map_err(|e| anyhow!(e))?;
        let cells = gl.create_texture().map_err(|e| anyhow!(e))?;
        gl.active_texture(glow::TEXTURE2);
        gl.bind_texture(glow::TEXTURE_2D, Some(cells));
        texture_parameters(&gl, glow::NEAREST);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            1,
            1,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(cells),
            0,
        );
        if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
            bail!("Character framebuffer is incomplete");
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.use_program(Some(ink_program));
        gl.uniform_1_i32(gl.get_uniform_location(ink_program, "u_cells").as_ref(), 2);
        gl.uniform_1_i32(gl.get_uniform_location(ink_program, "u_glyphs").as_ref(), 1);
        let ink_resolution = gl.get_uniform_location(ink_program, "u_resolution");
        let ink_grid = gl.get_uniform_location(ink_program, "u_grid");
        Ok(Self {
            ink_program,
            framebuffer,
            cells,
            grid_size: (1, 1),
            ink_resolution,
            ink_grid,
            gl,
            egl,
            display,
            surface,
            context,
            program,
            vao,
            textures: [photo, atlas],
            time,
            resolution,
            columns,
            brightness,
            width,
            height,
        })
    }

    pub fn draw(&mut self, time: f32, options: &Options) {
        unsafe {
            let columns = options.columns;
            let rows = (columns as f32 * self.height as f32 / self.width as f32 / 1.65)
                .floor()
                .max(1.0) as u32;
            if self.grid_size != (columns, rows) {
                self.gl.active_texture(glow::TEXTURE2);
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.cells));
                self.gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    columns as i32,
                    rows as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                self.grid_size = (columns, rows);
            }
            // Expensive animation runs once per character (~20k cells at defaults).
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
            self.gl.use_program(Some(self.program));
            self.gl.viewport(0, 0, columns as i32, rows as i32);
            self.gl.uniform_2_f32(
                self.resolution.as_ref(),
                self.width as f32,
                self.height as f32,
            );
            self.gl.uniform_1_f32(self.time.as_ref(), time);
            self.gl
                .uniform_1_f32(self.columns.as_ref(), options.columns as f32);
            self.gl
                .uniform_1_f32(self.brightness.as_ref(), options.brightness);
            self.gl.draw_arrays(glow::TRIANGLES, 0, 3);
            // Full-resolution pass only reads the cell color and glyph bitmap.
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gl.use_program(Some(self.ink_program));
            self.gl
                .viewport(0, 0, self.width as i32, self.height as i32);
            self.gl.uniform_2_f32(
                self.ink_resolution.as_ref(),
                self.width as f32,
                self.height as f32,
            );
            self.gl
                .uniform_2_f32(self.ink_grid.as_ref(), columns as f32, rows as f32);
            self.gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }

    pub fn swap(&self) -> Result<()> {
        self.egl
            .swap_buffers(self.display, self.surface)
            .context("Presenting wallpaper")
    }

    pub fn snapshot(&self, path: &Path) -> Result<()> {
        let mut pixels = vec![0; (self.width * self.height * 4) as usize];
        unsafe {
            self.gl.read_pixels(
                0,
                0,
                self.width as i32,
                self.height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
        }
        let mut image = image::RgbaImage::from_raw(self.width, self.height, pixels)
            .context("Snapshot dimensions")?;
        image::imageops::flip_vertical_in_place(&mut image);
        image
            .save(path)
            .with_context(|| format!("Saving {}", path.display()))
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            for texture in self.textures {
                self.gl.delete_texture(texture);
            }
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_program(self.program);
            self.gl.delete_program(self.ink_program);
            self.gl.delete_framebuffer(self.framebuffer);
            self.gl.delete_texture(self.cells);
        }
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_surface(self.display, self.surface);
        let _ = self.egl.destroy_context(self.display, self.context);
        let _ = self.egl.terminate(self.display);
    }
}

unsafe fn texture_parameters(gl: &glow::Context, filter: u32) {
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
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
}

fn glyph_atlas() -> Vec<u8> {
    // Original 5×7 bitmaps: space . , : ; - = + * o x % # @ M W
    let glyphs: [[u8; 7]; 16] = [
        [0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 4, 0],
        [0, 0, 0, 0, 4, 4, 8],
        [0, 4, 0, 0, 4, 0, 0],
        [0, 4, 0, 0, 4, 4, 8],
        [0, 0, 0, 31, 0, 0, 0],
        [0, 0, 31, 0, 31, 0, 0],
        [0, 4, 4, 31, 4, 4, 0],
        [0, 21, 14, 31, 14, 21, 0],
        [0, 0, 14, 17, 17, 14, 0],
        [0, 17, 10, 4, 10, 17, 0],
        [25, 26, 2, 4, 8, 11, 19],
        [10, 31, 10, 10, 31, 10, 0],
        [14, 17, 23, 21, 23, 16, 14],
        [17, 27, 21, 21, 17, 17, 17],
        [17, 17, 17, 21, 21, 27, 17],
    ];
    let mut pixels = vec![0; 128 * 12];
    for (i, glyph) in glyphs.iter().enumerate() {
        for (y, row) in glyph.iter().enumerate() {
            for x in 0..5 {
                if row & (1 << (4 - x)) != 0 {
                    pixels[(y + 2) * 128 + i * 8 + x + 1] = 255;
                }
            }
        }
    }
    pixels
}

unsafe fn compile_program(gl: &glow::Context, fragment: &str) -> Result<glow::Program> {
    let program = gl.create_program().map_err(|e| anyhow!(e))?;
    for (kind, source) in [
        (glow::VERTEX_SHADER, include_str!("../shaders/scene.vert")),
        (glow::FRAGMENT_SHADER, fragment),
    ] {
        let shader = gl.create_shader(kind).map_err(|e| anyhow!(e))?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            bail!("Shader: {}", gl.get_shader_info_log(shader));
        }
        gl.attach_shader(program, shader);
        gl.delete_shader(shader);
    }
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        bail!("Link: {}", gl.get_program_info_log(program));
    }
    Ok(program)
}
