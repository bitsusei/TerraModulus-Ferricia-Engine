/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */

//! ## Rendering in 3D
//!
//! There may be a generic rendering module regardless of dimensions, to support
//! flexible rendering of 3D in 2D environment in various places and scenarios.
//! Basically, 3D rendering would be mainly in GameplayScreen, but there may be occasions
//! where 3D objects may be rendered in 2D menus with a special supporting menu object.
//!
//! Moreover, the utilities like [Geom][super::rendering::Geom] and [Mesh][super::rendering::Mesh]
//! shall be generalized for specialized supports in 2D and 3D. However, coordinates and rendering
//! in 2D should only be recommended in 2D coordinates instead of being in 3D to prevent coordination
//! space conflict between entities in different dimensional spaces, in different aspects.
//!
//! When 2D objects are not in the 3D space logically, those should be regarded like 2D objects in
//! an environment where all 2D reside. In this case, utilities should also be used in 2D way,
//! but having generic utilities without dimensional constrain (as in 3D) may be problematic while handling.
//!
//! Therefore, several rendering utilties must be separately handled in 2D and 3D rendering modules
//! with their own implementations and collections of utilities. This may result in vast codebase and engine
//! to be ported to Kryon-native interface.
//!
//! At the end of rendering, all rendering environments are summerized into a single [Canvas],
//! so basically, there is still a need to separate mathing and transformation in different environments.
//! For 2D objects, those may only exist in GUI, so they shall always be explained in screen coordinates,
//! unless a case where isolated environments are required (probably keeping the flexibility?);
//! for 3D objects, separate environments must be needed, especially for one single environment for Gameplay.
//! The 2D environment must be created when a [Canvas] is to be initialized for all GUI elements to be in.
//! A 3D rendering environment must be created for a 3D canvas in GUI, or when a World is to be initialized.
//!
//! When a [PhyEnv][crate::phy::PhyEnv] is associated with a 3D rendering environment, a helper between
//! them must be used to interpret the objects in physics into the 3D rendering representations.
//! Shader programs aid in the interpretation, for various types of rendering logics and properties,
//! including "2.5D" objects (most likely particles) where the 2D textures always face to the camera.
//!
//! [Canvas]: super::rendering::CanvasHandle
use crate::mui::ogl::{buf_obj_with_data, draw_arrays, draw_elements, gen_buf_obj, gen_buf_objs, get_uniform_location, new_shader_program, update_buf_obj, use_program, use_uniform_mat_4, vert_attr, vert_attr_arr, with_new_vert_arr, NumType, ShaderType, VertexAttrVariant};
use crate::mui::rendering::{compile_shader_from, DrawableSet, DrawingContext, Geom, GuiProgram, RenderPrimitive};
use gl::{ARRAY_BUFFER, DYNAMIC_DRAW, ELEMENT_ARRAY_BUFFER, LINES, STATIC_DRAW, TRIANGLES};
use nalgebra_glm::{identity, look_at, ortho, quat_to_mat4, rotate_x, rotation, scale, translate, DQuat, Mat4, Quat, TMat4, Vec3, Vec4};
use sdl3::pixels::Color;
use std::cell::Cell;
use std::f32::consts::PI;
use std::sync::LazyLock;
use array_macro::array;
use csgrs::mesh::Mesh;
use csgrs::mesh::vertex::Vertex;
use csgrs::traits::CSG;
use futures::StreamExt;
use getset::Getters;
use crate::FerriciaResult;

static IDENT_MAT_4: LazyLock<Mat4> = LazyLock::new(identity);
static CAMERA_UP: Vec3 = Vec3::new(0.0, 1.0, 0.0);
static ELEVATION: f32 = 100.;
static INCLINATION: LazyLock<Vec3> = LazyLock::new(|| // in World coordinate system
	(Mat4::new_rotation(Vec3::new(PI * 30. / 180., 0., 0.)) * Vec4::new(0., 1., 0., 0.)).xyz());

pub(crate) struct Camera3d {
	proj_mat: Mat4,
	view_mat: Mat4,
}

impl Camera3d {
	/// Position is the position where the Camera is at.
	pub(super) fn new(canvas_size: (u32, u32), pos: Vec3) -> Self {
		Self {
			proj_mat: ortho_proj_mat(canvas_size),
			view_mat: look_view_mat(pos),
		}
	}

	pub(super) fn refresh_canvas_size(&mut self, canvas_size: (u32, u32)) {
		self.proj_mat = ortho_proj_mat(canvas_size);
	}

	pub(crate) fn refresh_pos(&mut self, pos: Vec3) {
		self.view_mat = look_view_mat(pos);
	}

	pub(super) fn draw(&self, obj: &DrawableWorldObj, program: &impl GwrProgram) {
		obj.prim.apply_vao();
		program.uniform(&self.proj_mat, &self.view_mat, obj);
		obj.prim.draw();
	}
}

