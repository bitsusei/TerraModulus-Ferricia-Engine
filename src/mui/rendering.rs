/*
 * SPDX-FileCopyrightText: 2025 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */

#![allow(private_interfaces)]

use crate::mui::ogl::{GLHandle, NumType, ShaderType, SrcTexFmt, SrcTexTyp, TexSrc, VertexAttrVariant};
use crate::mui::window::WindowHandle;
use crate::FerriciaResult;
use glow::{Buffer, NativeTexture, NativeVertexArray, Program, Shader, Texture, UniformLocation, VertexArray, ARRAY_BUFFER, DYNAMIC_DRAW, ELEMENT_ARRAY_BUFFER, LINES, STATIC_DRAW, TRIANGLES};
use image::imageops::flip_vertical_in_place;
use image::{DynamicImage, EncodableLayout, ImageBuffer, ImageReader, Rgb, Rgb32FImage, RgbImage, Rgba, Rgba32FImage, RgbaImage};
use nalgebra_glm::{identity, mat3_to_mat4, ortho, rotation2d, scaling, translation, DVec3, Mat4, TMat4, Vec3};
use ordermap::OrderSet;
use sdl3::pixels::Color;
use std::any::Any;
use std::borrow::Cow;
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::num::NonZeroU32;
use std::ptr;
use std::sync::{Arc, LazyLock};

static IDENT_MAT_4: LazyLock<TMat4<f32>> = LazyLock::new(identity);

pub(crate) struct CanvasHandle {
	/// Size of Canvas in pixels
	size: (u32, u32),
	ortho_proj_mat: TMat4<f32>,
	// drawable_sets: HashMap<OpaqueId, DrawableSet>,
	used_program: Cell<u32>,
	/// DO NOT MUTATE
	gl_handle: Arc<GLHandle>,
}

impl CanvasHandle {
	pub(crate) fn new(window_handle: &WindowHandle) -> Self {
		let size = window_handle.window_size_in_pixels();
		let gl_handle = window_handle.gl_handle().clone();
		Self {
			ortho_proj_mat: ortho_proj_mat(size),
			size,
			gl_handle,
			used_program: Cell::new(0),
			// drawable_sets: HashMap::new(),
		}
	}

	// pub(crate) fn new_drawable_set(&mut self, prim: impl RenderPrimitive + 'static) -> &DrawableSet {
	// 	let set = DrawableSet::new(prim);
	// 	let id = set.id;
	// 	if let Some(v) = self.drawable_sets.insert(set.id, set) {
	// 		panic!("{:?} should be unique", v.id)
	// 	}
	// 	self.drawable_sets.get(&id).expect("should exist")
	// }

	pub(crate) fn load_image(&self, data: &[u8]) -> Texture {
		let mut img = ImageReader::new(Cursor::new(data))
			.with_guessed_format()
			.expect("unknown format")
			.decode()
			.expect("Cannot decode image");
		// Image coordinates have a difference direction as OpenGL texture coordinates.
		flip_vertical_in_place(&mut img);
		fn from_rgb8(img: RgbImage) -> TexSrc {
			TexSrc::new(img.width(), img.height(), img.as_bytes(), SrcTexFmt::Rgb, SrcTexTyp::UnsignedByte)
		}
		fn from_rgba8(img: RgbaImage) -> TexSrc {
			TexSrc::new(img.width(), img.height(), img.as_bytes(), SrcTexFmt::Rgba, SrcTexTyp::UnsignedByte)
		}
		fn from_rgb16(img: ImageBuffer<Rgb<u16>, Vec<u16>>) -> TexSrc {
			TexSrc::new(img.width(), img.height(), img.as_bytes(), SrcTexFmt::Rgb, SrcTexTyp::UnsignedShort)
		}
		fn from_rgba16(img: ImageBuffer<Rgba<u16>, Vec<u16>>) -> TexSrc {
			TexSrc::new(img.width(), img.height(), img.as_bytes(), SrcTexFmt::Rgba, SrcTexTyp::UnsignedShort)
		}
		fn from_rgb32f(img: Rgb32FImage) -> TexSrc {
			TexSrc::new(img.width(), img.height(), img.as_bytes(), SrcTexFmt::Rgb, SrcTexTyp::Float)
		}
		fn from_rgba32f(img: Rgba32FImage) -> TexSrc {
			TexSrc::new(img.width(), img.height(), img.as_bytes(), SrcTexFmt::Rgba, SrcTexTyp::Float)
		}
		self.gl_handle.new_sprite_texture(match img {
			DynamicImage::ImageLuma8(_) => from_rgb8(img.into_rgb8()),
			DynamicImage::ImageLumaA8(_) => from_rgba8(img.into_rgba8()),
			DynamicImage::ImageRgb8(img) => from_rgb8(img),
			DynamicImage::ImageRgba8(img) => from_rgba8(img),
			DynamicImage::ImageLuma16(_) => from_rgb16(img.into_rgb16()),
			DynamicImage::ImageLumaA16(_) => from_rgba16(img.into_rgba16()),
			DynamicImage::ImageRgb16(img) => from_rgb16(img),
			DynamicImage::ImageRgba16(img) => from_rgba16(img),
			DynamicImage::ImageRgb32F(img) => from_rgb32f(img),
			DynamicImage::ImageRgba32F(img) => from_rgba32f(img),
			_ => unimplemented!(),
		}, None)
	}

