/*
 * SPDX-FileCopyrightText: 2025 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */

//! ## High level OpenGL
//!
//! *See EFP 8 for details.*
//!
//! Current target minimal OpenGL version to support is **2.0**, with advanced features supported
//! in newer versions used only in newer versions detected.
//!
//! According to Wikipedia, the GLSL version in OpenGL 2.0 is 1.10, so version 110 would be used
//! as the base GLSL version to use for all the shaders used in the Engine.
//! If a more advanced GLSL version is supported while advanced features are used, the target
//! GLSL version should be specified and the shaders are applied only when the target version
//! is matched for both GL and GLSL versions.
//!
//! Although OpenGL 2.0 core does support **VAOs**, VAO is still used due to its simplicity and performance
//! improvement, and the complexity of cross-version maintenance. If the version is lower than 3.0,
//! the extension of `GL_ARB_vertex_array_object` is required.
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

use getset::Getters;
use gl::types::{GLenum, GLubyte, GLuint};
use gl::{ActiveTexture, AttachShader, BindBuffer, BindTexture, BindVertexArray, BlendFunc, BufferData, BufferSubData, Clear, ClearColor, CompileShader, CreateProgram, CreateShader, DeleteShader, DisableVertexAttribArray, DrawArrays, DrawElements, Enable, EnableVertexAttribArray, GenBuffers, GenVertexArrays, GetIntegerv, GetShaderInfoLog, GetShaderiv, GetString, GetStringi, GetUniformLocation, LinkProgram, ShaderSource, Uniform3fv, UniformMatrix4fv, UseProgram, VertexAttrib1d, VertexAttrib1f, VertexAttrib1s, VertexAttrib2d, VertexAttrib2f, VertexAttrib2s, VertexAttrib3d, VertexAttrib3f, VertexAttrib3s, VertexAttrib4Nub, VertexAttrib4d, VertexAttrib4f, VertexAttrib4s, VertexAttribI1i, VertexAttribI1ui, VertexAttribI2i, VertexAttribI2ui, VertexAttribI3i, VertexAttribI3ui, VertexAttribI4i, VertexAttribI4ui, VertexAttribPointer, Viewport, ARRAY_BUFFER, BLEND, BYTE, COLOR_BUFFER_BIT, COMPILE_STATUS, COMPUTE_SHADER, DOUBLE, EXTENSIONS, FALSE, FLOAT, FRAGMENT_SHADER, GEOMETRY_SHADER, INT, NUM_EXTENSIONS, ONE_MINUS_SRC_ALPHA, RENDERER, SHADING_LANGUAGE_VERSION, SHORT, SRC_ALPHA, TESS_CONTROL_SHADER, TESS_EVALUATION_SHADER, TEXTURE0, TEXTURE_2D, UNSIGNED_BYTE, UNSIGNED_INT, UNSIGNED_SHORT, VENDOR, VERSION, VERTEX_SHADER};
use num_traits::{Bounded, Num};
use regex::Regex;
use sdl3::video::GLContext;
use semver::Version;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::ffi::{c_char, CStr, CString};
use std::mem::MaybeUninit;
use std::ptr::{null, null_mut};
use std::sync::LazyLock;
use icu::datetime::fieldsets::T;
use nalgebra_glm::{TMat4, Vec3};
use sdl3::pixels::Color;
use crate::util::{str_from_c, str_to_c};

const VER_2_0: Version = Version::new(2, 0, 0);
const VER_3_0: Version = Version::new(3, 0, 0);
const VER_3_1: Version = Version::new(3, 1, 0);

/// As long as this is never mutated after creation, this **should** be *thread-safe*.
#[derive(Getters)]
pub(super) struct GLHandle {
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
	full_glsl_version: String,
	#[get = "pub"]
	glsl_version: Version,
	extensions: HashSet<String>,
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
	pub(crate) fn new(gl_context: GLContext) -> Result<Self, String> {
		let full_gl_version = get_string(VERSION);
		let full_glsl_version = get_string(SHADING_LANGUAGE_VERSION);
		let mut instance = Self {
			gl_context,
			vendor: get_string(VENDOR),
			renderer: get_string(RENDERER),
			gl_version: parse_version(&full_gl_version)?,
			full_gl_version,
			glsl_version: parse_version(&full_glsl_version)?,
			full_glsl_version,
			extensions: get_extensions(),
			features: HashSet::new(),
		};
		instance.check_requirements()?;
		setup();
		Ok(instance)
	}