fn ortho_proj_mat(size: (u32, u32)) -> Mat4 {
	let (width, height) = size;
	ortho(0., width as _, 0., height as _, f32::NEG_INFINITY, f32::INFINITY)
}

fn look_view_mat(pos: Vec3) -> Mat4 {
	let mut eye = pos;
	eye.y = ELEVATION;
	look_at(&pos, &eye, &INCLINATION)
}

/// Gameplay World Rendering (GWR) Program
pub(crate) trait GwrProgram {
	fn id(&self) -> u32;

	fn apply(&self);

	fn uniform(&self, proj: &Mat4, view: &Mat4, obj: &DrawableWorldObj);
}

pub(crate) struct GwrGeoProgram {
	id: u32,
	model_pos: u32,
	view_pos: u32,
	projection_pos: u32,
	filter_pos: u32,
}

impl GwrGeoProgram {
	pub(crate) fn new(vsh: String, fsh: String) -> FerriciaResult<Self> {
		let id = new_shader_program([
			compile_shader_from(ShaderType::Vertex, vsh)?,
			compile_shader_from(ShaderType::Fragment, fsh)?,
		]);
		Ok(Self {
			model_pos: get_uniform_location(id, "model"),
			view_pos: get_uniform_location(id, "view"),
			projection_pos: get_uniform_location(id, "projection"),
			filter_pos: get_uniform_location(id, "filter"),
			id,
		})
	}
}

impl GwrProgram for GwrGeoProgram {
	fn id(&self) -> u32 {
		self.id
	}

	#[inline]
	fn apply(&self) {
		use_program(self.id);
	}

	fn uniform(&self, proj: &Mat4, view: &Mat4, obj: &DrawableWorldObj) {
		use_uniform_mat_4(self.projection_pos, proj);
		use_uniform_mat_4(self.view_pos, view);
		use_uniform_mat_4(self.model_pos, &obj.model);
		use_uniform_mat_4(self.filter_pos, &IDENT_MAT_4);
	}
}

pub(crate) struct DrawableWorldObj {
	prim: Box<dyn RenderPrimitive>,
	/// It is assumed that object movement has a lower updating frequency than graphics,
	/// so having computed the Model matrix per some frames may be more efficient.
	/// All parameters must be provided by Kryon for the update of this value.
	model: Mat4,
}

impl DrawableWorldObj {
	pub(crate) fn new(prim: impl RenderPrimitive + 'static) -> Self {
		// static COUNTER: AtomicUsize = AtomicUsize::new(0);
		Self {
			// id: OpaqueId::new(&COUNTER),
			prim: Box::new(prim),
			model: *IDENT_MAT_4,
			// _pin: PhantomPinned,
		}
	}

	/// Scaling matrix from `[-1, 1]` to a form; for example, times .5 to size of one meter.
	/// Position must be based on values of the Center of Gravity (CoG).
	pub fn update_model(&mut self, pos: Vec3, q: DQuat, scaling: Vec3) {
		let m = quat_to_mat4(&q).cast(); // Rotation
		let m = scale(&m, &scaling); // Scaling
		self.model = translate(&m, &pos); // Translation from Origin to World Coordinates by CoG
	}
}

// Before having a better methodology, coordinates of vertices in RenderPrimitive must be in [-1, 1]
// to be transformed by a Model matrix to World coordinates, including its object size in World.

/// Linear Geom with only two points and one color. This uses `LINES`.
pub(crate) struct SimpleLine3dGeom {
	vao: u32,
	vbo: u32,
	color: Color,
}

impl SimpleLine3dGeom {
	const NUM_VERTICES: u32 = 2;
	pub(crate) fn new(points: [Vec3; 2], color: Color) -> Self {
		let vao = with_new_vert_arr();
		let vbo = gen_buf_obj();
		let vertices = [
			points[0].x, points[0].y, points[0].z,
			points[1].x, points[1].y, points[1].z,
		];
		buf_obj_with_data(ARRAY_BUFFER, vbo, &vertices, DYNAMIC_DRAW);
		vert_attr_arr(0, 3, NumType::Float, 3, 0); // Position
		Self { vao, vbo, color } // Note: Binding to the VAO remains
	}
}

impl RenderPrimitive for SimpleLine3dGeom {
	fn vao(&self) -> u32 {
		self.vao
	}

	fn draw(&self) {
		vert_attr(1, VertexAttrVariant::UbyteNorm4.call(self.color.rgba())); // Color
		draw_arrays(LINES, Self::NUM_VERTICES);
	}

	unsafe fn set_pos_f32(&self, vec: &[f32]) {
		assert_eq!(vec.len(), 3 * Self::NUM_VERTICES as usize);
		update_buf_obj(ARRAY_BUFFER, self.vbo, 0, vec);
	}

	unsafe fn set_pos_f64(&self, _vec: &[f64]) {
		unimplemented!("Unsupported")
	}
}

impl Geom for SimpleLine3dGeom {}

