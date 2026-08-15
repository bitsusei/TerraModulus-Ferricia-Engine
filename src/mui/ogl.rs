/*
 * SPDX-FileCopyrightText: 2025 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */

//! ## High level OpenGL
//!
//! *See EFP 9 for details.*
//!
//! To not waste GL callsites, **clearing bindings** is generally not done, so when mutations are
//! done, one should ensure that the target desired object has already been bound.
//! All renderings should use VAOs regardless to keep uniform patterns across the Engine.
//! Please keep in mind that binding existing VAOs may replace the states of EBO binding and
//! vertex attributes when they were set, but not for VBOs, so this must be taken carefully.
//! During rendering using VAOs, all the used objects must be bound in the VAO as desired.
//!
//! Due to performance improvement, **UBOs** are used for cross-frame constants like the projection matrix.
//! This feature is introduced in OpenGL 3.1, but existed as the `GL_ARB_uniform_buffer_object` extension.
//! For versions prior to 3.1, the extension is required to simplify the amount of work;
//! otherwise, regular uniforms are used instead.
//!
//! When using functions from extensions, extension functions (handled by `glow`) shall be used.
//! This behavior shall be abstract out by using traits implemented across different OpenGL versions.

use getset::Getters;
use gl::{VertexAttrib1d, VertexAttrib1f, VertexAttrib1s, VertexAttrib2d, VertexAttrib2f, VertexAttrib2s, VertexAttrib3d, VertexAttrib3f, VertexAttrib3s, VertexAttrib4Nub, VertexAttrib4d, VertexAttrib4f, VertexAttrib4s, VertexAttribI1i, VertexAttribI1ui, VertexAttribI2i, VertexAttribI2ui, VertexAttribI3i, VertexAttribI3ui, VertexAttribI4i, VertexAttribI4ui};
use glow::{Buffer, Context, HasContext, PixelUnpackData, Program, Shader, Texture, UniformLocation, VertexArray, BGR, BGRA, BLEND, BYTE, CLAMP_TO_EDGE, COLOR_BUFFER_BIT, COMPUTE_SHADER, DOUBLE, FLOAT, FRAGMENT_SHADER, GEOMETRY_SHADER, INT, MULTISAMPLE, NEAREST, NEAREST_MIPMAP_LINEAR, ONE_MINUS_SRC_ALPHA, RENDERER, RGB, RGB10, RGB10_A2, RGB12, RGB16, RGB16F, RGB32F, RGB8, RGBA, RGBA12, RGBA16, RGBA16F, RGBA32F, RGBA8, SHADING_LANGUAGE_VERSION, SHORT, SRC_ALPHA, SRGB, SRGB8, SRGB8_ALPHA8, SRGB_ALPHA, TESS_CONTROL_SHADER, TESS_EVALUATION_SHADER, TEXTURE0, TEXTURE_2D, TEXTURE_MAG_FILTER, TEXTURE_MIN_FILTER, TEXTURE_WRAP_S, TEXTURE_WRAP_T, UNSIGNED_BYTE, UNSIGNED_INT, UNSIGNED_SHORT, VENDOR, VERSION, VERTEX_SHADER};
use nalgebra_glm::{TMat4, Vec3};
use num_traits::{Bounded, Num};
use regex::Regex;
use sdl3::video::GLContext;
use semver::Version;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::slice;
use std::sync::LazyLock;

const VER_2_0: Version = Version::new(2, 0, 0);
const VER_3_0: Version = Version::new(3, 0, 0);
const VER_3_1: Version = Version::new(3, 1, 0);

/// As long as this is never mutated after creation, this **should** be *thread-safe*.
#[derive(Getters)]
pub(super) struct GLHandle {
	gl: Context,
	gl_context: GLContext,
	#[get = "pub"]
	vendor: String,
	#[get = "pub"]
	renderer: String,
	#[get = "pub"]
	full_gl_version: String,
	#[get = "pub"]
	gl_version: Version,
	#[get = "pub"]
	glsl_version: Version,
	platform_extensions: HashSet<String>,
	features: HashSet<GLFeature>,
}

unsafe impl Send for GLHandle {}

unsafe impl Sync for GLHandle {}