	pub(crate) fn refresh_canvas_size(&mut self, width: u32, height: u32, camera: Option<&mut Camera3d>) {
		self.size = (width, height);
		self.ortho_proj_mat = ortho_proj_mat(self.size);
		if let Some(camera) = camera {
			camera.refresh_canvas_size(self.size)
		}
	}

	pub(crate) fn new_camera(&self, pos: Vec3) -> Camera3d {
		Camera3d::new(self.size, pos)
	}

	pub(crate) fn draw_gwr(&self, gl: &GLHandle, camera: &Camera3d, obj: &DrawableWorldObj, program: &impl GwrProgram) {
		if self.used_program.get() != program.id() {
			program.apply(gl);
			self.used_program.set(program.id());
		}

		camera.draw(gl, obj, program);
	}

	pub(crate) fn draw_gui(&self, set: &DrawableSet, program: &impl GuiProgram, texture: Option<u32>) {
		if self.used_program.get() != program.id() {
			program.apply(&self.gl_handle);
			self.used_program.set(program.id());
		}

		if let Some(v) = texture {
			self.gl_handle.use_texture_2d(NativeTexture(NonZeroU32::new(v).unwrap()));
		}

		set.prim.apply_vao(&self.gl_handle);
		let context = DrawingContext { window_size: &self.size };
		program.uniform(&self.gl_handle, &self.ortho_proj_mat, set, context);
		set.prim.draw(&self.gl_handle);
	}
}

use crate::mui::rendering3d::{Camera3d, DrawableWorldObj, GwrProgram};

pub(super) struct DrawingContext<'a> {
	window_size: &'a (u32, u32),
}

/// Usage: `unsafe { UniformMatrix4fv(0, 1, FALSE, ortho.as_ptr()) }`
///
/// This may be an identity matrix if no model/view matrix is supplied.
fn ortho_proj_mat(size: (u32, u32)) -> TMat4<f32> {
	let (width, height) = size;
	ortho::<f32>(0., width as _, 0., height as _, -1., 1.)
}

/// Not in production
pub(super) fn compile_shader_from(gl: &GLHandle, kind: ShaderType, src: String) -> FerriciaResult<Shader> {
	Ok(gl.compile_shader(src, kind)?)
}

pub(crate) trait GuiProgram {
	fn id(&self) -> u32;

	fn apply(&self, gl: &GLHandle);

	fn uniform(&self, gl: &GLHandle, proj: &TMat4<f32>, set: &DrawableSet, drawing_context: DrawingContext);
}

pub(crate) struct GeoProgram {
	id: Program,
	model_pos: UniformLocation,
	projection_pos: UniformLocation,
	filter_pos: UniformLocation,
}

impl GeoProgram {
	pub(crate) fn new(gl: &GLHandle, vsh: String, fsh: String) -> FerriciaResult<Self> {
		let id = gl.new_shader_program([
			compile_shader_from(gl, ShaderType::Vertex, vsh)?,
			compile_shader_from(gl, ShaderType::Fragment, fsh)?,
		]);
		Ok(Self {
			model_pos: gl.get_uniform_location(id, "model"),
			projection_pos: gl.get_uniform_location(id, "projection"),
			filter_pos: gl.get_uniform_location(id, "filter"),
			id,
		})
	}
}