	/// Since mobile platforms are not supported, OpenGL ES and OES extensions are not relevant.
	fn check_requirements(&mut self) -> Result<(), String> {
		if self.gl_version.cmp(&VER_2_0) == Ordering::Less { // < 2.0
			return Err(format!("GL {} not supported", self.gl_version));
		}

		if self.gl_version.cmp(&VER_3_0) == Ordering::Less { // < 3.0
			if !self.extensions.contains("GL_ARB_vertex_array_object") {
				return Err(format!("GL_ARB_vertex_array_object not found with GL {}", self.gl_version));
			}
		}

		if self.gl_version.cmp(&VER_3_1) == Ordering::Less { // < 3.1
			if self.extensions.contains("GL_ARB_uniform_buffer_object") {
				self.features.insert(GLFeature::Ubo);
			}
		} else {
			self.features.insert(GLFeature::Ubo);
		}

		Ok(())
	}

	pub(super) fn gl_resize_viewport(&self, width: u32, height: u32) {
		unsafe { Viewport(0, 0, width as i32, height as i32) }
	}

	pub(super) fn ubo_supported(&self) -> bool {
		self.features.contains(&GLFeature::Ubo)
	}
}

fn setup() {
	unsafe { Enable(BLEND); }
	unsafe { BlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA); }
}

fn get_string(name: GLenum) -> String {
	unsafe { str_from_gl(GetString(name)).to_string() }
}

fn get_extensions() -> HashSet<String> {
	let mut data = MaybeUninit::uninit();
	unsafe { GetIntegerv(NUM_EXTENSIONS, data.as_mut_ptr()); }
	let num = unsafe { data.assume_init() } as u32;
	let mut data = HashSet::with_capacity(num as usize);
	for i in 0..num {
		data.insert(str_from_gl(unsafe { GetStringi(EXTENSIONS, i as GLuint) }).to_string());
	}
	data
}

static VERSION_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)\.(\d+)").expect("invalid regex"));
static ES_VERSION_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^OpenGL ES (\d+)\.(\d+)").expect("invalid regex"));

/// Only parses the first two parts (major, minor) of the version string.
fn parse_version(version_str: &str) -> Result<Version, String> {
	match VERSION_REGEX.captures(version_str) {
		Some(caps) => Ok(Version::new(caps[1].parse().unwrap(), caps[2].parse().unwrap(), 0)),
		None => Err({
			if !ES_VERSION_REGEX.is_match(version_str) { panic!("invalid version string: {}", version_str); }
			format!("OpenGL ES is not supported; found: {}", version_str)
		}),
	}
}

fn str_from_gl(string: *const GLubyte) -> &'static str {
	str_from_c(string as *const _)
}

pub(crate) fn clear_canvas() {
	unsafe { Clear(COLOR_BUFFER_BIT) }
}

pub(crate) fn set_clear_color(color: (f32, f32, f32, f32)) {
	unsafe { ClearColor(color.0, color.1, color.2, color.3) }
}

/// Generate a single Buffer Object.
pub(super) fn gen_buf_obj() -> u32 {
	let mut bo = MaybeUninit::uninit();
	unsafe { GenBuffers(1, bo.as_mut_ptr()); }
	unsafe { bo.assume_init() }
}

/// Generate multiple Buffer Objects at once for optimization.
pub(super) fn gen_buf_objs<const N: usize>() -> [u32; N] {
	let mut bos = MaybeUninit::uninit();
	unsafe { GenBuffers(N as _, bos.as_mut_ptr() as *mut _); }
	unsafe { bos.assume_init() }
}

/// Generate a single Vertex Array Object.
pub(super) fn gen_vert_arr_obj() -> u32 {
	let mut vao = MaybeUninit::uninit();
	unsafe { GenVertexArrays(1, vao.as_mut_ptr()); }
	unsafe { vao.assume_init() }
}