#[derive(Hash, PartialEq, Eq, Copy, Clone, Debug)]
enum GLFeature {
	Ubo,
}

/// Supposed to be **immutable**.
impl GLHandle {
	/// Make sure context is current and function pointer is handled before this.
	pub(crate) fn new(gl_context: GLContext, gl: Context) -> Result<Self, String> {
		let full_glsl_version = unsafe { gl.get_parameter_string(SHADING_LANGUAGE_VERSION) };
		let gl_version = gl.version();
		if gl_version.is_embedded {
			return Err(format!("OpenGL ES is not supported; found: {}", gl_version))
		}
		let handle = Self {
			gl_context,
			vendor: unsafe { gl.get_parameter_string(VENDOR) },
			renderer: unsafe { gl.get_parameter_string(RENDERER) },
			platform_extensions: get_platform_extensions(),
			full_gl_version: unsafe { gl.get_parameter_string(VERSION) },
			gl,
			gl_version: Version::new(gl_version.major as _, gl_version.minor as _, gl_version.revision.unwrap_or(0) as _),
			glsl_version: parse_version(&full_glsl_version)?,
			features: HashSet::new(),
		};
		let mut instance = handle;
		instance.check_requirements()?;
		instance.setup();
		Ok(instance)
	}

	fn setup(&self) {
		unsafe { self.gl.enable(BLEND); }
		unsafe { self.gl.enable(MULTISAMPLE); }
		unsafe { self.gl.blend_func(SRC_ALPHA, ONE_MINUS_SRC_ALPHA); }
	}

	/// Since mobile platforms are not supported, OpenGL ES and OES extensions are not relevant.
	fn check_requirements(&mut self) -> Result<(), String> {
		if self.gl_version.cmp(&VER_2_0) == Ordering::Less { // < 2.0
			return Err(format!("GL {} not supported", self.gl_version));
		}

		if self.gl_version.cmp(&VER_3_0) == Ordering::Less { // < 3.0
			if !self.platform_extensions.contains("GL_ARB_vertex_array_object") {
				return Err(format!("GL_ARB_vertex_array_object not found with GL {}", self.gl_version));
			}
		}

		if self.gl_version.cmp(&VER_3_1) == Ordering::Less { // < 3.1
			if self.platform_extensions.contains("GL_ARB_uniform_buffer_object") {
				self.features.insert(GLFeature::Ubo);
			}
		} else {
			self.features.insert(GLFeature::Ubo);
		}

		Ok(())
	}

	pub(super) fn gl_resize_viewport(&self, width: u32, height: u32) {
		unsafe { self.gl.viewport(0, 0, width as i32, height as i32) }
	}

	pub(super) fn ubo_supported(&self) -> bool {
		self.features.contains(&GLFeature::Ubo)
	}

	pub(crate) fn clear_canvas(&self) {
		unsafe { self.gl.clear(COLOR_BUFFER_BIT) }
	}

	pub(crate) fn set_clear_color(&self, color: (f32, f32, f32, f32)) {
		unsafe { self.gl.clear_color(color.0, color.1, color.2, color.3) }
	}

	/// Generate a single Buffer Object.
	pub(super) fn gen_buf_obj(&self) -> Buffer {
		unsafe { self.gl.create_buffer().unwrap() }
	}

	/// Generate multiple Buffer Objects at once for optimization.
	pub(super) fn gen_buf_objs<const N: usize>(&self) -> [Buffer; N] {
		std::array::from_fn(|_| self.gen_buf_obj())
	}

	/// Generate a single Vertex Array Object.
	pub(super) fn gen_vert_arr_obj(&self) -> VertexArray {
		unsafe { self.gl.create_vertex_array().unwrap() }
	}

	/// Generate multiple Vertex Array Objects at once for optimization.
	pub(super) fn gen_vert_arr_objs<const N: usize>(&self) -> [VertexArray; N] {
		std::array::from_fn(|_| self.gen_vert_arr_obj())
	}

	pub(super) fn buf_obj_with_data<T: Number>(&self, target: u32, buffer: Buffer, data: &[T], usage: u32) {
		unsafe { self.gl.bind_buffer(target, Some(buffer)) }
		unsafe { self.gl.buffer_data_u8_slice(target, slice_to_u8_slice(data), usage) }
	}