impl GuiProgram for GeoProgram {
	fn id(&self) -> u32 {
		self.id.0.get()
	}

	#[inline]
	fn apply(&self, gl: &GLHandle) {
		gl.use_program(self.id);
	}

	fn uniform(&self, gl: &GLHandle, proj: &TMat4<f32>, set: &DrawableSet, drawing_context: DrawingContext) {
		gl.use_uniform_mat_4(&self.projection_pos, proj);
		let model = set.eval_model_mat(&drawing_context);
		gl.use_uniform_mat_4(&self.model_pos, model.as_ref());
		let filter = set.eval_filter_mat(&drawing_context);
		gl.use_uniform_mat_4(&self.filter_pos, filter.as_ref());
	}
}

pub(crate) struct TexProgram {
	id: Program,
	model_pos: UniformLocation,
	projection_pos: UniformLocation,
	filter_pos: UniformLocation,
}

impl TexProgram {
	pub(crate) fn new(gl: &GLHandle, vsh: String, fsh: String) -> FerriciaResult<Self> {
		let id = gl.new_shader_program([
			compile_shader_from(gl, ShaderType::Vertex, vsh)?,
			compile_shader_from(gl, ShaderType::Fragment, fsh)?,
		]);
		Ok(Self {
			model_pos: gl.get_uniform_location(id, "model"),
			projection_pos: gl.get_uniform_location(id, "projection"),
			filter_pos: gl.get_uniform_location(id, "filter"),
			id,
		})
	}
}

impl GuiProgram for TexProgram {
	fn id(&self) -> u32 {
		self.id.0.get()
	}

	#[inline]
	fn apply(&self, gl: &GLHandle) {
		gl.use_program(self.id);
	}

	fn uniform(&self, gl: &GLHandle, proj: &TMat4<f32>, set: &DrawableSet, drawing_context: DrawingContext) {
		gl.use_uniform_mat_4(&self.projection_pos, proj);
		let model = set.eval_model_mat(&drawing_context);
		gl.use_uniform_mat_4(&self.model_pos, model.as_ref());
		let filter = set.eval_filter_mat(&drawing_context);
		gl.use_uniform_mat_4(&self.filter_pos, filter.as_ref());
	}
}

// /// All the state data visible to the rendering (main) thread to be processed.
// /// All data access must be done using `Mutex` locks; should not be accessed twice
// /// at the same time in the same thread, as it might panic.
// struct RenderingState {
// 	lock: Mutex<()>,
// }
//
// impl RenderingState {
// 	fn new() -> Self {
// 		Self {
// 			lock: Mutex::new(()),
// 		}
// 	}
// }
//
// /// A sequence of links to `DrawableSet` that are drawn by this instance.
// pub(crate) struct DrawingLinks<'a> {
// 	id: OpaqueId,
// 	links: HashMap<OpaqueId, DrawingNode<'a>>,
// }
//
// impl DrawingLinks<'_> {
// 	fn new() -> Self {
// 		static COUNTER: AtomicUsize = AtomicUsize::new(0);
// 		Self {
// 			id: OpaqueId::new(&COUNTER),
// 			links: HashMap::new(),
// 		}
// 	}
//
// 	fn add_link(node: DrawingNode) {
//
// 	}
// }
//
// enum DrawingNode<'a> {
// 	Links(&'a DrawingLinks<'a>),
// 	Set(&'a DrawableSet),
// }
//
// impl<'a> DrawingNode<'a> {
// 	fn id(&self) -> OpaqueId {
// 		match self {
// 			DrawingNode::Links(v) => v.id,
// 			DrawingNode::Set(v) => v.id,
// 		}
// 	}
// }

/// A set of data that is completely drawable for an instance with all the information available.
///
/// The functions of model and filter additions and removals are made generalized using
/// experimental features which may not have guarantees.
/// See [Rust RFC 2580](https://rust-lang.github.io/rfcs/2580-ptr-meta.html) for details.
// #[derive(Getters)]
pub(crate) struct DrawableSet<'a> {
	// #[getset(get = "pub")]
	// id: OpaqueId,
	prim: Box<dyn RenderPrimitive>,
	models: OrderSet<&'a dyn PrimModelTransform>,
	filters: OrderSet<&'a dyn PrimColorFilter>,
	// _pin: PhantomPinned,
}

