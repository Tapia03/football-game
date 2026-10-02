//! WebGL2 through glow. Phase 3 ships only [`webgl2_smoke`], which proves the
//! whole path (canvas → WebGL2 context → GLSL ES 3.00 program → draw →
//! readback) works in the target browser. The real backend is Phase 6.

use glow::HasContext;
use wasm_bindgen::JsCast;

/// Colour the smoke shader writes; `0.5` reads back as 128 on every browser
/// we target (round-to-nearest on 8-bit unorm).
pub const SMOKE_EXPECTED_RGBA: [u8; 4] = [255, 128, 0, 255];

const VS: &str = "#version 300 es
void main() {
    // Full-screen triangle from gl_VertexID; no vertex buffers needed.
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}";

const FS: &str = "#version 300 es
precision mediump float;
out vec4 color;
void main() { color = vec4(1.0, 0.5, 0.0, 1.0); }";

/// Draws a full-screen triangle on a throwaway 4×4 canvas and returns the
/// RGBA of pixel (0, 0). Errors carry the browser's message.
///
/// # Errors
/// When WebGL2 is unavailable or a shader fails to compile/link.
pub fn webgl2_smoke() -> Result<[u8; 4], String> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .map_err(|e| format!("{e:?}"))?
        .dyn_into()
        .map_err(|_| "element is not a canvas")?;
    canvas.set_width(4);
    canvas.set_height(4);
    let ctx: web_sys::WebGl2RenderingContext = canvas
        .get_context("webgl2")
        .map_err(|e| format!("{e:?}"))?
        .ok_or("WebGL2 unavailable")?
        .dyn_into()
        .map_err(|_| "context is not WebGL2")?;
    let gl = glow::Context::from_webgl2_context(ctx);

    let mut px = [0_u8; 4];
    // SAFETY: glow's API is `unsafe` because GL objects are raw handles. All
    // handles used here are created on this context and used only on it,
    // within this function, on the thread that owns the context.
    unsafe {
        let program = gl.create_program()?;
        for (kind, src) in [(glow::VERTEX_SHADER, VS), (glow::FRAGMENT_SHADER, FS)] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, src);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }
            gl.attach_shader(program, shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        let vao = gl.create_vertex_array()?;
        gl.bind_vertex_array(Some(vao));
        gl.use_program(Some(program));
        gl.viewport(0, 0, 4, 4);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        gl.draw_arrays(glow::TRIANGLES, 0, 3);
        gl.read_pixels(
            0,
            0,
            1,
            1,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut px)),
        );
    }
    Ok(px)
}

// ---------------------------------------------------------------------------
// SPIKE (branch `spike-render`): draw one coloured triangle list on a canvas.

const MESH_VS: &str = "#version 300 es
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec4 a_col;
out vec4 v_col;
void main() { v_col = a_col; gl_Position = vec4(a_pos, 0.0, 1.0); }";

const MESH_FS: &str = "#version 300 es
precision mediump float;
in vec4 v_col;
out vec4 color;
void main() { color = v_col; }";

/// Clears the canvas `canvas_id` to `clear` and draws `verts`
/// (`[x, y, r, g, b, a]` per vertex, clip space) as triangles.
///
/// # Errors
/// When the canvas or WebGL2 is unavailable, or the shader fails.
pub fn draw_mesh(canvas_id: &str, clear: [f32; 4], verts: &[f32]) -> Result<(), String> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let canvas: web_sys::HtmlCanvasElement = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into()
        .map_err(|_| "element is not a canvas")?;
    let ctx: web_sys::WebGl2RenderingContext = canvas
        .get_context("webgl2")
        .map_err(|e| format!("{e:?}"))?
        .ok_or("WebGL2 unavailable")?
        .dyn_into()
        .map_err(|_| "context is not WebGL2")?;
    let gl = glow::Context::from_webgl2_context(ctx);
    let w = i32::try_from(canvas.width()).map_err(|_| "canvas too wide")?;
    let h = i32::try_from(canvas.height()).map_err(|_| "canvas too tall")?;
    let count = i32::try_from(verts.len() / 6).map_err(|_| "mesh too large")?;

    // SAFETY: as in `webgl2_smoke`: every GL handle is created on this
    // context and used only within this call. The byte view of `verts` is
    // valid for its whole length (f32 has no padding, u8 has alignment 1).
    unsafe {
        let program = gl.create_program()?;
        for (kind, src) in [
            (glow::VERTEX_SHADER, MESH_VS),
            (glow::FRAGMENT_SHADER, MESH_FS),
        ] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, src);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }
            gl.attach_shader(program, shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        let bytes =
            core::slice::from_raw_parts(verts.as_ptr().cast::<u8>(), core::mem::size_of_val(verts));
        let vao = gl.create_vertex_array()?;
        gl.bind_vertex_array(Some(vao));
        let vbo = gl.create_buffer()?;
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
        let stride = 6 * 4;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, stride, 2 * 4);
        gl.use_program(Some(program));
        gl.viewport(0, 0, w, h);
        gl.enable(glow::BLEND);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
        gl.clear_color(clear[0], clear[1], clear[2], clear[3]);
        gl.clear(glow::COLOR_BUFFER_BIT);
        gl.draw_arrays(glow::TRIANGLES, 0, count);
    }
    Ok(())
}