	pub(super) fn update_buf_obj<T: Number>(&self, target: u32, buffer: Buffer, offset: usize, data: &[T]) {
		unsafe { self.gl.bind_buffer(target, Some(buffer)) }
		unsafe { self.gl.buffer_sub_data_u8_slice(target, (offset * size_of::<T>()) as _, slice_to_u8_slice(data)) }
	}

	/// Defines an array of Vertex Attribute. Normalized is not applied.
	pub(super) fn vert_attr_arr(&self, i: u32, vec_size: usize, kind: NumType, stride_len: usize, offset_len: usize) {
		unsafe { self.gl.enable_vertex_attrib_array(i) }
		unsafe {
			self.gl.vertex_attrib_pointer_f32(
				i,
				vec_size as _,
				kind.gl_type(),
				false,
				(stride_len * kind.size()) as _,
				(offset_len * kind.size()) as _,
			)
		}
	}

	pub(super) fn vert_attr(&self, i: u32, data: VertexAttrVariant) {
		unsafe { self.gl.disable_vertex_attrib_array(i) }
		data.invoke_gl(i);
	}

	pub(super) fn with_new_vert_arr(&self) -> VertexArray {
		let vao = self.gen_vert_arr_obj();
		unsafe { self.gl.bind_vertex_array(Some(vao)) }
		vao
	}

	/// `src` should not contain any `\0` char.
	pub(super) fn compile_shader(&self, src: String, kind: ShaderType) -> Result<Shader, String> {
		let shader = kind.invoke_gl(&self.gl);
		unsafe { self.gl.shader_source(shader, &src) }
		unsafe { self.gl.compile_shader(shader) }
		unsafe {
			if !self.gl.get_shader_compile_status(shader) {
				Err(self.gl.get_shader_info_log(shader))
			} else {
				Ok(shader)
			}
		}
	}

	pub(super) fn new_shader_program<const N: usize>(&self, shaders: [Shader; N]) -> Program {
		let program = unsafe { self.gl.create_program().unwrap() };
		shaders.iter().for_each(|s| unsafe { self.gl.attach_shader(program, *s) });
		unsafe { self.gl.link_program(program); }
		shaders.into_iter().for_each(|s| unsafe { self.gl.delete_shader(s) });
		program
	}

	pub(super) fn get_uniform_location(&self, program: Program, name: &str) -> UniformLocation {
		unsafe { self.gl.get_uniform_location(program, name).unwrap() }
	}

	pub(super) fn use_program(&self, program: Program) {
		unsafe { self.gl.use_program(Some(program)) }
	}

	/// After `use_program`
	pub(super) fn use_texture_2d(&self, texture: Texture) {
		unsafe { self.gl.active_texture(TEXTURE0) }
		unsafe { self.gl.bind_texture(TEXTURE_2D, Some(texture)) }
	}

	pub(super) fn use_vao(&self, vao: VertexArray) {
		unsafe { self.gl.bind_vertex_array(Some(vao)) }
	}

	pub(super) fn use_uniform_mat_4(&self, i: &UniformLocation, mat: &TMat4<f32>) {
		unsafe { self.gl.uniform_matrix_4_f32_slice(Some(i), false, mat.as_slice()) }
	}

	pub(super) fn use_uniform_vec_3(&self, i: &UniformLocation, vec: &Vec3) {
		unsafe { self.gl.uniform_3_f32_slice(Some(i), vec.as_slice()) }
	}

	pub(super) fn draw_arrays(&self, mode: u32, count: u32) {
		unsafe { self.gl.draw_arrays(mode, 0, count as _) }
	}

	pub(super) fn draw_elements(&self, mode: u32, count: u32) {
		unsafe { self.gl.draw_elements(mode, count as _, UNSIGNED_INT, 0) }
	}