impl<'a> DrawableSet<'a> {
	pub(crate) fn new(prim: impl RenderPrimitive + 'static) -> Self {
		// static COUNTER: AtomicUsize = AtomicUsize::new(0);
		Self {
			// id: OpaqueId::new(&COUNTER),
			prim: Box::new(prim),
			models: OrderSet::new(),
			filters: OrderSet::new(),
			// _pin: PhantomPinned,
		}
	}

	/// Requires careful management
	pub(crate) unsafe fn prim<T: RenderPrimitive>(&mut self) -> &mut T {
		unsafe { (self.prim.as_mut() as &mut dyn Any).downcast_unchecked_mut() }
	}

	pub(crate) unsafe fn set_prim_pos(&self, gl: &GLHandle, pos: &[f32]) {
		unsafe { self.prim.set_pos_f32(gl, pos) }
	}

	pub(crate) fn add_model_transform<'b: 'a>(&mut self, transform: &'b dyn PrimModelTransform) {
		self.models.insert(transform);
	}

	pub(crate) fn remove_model_transform<'b: 'a>(&mut self, transform: &'b dyn PrimModelTransform) {
		self.models.remove(&transform);
	}

	pub(crate) fn add_filter_transform<'b: 'a>(&mut self, filter: &'b dyn PrimColorFilter) {
		self.filters.insert(filter);
	}

	pub(crate) fn remove_filter_transform<'b: 'a>(&mut self, filter: &'b dyn PrimColorFilter) {
		self.filters.remove(&filter);
	}

	fn eval_model_mat(&self, drawing_context: &DrawingContext) -> Cow<TMat4<f32>> {
		if self.models.is_empty() {
			Cow::Borrowed(&*IDENT_MAT_4)
		} else {
			let mut it = self.models.iter();
			let first = it.next().unwrap();
			Cow::Owned(it.fold(first.model_matrix(drawing_context), |m1, m2| m2.model_matrix(drawing_context) * m1))
		}
	}

	fn eval_filter_mat(&self, drawing_context: &DrawingContext) -> Cow<TMat4<f32>> {
		if self.filters.is_empty() {
			Cow::Borrowed(&*IDENT_MAT_4)
		} else {
			let mut it = self.filters.iter();
			let first = it.next().unwrap();
			Cow::Owned(it.fold(first.filter_matrix(drawing_context), |m1, m2| m2.filter_matrix(drawing_context) * m1))
		}
	}
}

pub(crate) trait RenderPrimitive : Any {
	fn vao(&self) -> u32;

	#[inline]
	fn apply_vao(&self, gl: &GLHandle) {
		gl.use_vao(NativeVertexArray(NonZeroU32::new(self.vao()).unwrap()));
	}

	fn draw(&self, gl: &GLHandle);

	unsafe fn set_pos_f32(&self, gl: &GLHandle, vec: &[f32]);

	unsafe fn set_pos_f64(&self, vec: &[f64]);
}

/// All `Geom`s take coordinates as screen coordinates.
pub(super) trait Geom : RenderPrimitive {

}

/// Linear Geom with only two points and one color. This uses `LINES`.
pub(crate) struct SimpleLineGeom {
	vao: VertexArray,
	vbo: Buffer,
	color: Color,
}

impl SimpleLineGeom {
	const NUM_VERTICES: u32 = 2;
	pub(crate) fn new(gl: &GLHandle, points: [(f32, f32); 2], color: Color) -> Self {
		let vao = gl.with_new_vert_arr();
		let vbo = gl.gen_buf_obj();
		let vertices = [
			points[0].0, points[0].1,
			points[1].0, points[1].1,
		];
		gl.buf_obj_with_data(ARRAY_BUFFER, vbo, &vertices, DYNAMIC_DRAW);
		gl.vert_attr_arr(0, 2, NumType::Float, 2, 0); // Position
		Self { vao, vbo, color } // Note: Binding to the VAO remains
	}
}

impl RenderPrimitive for SimpleLineGeom {
	fn vao(&self) -> u32 {
		self.vao.0.get()
	}

	fn draw(&self, gl: &GLHandle) {
		gl.vert_attr(1, VertexAttrVariant::UbyteNorm4.call(self.color.rgba())); // Color
		gl.draw_arrays(LINES, Self::NUM_VERTICES);
	}