/// Generate multiple Vertex Array Objects at once for optimization.
pub(super) fn gen_vert_arr_objs<const N: usize>() -> [u32; N] {
	let mut vaos = MaybeUninit::uninit();
	unsafe { GenVertexArrays(N as _, vaos.as_mut_ptr() as *mut _); }
	unsafe { vaos.assume_init() }
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
	fn gl_type(&self) -> GLenum {
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

pub(super) fn buf_obj_with_data<T: Number>(target: GLenum, buffer: u32, data: &[T], usage: GLenum) {
	unsafe { BindBuffer(target, buffer); }
	unsafe { BufferData(target, size_of_val(data) as _, data.as_ptr() as _, usage); }
}

pub(super) fn update_buf_obj<T: Number>(target: GLenum, buffer: u32, offset: usize, data: &[T]) {
	unsafe { BindBuffer(target, buffer); }
	unsafe { BufferSubData(target, (offset * size_of::<T>()) as _, size_of_val(data) as _, data.as_ptr() as _); }
}

/// Defines an array of Vertex Attribute. Normalized is not applied.
pub(super) fn vert_attr_arr(i: u32, vec_size: usize, kind: NumType, stride_len: usize, offset_len: usize) {
	unsafe { EnableVertexAttribArray(i); }
	unsafe {
		VertexAttribPointer(
			i,
			vec_size as _,
			kind.gl_type(),
			FALSE,
			(stride_len * kind.size()) as _,
			(offset_len * kind.size()) as _,
		);
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

pub(super) fn vert_attr(i: u32, data: VertexAttrVariant) {
	unsafe { DisableVertexAttribArray(i); }
	data.invoke_gl(i);
}

pub(super) fn with_new_vert_arr() -> u32 {
	let vao = gen_vert_arr_obj();
	unsafe { BindVertexArray(vao); }
	vao
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
	fn invoke_gl(self) -> u32 {
		unsafe {
			match self {
				ShaderType::Vertex => CreateShader(VERTEX_SHADER),
				ShaderType::Fragment => CreateShader(FRAGMENT_SHADER),
				ShaderType::Compute => CreateShader(COMPUTE_SHADER),
				ShaderType::Geometry => CreateShader(GEOMETRY_SHADER),
				ShaderType::TessControl => CreateShader(TESS_CONTROL_SHADER),
				ShaderType::TessEvaluation => CreateShader(TESS_EVALUATION_SHADER),
			}
		}
	}
}

/// `src` should not contain any `\0` char.
pub(super) fn compile_shader(src: String, kind: ShaderType) -> Result<u32, String> {
	let shader = kind.invoke_gl();
	let src = str_to_c(src);
	unsafe { ShaderSource(shader, 1, &src.as_ptr(), null()); }
	unsafe { CompileShader(shader); }
	let mut status = MaybeUninit::uninit();
	unsafe { GetShaderiv(shader, COMPILE_STATUS, status.as_mut_ptr()); }
	if unsafe { status.assume_init() } == FALSE as i32 {
		let out = CString::default().into_raw();
		unsafe { GetShaderInfoLog(shader, COMPILE_STATUS as _, null_mut(), out); }
		let out = unsafe { CString::from_raw(out) };
		return Err(out.to_str().expect("Invalid UTF-8 CString").to_string());
	}
	Ok(shader)
}

pub(super) fn new_shader_program<const N: usize>(shaders: [u32; N]) -> u32 {
	let program = unsafe { CreateProgram() };
	shaders.iter().for_each(|s| unsafe { AttachShader(program, *s) });
	unsafe { LinkProgram(program); }
	shaders.into_iter().for_each(|s| unsafe { DeleteShader(s) });
	program
}

pub(super) fn get_uniform_location(program: u32, name: &str) -> u32 {
	let name = str_to_c(name);
	unsafe { GetUniformLocation(program, name.as_ptr()) as _ }
}

pub(super) fn use_program(program: u32) {
	unsafe { UseProgram(program); }
}

/// After `use_program`
pub(super) fn use_texture_2d(texture: u32) {
	unsafe { ActiveTexture(TEXTURE0) }
	unsafe { BindTexture(TEXTURE_2D, texture); }
}

pub(super) fn use_vao(vao: u32) {
	unsafe { BindVertexArray(vao); }
}

pub(super) fn use_uniform_mat_4(i: u32, mat: &TMat4<f32>) {
	unsafe { UniformMatrix4fv(i as _, 1, FALSE, mat.as_ptr()); }
}

pub(super) fn use_uniform_vec_3(i: u32, vec: &Vec3) {
	unsafe { Uniform3fv(i as _, 1, vec.as_ptr()); }
}

pub(super) fn draw_arrays(mode: GLenum, count: u32) {
	unsafe { DrawArrays(mode, 0, count as _) }
}

pub(super) fn draw_elements(mode: GLenum, count: u32) {
	unsafe { DrawElements(mode, count as _, UNSIGNED_INT, 0 as _) }
}