	pub(super) fn new_sprite_texture(&self, tex_src: TexSrc, itn_tex_fmt: Option<ItnTexFmt>) -> Texture {
		unsafe {
			let tex = self.gl.create_texture().unwrap();
			self.gl.bind_texture(TEXTURE_2D, Some(tex));
			self.gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE as _);
			self.gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE as _);
			self.gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MIN_FILTER, NEAREST_MIPMAP_LINEAR as _);
			self.gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MAG_FILTER, NEAREST as _);
			self.gl.tex_image_2d(
				TEXTURE_2D,
				0,
				match itn_tex_fmt {
					None => match tex_src.tex_fmt {
						SrcTexFmt::Rgb | SrcTexFmt::Bgr => match tex_src.tex_typ {
							SrcTexTyp::Byte | SrcTexTyp::UnsignedByte => RGB8,
							SrcTexTyp::Short | SrcTexTyp::UnsignedShort => RGB16,
							SrcTexTyp::Int | SrcTexTyp::UnsignedInt | SrcTexTyp::Float => RGB32F,
						}
						SrcTexFmt::Rgba | SrcTexFmt::Bgra => match tex_src.tex_typ {
							SrcTexTyp::Byte | SrcTexTyp::UnsignedByte => RGBA8,
							SrcTexTyp::Short | SrcTexTyp::UnsignedShort => RGBA16,
							SrcTexTyp::Int | SrcTexTyp::UnsignedInt | SrcTexTyp::Float => RGBA32F,
						}
					},
					Some(x) => x.into_gl(),
				} as _,
				tex_src.width as _,
				tex_src.height as _,
				0,
				tex_src.tex_fmt.into_gl(),
				tex_src.tex_typ.into_gl(),
				PixelUnpackData::Slice(Some(tex_src.data)),
			);
			self.gl.generate_mipmap(TEXTURE_2D);
			tex
		}
	}
}

fn slice_to_u8_slice<T>(data: &[T]) -> &[u8] {
	bytemuck::cast_slice(data)
	// unsafe {
	// 	slice::from_raw_parts(
	// 		data.as_ptr() as *const u8,
	// 		size_of_val(data),
	// 	)
	// }
}

/// Retrieve WGL/GLX extensions
fn get_platform_extensions() -> HashSet<String> {
	todo!()
}

static VERSION_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)\.(\d+)").expect("invalid regex"));
static ES_VERSION_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^OpenGL ES (\d+)\.(\d+)").expect("invalid regex"));

/// Only parses the first two parts (major, minor) of the version string.
fn parse_version(version_str: &str) -> Result<Version, String> {
	match VERSION_REGEX.captures(version_str) {
		Some(caps) => Ok(Version::new(caps[1].parse()?, caps[2].parse()?, 0)),
		None => Err({
			if !ES_VERSION_REGEX.is_match(version_str) { panic!("invalid version string: {}", version_str); }
			format!("OpenGL ES is not supported; found: {}", version_str)
		}),
	}
}

pub(super) struct TexSrc<'a> {
	width: u32,
	height: u32,
	data: &'a [u8],
	tex_fmt: SrcTexFmt,
	tex_typ: SrcTexTyp,
}

impl TexSrc<'_> {
	pub(super) fn new(width: u32, height: u32, data: &[u8], tex_fmt: SrcTexFmt, tex_typ: SrcTexTyp) -> Self {
		Self {
			width,
			height,
			data,
			tex_fmt,
			tex_typ,
		}
	}
}

pub(super) enum SrcTexFmt {
	Rgb,
	Bgr,
	Rgba,
	Bgra,
}

impl SrcTexFmt {
	fn into_gl(self) -> u32 {
		match self {
			SrcTexFmt::Rgb => RGB,
			SrcTexFmt::Bgr => BGR,
			SrcTexFmt::Rgba => RGBA,
			SrcTexFmt::Bgra => BGRA,
		}
	}
}

pub(super) enum SrcTexTyp {
	Byte,
	UnsignedByte,
	Short,
	UnsignedShort,
	Int,
	UnsignedInt,
	Float,
}

impl SrcTexTyp {
	fn into_gl(self) -> u32 {
		match self {
			SrcTexTyp::Byte => BYTE,
			SrcTexTyp::UnsignedByte => UNSIGNED_BYTE,
			SrcTexTyp::Short => SHORT,
			SrcTexTyp::UnsignedShort => UNSIGNED_SHORT,
			SrcTexTyp::Int => INT,
			SrcTexTyp::UnsignedInt => UNSIGNED_INT,
			SrcTexTyp::Float => FLOAT,
		}
	}
}