	unsafe fn set_pos_f32(&self, gl: &GLHandle, vec: &[f32]) {
		assert_eq!(vec.len(), 2 * Self::NUM_VERTICES as usize);
		gl.update_buf_obj(ARRAY_BUFFER, self.vbo, 0, vec);
	}

	unsafe fn set_pos_f64(&self, _vec: &[f64]) {
		unimplemented!("Unsupported")
	}
}

impl Geom for SimpleLineGeom {}

pub(crate) struct SimpleRectGeom {
	vao: VertexArray,
	vbo: Buffer,
	ebo: Buffer,
	color: Color,
}

impl SimpleRectGeom {
	const INDICES: [u32; 6] = [
		0, 1, 2, // first triangle
		0, 2, 3  // second triangle
	];

	const NUM_ELEMENTS: u32 = 6;

	/// `[x0, y0, x1, y1]`; (0, 0) as bottom-left
	pub(crate) fn new(gl: &GLHandle, points: [f32; 4], color: Color) -> Self {
		let vao = gl.with_new_vert_arr();
		let [vbo, ebo] = gl.gen_buf_objs();
		let vertices = [
			points[0], points[3], // top-left
			points[0], points[1], // bottom-left
			points[2], points[1], // bottom-right
			points[2], points[3], // top-right
		];
		gl.buf_obj_with_data(ARRAY_BUFFER, vbo, &vertices, DYNAMIC_DRAW);
		gl.buf_obj_with_data(ELEMENT_ARRAY_BUFFER, ebo, &Self::INDICES, STATIC_DRAW);
		gl.vert_attr_arr(0, 2, NumType::Float, 2, 0); // Position
		Self { vao, vbo, ebo, color } // Note: Binding to the VAO remains
	}
}

impl RenderPrimitive for SimpleRectGeom {
	fn vao(&self) -> u32 {
		self.vao.0.get()
	}

	fn draw(&self, gl: &GLHandle) {
		gl.vert_attr(1, VertexAttrVariant::UbyteNorm4.call(self.color.rgba())); // Color
		gl.draw_elements(TRIANGLES, Self::NUM_ELEMENTS);
	}

	unsafe fn set_pos_f32(&self, gl: &GLHandle, vec: &[f32]) {
		assert_eq!(vec.len(), 4);
		gl.update_buf_obj(ARRAY_BUFFER, self.vbo, 0, &[
			vec[0], vec[3], // top-left
			vec[0], vec[1], // bottom-left
			vec[2], vec[1], // bottom-right
			vec[2], vec[3], // top-right
		]);
	}

	unsafe fn set_pos_f64(&self, _vec: &[f64]) {
		unimplemented!("Unsupported")
	}
}

impl Geom for SimpleRectGeom {}

trait Mesh : RenderPrimitive {

}

/// Simplest form of a **Mesh**
pub(crate) struct SpriteMesh {
	vao: VertexArray,
	vbo: Buffer,
	ebo: Buffer,
}

impl SpriteMesh {
	const INDICES: [u32; 6] = [
		0, 1, 2, // first triangle
		0, 2, 3  // second triangle
	];

	const NUM_ELEMENTS: u32 = 6;

	/// `[x0, y0, x1, y1]`; (0, 0) as bottom-left
	pub(crate) fn new(gl: &GLHandle, points: [u32; 4]) -> Self {
		let vao = gl.with_new_vert_arr();
		let [vbo, ebo] = gl.gen_buf_objs();
		let vertices: [f32; 16] = [
			// positions
			points[0] as _, points[3] as _, // top-left
			points[0] as _, points[1] as _, // bottom-left
			points[2] as _, points[1] as _, // bottom-right
			points[2] as _, points[3] as _, // top-right
			// tex coords
			0.0, 1.0, // top-left
			0.0, 0.0, // bottom-left
			1.0, 0.0, // bottom-right
			1.0, 1.0, // top-right
		];
		gl.buf_obj_with_data(ARRAY_BUFFER, vbo, &vertices, DYNAMIC_DRAW);
		gl.buf_obj_with_data(ELEMENT_ARRAY_BUFFER, ebo, &Self::INDICES, STATIC_DRAW);
		gl.vert_attr_arr(0, 2, NumType::Float, 2, 0); // Position
		gl.vert_attr_arr(1, 2, NumType::Float, 2, 8); // Texture coord
		Self { vao, vbo, ebo } // Note: Binding to the VAO remains
	}
}