pub(crate) struct SimpleQuad3dGeom {
	vao: u32,
	vbo: u32,
	ebo: u32,
	color: Color,
}

impl SimpleQuad3dGeom {
	const INDICES: [u32; Self::NUM_ELEMENTS as usize] = [
		0, 1, 2, // first triangle
		0, 2, 3  // second triangle
	];

	const NUM_ELEMENTS: u32 = 6;

	pub(crate) fn new(points: [Vec3; 4], color: Color) -> Self {
		let vao = with_new_vert_arr();
		let [vbo, ebo] = gen_buf_objs();
		let vertices = points.iter().flat_map(|e| e.as_slice()).cloned().collect::<Vec<_>>();
		buf_obj_with_data(ARRAY_BUFFER, vbo, vertices.as_slice(), DYNAMIC_DRAW);
		buf_obj_with_data(ELEMENT_ARRAY_BUFFER, ebo, &Self::INDICES, STATIC_DRAW);
		vert_attr_arr(0, 3, NumType::Float, 3, 0); // Position
		Self { vao, vbo, ebo, color } // Note: Binding to the VAO remains
	}
}

impl RenderPrimitive for SimpleQuad3dGeom {
	fn vao(&self) -> u32 {
		self.vao
	}

	fn draw(&self) {
		vert_attr(1, VertexAttrVariant::UbyteNorm4.call(self.color.rgba())); // Color
		draw_elements(TRIANGLES, Self::NUM_ELEMENTS);
	}

	unsafe fn set_pos_f32(&self, vec: &[f32]) {
		assert_eq!(vec.len(), 12);
		update_buf_obj(ARRAY_BUFFER, self.vbo, 0, vec);
	}

	unsafe fn set_pos_f64(&self, _vec: &[f64]) {
		unimplemented!("Unsupported")
	}
}

impl Geom for SimpleQuad3dGeom {}

pub(crate) struct SimpleBox3dGeom {
	vao: u32,
	vbo: u32,
	ebo: u32,
	color: Color,
}

impl SimpleBox3dGeom {
	// 24 Vertices from 12 Triangles; each two triangles form one face
	const INDICES: [u32; Self::NUM_ELEMENTS as usize] = array![x => ((x / 6 * 4) + match x % 3 {
		1|2 if (x / 3) % 2 == 1 => (x % 3) + 1, // Refers to SimpleQuad3dGeom::INDICES
		_ => x % 3,
	}) as u32; Self::NUM_ELEMENTS as usize];

	const NUM_ELEMENTS: u32 = 36; // Each triangle contains three elements

	pub(crate) fn new(points: [Vec3; 2], color: Color) -> Self {
		todo!("TBA");
	}
}

/// Utilizing CSG's [Mesh]
pub(crate) struct SimpleMesh3dGeom {
	vao: u32,
	vbo: u32,
	ebo: u32,
	mesh: Mesh<()>,
	num_vertices: u32,
	color: Color,
}

impl SimpleMesh3dGeom {
	pub(crate) fn new_cube(width: f32, color: Color) -> Self {
		// Has to be centered for Rotation matrix to work correctly, if correct.
		let mesh = Mesh::cube(width, None).translate(-width / 2.0, -width / 2.0, -width / 2.0);
		Self::new_mesh_centered(mesh, color)
	}

	fn new_mesh_centered(mesh: Mesh<()>, color: Color) -> Self {
		let vao = with_new_vert_arr();
		let [vbo, ebo] = gen_buf_objs();
		let tri_mesh = mesh.to_trimesh().unwrap();
		buf_obj_with_data(ARRAY_BUFFER, vbo, tri_mesh.vertices().into_iter().flatten().collect(), DYNAMIC_DRAW);
		buf_obj_with_data(ELEMENT_ARRAY_BUFFER, ebo, tri_mesh.indices().as_flattened(), STATIC_DRAW);
		vert_attr_arr(0, 3, NumType::Float, 3, 0);
		Self { vao, vbo, ebo, num_vertices: tri_mesh.vertices().len() as u32, mesh, color }
	}

	pub(crate) fn new_sphere(radius: f32, color: Color) -> Self {
		let mesh = Mesh::sphere(radius, 20, 10, None).translate(-radius, -radius, -radius);
		Self::new_mesh_centered(mesh, color)
	}
}

impl RenderPrimitive for SimpleMesh3dGeom {
	fn vao(&self) -> u32 {
		self.vao
	}

	fn draw(&self) {
		vert_attr(1, VertexAttrVariant::UbyteNorm4.call(self.color.rgba())); // Color
		draw_elements(TRIANGLES, self.num_vertices);
	}

	unsafe fn set_pos_f32(&self, _vec: &[f32]) {
		unimplemented!("Unsupported")
	}

	unsafe fn set_pos_f64(&self, _vec: &[f64]) {
		unimplemented!("Unsupported")
	}
}

impl Geom for SimpleMesh3dGeom {}