/// Compression is separately handled
pub(super) enum ItnTexFmt {
	Rgb,
	Rgba,
	Rgb8,
	Rgba8,
	Rgb10,
	Rgba10, // 2-bit A
	Rgb12,
	Rgba12,
	Rgb16,
	Rgba16,
	Rgb16F,
	Rgba16F,
	Rgb32F,
	Rgba32F,
	Srgb,
	Srgb8,
	Srgba,
	Srgba8,
}

impl ItnTexFmt {
	fn into_gl(self) -> u32 {
		match self {
			ItnTexFmt::Rgb => RGB,
			ItnTexFmt::Rgba => RGBA,
			ItnTexFmt::Rgb8 => RGB8,
			ItnTexFmt::Rgba8 => RGBA8,
			ItnTexFmt::Rgb10 => RGB10,
			ItnTexFmt::Rgba10 => RGB10_A2,
			ItnTexFmt::Srgb => SRGB,
			ItnTexFmt::Srgb8 => SRGB8,
			ItnTexFmt::Srgba => SRGB_ALPHA,
			ItnTexFmt::Srgba8 => SRGB8_ALPHA8,
			ItnTexFmt::Rgb12 => RGB12,
			ItnTexFmt::Rgba12 => RGBA12,
			ItnTexFmt::Rgb16 => RGB16,
			ItnTexFmt::Rgba16 => RGBA16,
			ItnTexFmt::Rgb16F => RGB16F,
			ItnTexFmt::Rgba16F => RGBA16F,
			ItnTexFmt::Rgb32F => RGB32F,
			ItnTexFmt::Rgba32F => RGBA32F,
		}
	}
}

/// It is better to precompress before passing to GPU.
/// However, on CUDA platforms, it is possible to perform efficient compression on GPU.
/// BPTC requires `EXT_texture_compression_bptc`/`ARB_texture_compression_bptc`.
/// S3TC requires `EXT_texture_compression_s3tc `.
pub(super) enum CmpItnTexFmt {
	RgbBptc,
	RgbS3tc,
	RgbaBptc,
	RgbaS3tc,
	SrgbBptc,
	SrgbS3tc,
	SrgbSaBptc,
	SrgbSaS3tc,
	SrgbUaBptc,
	SrgbUaS3tc,
}

pub(super) trait Number : Num + Bounded {}

impl<T: Num + Bounded> Number for T {}

pub(super) enum NumType {
	Byte,
	UnsignedByte,
	Short,
	UnsignedShort,
	Int,
	UnsignedInt,
	Float,
	Double,
	// Other types are skipped due to ages and compatibility.
}

impl NumType {
	#[inline]
	fn size(&self) -> usize {
		match self {
			NumType::Byte => size_of::<i8>(),
			NumType::UnsignedByte => size_of::<u8>(),
			NumType::Short => size_of::<i16>(),
			NumType::UnsignedShort => size_of::<u16>(),
			NumType::Int => size_of::<i32>(),
			NumType::UnsignedInt => size_of::<u32>(),
			NumType::Float => size_of::<f32>(),
			NumType::Double => size_of::<f64>(),
		}
	}

	#[inline]
	fn gl_type(&self) -> u32 {
		match self {
			NumType::Byte => BYTE,
			NumType::UnsignedByte => UNSIGNED_BYTE,
			NumType::Short => SHORT,
			NumType::UnsignedShort => UNSIGNED_SHORT,
			NumType::Int => INT,
			NumType::UnsignedInt => UNSIGNED_INT,
			NumType::Float => FLOAT,
			NumType::Double => DOUBLE,
		}
	}
}