impl Mesh for SpriteMesh {}

impl RenderPrimitive for SpriteMesh {
	fn vao(&self) -> u32 {
		self.vao.0.get()
	}

	fn draw(&self, gl: &GLHandle) {
		gl.draw_elements(TRIANGLES, Self::NUM_ELEMENTS);
	}

	unsafe fn set_pos_f32(&self, gl: &GLHandle, vec: &[f32]) {
		assert_eq!(vec.len(), 4);
		gl.update_buf_obj(ARRAY_BUFFER, self.vbo, 0, &[
			vec[0], vec[3], // top-left
			vec[0], vec[1], // bottom-left
			vec[2], vec[1], // bottom-right
			vec[2], vec[3], // top-right
		]);
	}

	unsafe fn set_pos_f64(&self, _vec: &[f64]) {
		unimplemented!("Unsupported")
	}
}

pub(crate) trait PrimModelTransform {
	fn model_matrix(&self, drawing_context: &DrawingContext) -> TMat4<f32>;
}

impl Hash for &dyn PrimModelTransform {
	fn hash<H: Hasher>(&self, state: &mut H) {
		ptr::hash(self, state);
	}
}

impl PartialEq for &dyn PrimModelTransform {
	fn eq(&self, other: &Self) -> bool {
		ptr::eq(self, other)
	}
}

impl Eq for &dyn PrimModelTransform {}

pub(crate) struct GeneralTransform {
	scale: DVec3,
	angle: f64,
	pos: DVec3,
	model: Mat4,
}

impl GeneralTransform {
	pub fn new(scale: DVec3, angle: f64, pos: DVec3) -> Self {
		Self {
			model: Self::eval_model(&scale, angle, &pos),
			scale,
			angle,
			pos,
		}
	}

	fn eval_model(scale: &DVec3, angle: f64, pos: &DVec3) -> Mat4 {
		let r = rotation2d(angle);
		(translation(&pos) * mat3_to_mat4(&r) * scaling(&scale)).cast() // SRT
	}

	fn update_model(&mut self) {
		self.model = Self::eval_model(&self.scale, self.angle, &self.pos);
	}

	pub fn update(&mut self, scale: DVec3, angle: f64, pos: DVec3) {
		self.model = Self::eval_model(&scale, angle, &pos);
		self.scale = scale;
		self.angle = angle;
		self.pos = pos;
	}

	pub fn update_scale(&mut self, scale: DVec3) {
		self.scale = scale;
		self.update_model();
	}

	pub fn update_angle(&mut self, angle: f64) {
		self.angle = angle;
		self.update_model();
	}

	pub fn update_pos(&mut self, pos: DVec3) {
		self.pos = pos;
		self.update_model();
	}

	pub fn update_scale_angle(&mut self, scale: DVec3, angle: f64) {
		self.scale = scale;
		self.angle = angle;
		self.update_model();
	}
}

impl PrimModelTransform for GeneralTransform {
	fn model_matrix(&self, drawing_context: &DrawingContext) -> TMat4<f32> {
		self.model
	}
}

pub(crate) trait PrimColorFilter {
	fn filter_matrix(&self, drawing_context: &DrawingContext) -> TMat4<f32>;
}

impl Hash for &dyn PrimColorFilter {
	fn hash<H: Hasher>(&self, state: &mut H) {
		ptr::hash(self, state);
	}
}

impl PartialEq for &dyn PrimColorFilter {
	fn eq(&self, other: &Self) -> bool {
		ptr::eq(self, other)
	}
}

impl Eq for &dyn PrimColorFilter {}

pub(crate) struct AlphaFilter {
	alpha: f32,
}

impl AlphaFilter {
	pub(crate) fn new(alpha: f32) -> Self {
		Self { alpha }
	}

	pub(crate) fn set_alpha(&mut self, alpha: f32) {
		self.alpha = alpha;
	}
}

impl PrimColorFilter for AlphaFilter {
	fn filter_matrix(&self, _drawing_context: &DrawingContext) -> TMat4<f32> {
		let mut mat = *IDENT_MAT_4;
		mat.m44 = self.alpha;
		mat
	}
}