pub(super) enum VertexAttrVariant {
	Float1(f32),
	Short1(i16),
	Double1(f64),
	Int1(i32),
	Uint1(u32),
	Float2(f32, f32),
	Short2(i16, i16),
	Double2(f64, f64),
	Int2(i32, i32),
	Uint2(u32, u32),
	Float3(f32, f32, f32),
	Short3(i16, i16, i16),
	Double3(f64, f64, f64),
	Int3(i32, i32, i32),
	Uint3(u32, u32, u32),
	Float4(f32, f32, f32, f32),
	Short4(i16, i16, i16, i16),
	Double4(f64, f64, f64, f64),
	Int4(i32, i32, i32, i32),
	Uint4(u32, u32, u32, u32),
	UbyteNorm4(u8, u8, u8, u8),
	// `v`, `L` and `P` variants are ignored.
}

/// Functions from `glow` are incomplete for this set of API, so `gl` is used.
impl VertexAttrVariant {
	fn invoke_gl(self, i: u32) {
		match self {
			VertexAttrVariant::Float1(a) => unsafe {
				VertexAttrib1f(i, a);
			}
			VertexAttrVariant::Short1(a) => unsafe {
				VertexAttrib1s(i, a);
			}
			VertexAttrVariant::Double1(a) => unsafe {
				VertexAttrib1d(i, a);
			}
			VertexAttrVariant::Int1(a) => unsafe {
				VertexAttribI1i(i, a);
			}
			VertexAttrVariant::Uint1(a) => unsafe {
				VertexAttribI1ui(i, a);
			}
			VertexAttrVariant::Float2(a, b) => unsafe {
				VertexAttrib2f(i, a, b);
			}
			VertexAttrVariant::Short2(a, b) => unsafe {
				VertexAttrib2s(i, a, b);
			}
			VertexAttrVariant::Double2(a, b) => unsafe {
				VertexAttrib2d(i, a, b);
			}
			VertexAttrVariant::Int2(a, b) => unsafe {
				VertexAttribI2i(i, a, b);
			}
			VertexAttrVariant::Uint2(a, b) => unsafe {
				VertexAttribI2ui(i, a, b);
			}
			VertexAttrVariant::Float3(a, b, c) => unsafe {
				VertexAttrib3f(i, a, b, c);
			}
			VertexAttrVariant::Short3(a, b, c) => unsafe {
				VertexAttrib3s(i, a, b, c);
			}
			VertexAttrVariant::Double3(a, b, c) => unsafe {
				VertexAttrib3d(i, a, b, c);
			}
			VertexAttrVariant::Int3(a, b, c) => unsafe {
				VertexAttribI3i(i, a, b, c);
			}
			VertexAttrVariant::Uint3(a, b, c) => unsafe {
				VertexAttribI3ui(i, a, b, c);
			}
			VertexAttrVariant::Float4(a, b, c, d) => unsafe {
				VertexAttrib4f(i, a, b, c, d);
			}
			VertexAttrVariant::Short4(a, b, c, d) => unsafe {
				VertexAttrib4s(i, a, b, c, d);
			}
			VertexAttrVariant::Double4(a, b, c, d) => unsafe {
				VertexAttrib4d(i, a, b, c, d);
			}
			VertexAttrVariant::Int4(a, b, c, d) => unsafe {
				VertexAttribI4i(i, a, b, c, d);
			}
			VertexAttrVariant::Uint4(a, b, c, d) => unsafe {
				VertexAttribI4ui(i, a, b, c, d);
			}
			VertexAttrVariant::UbyteNorm4(a, b, c, d) => unsafe {
				VertexAttrib4Nub(i, a, b, c, d);
			}
		}
	}
}

pub(super) enum ShaderType {
	Vertex,
	Fragment,
	Compute,
	Geometry,
	TessControl,
	TessEvaluation,
}

impl ShaderType {
	fn invoke_gl(self, gl: &Context) -> Shader {
		unsafe {
			match self {
				ShaderType::Vertex => gl.create_shader(VERTEX_SHADER).unwrap(),
				ShaderType::Fragment => gl.create_shader(FRAGMENT_SHADER).unwrap(),
				ShaderType::Compute => gl.create_shader(COMPUTE_SHADER).unwrap(),
				ShaderType::Geometry => gl.create_shader(GEOMETRY_SHADER).unwrap(),
				ShaderType::TessControl => gl.create_shader(TESS_CONTROL_SHADER).unwrap(),
				ShaderType::TessEvaluation => gl.create_shader(TESS_EVALUATION_SHADER).unwrap(),
			}
		}
	}
}
