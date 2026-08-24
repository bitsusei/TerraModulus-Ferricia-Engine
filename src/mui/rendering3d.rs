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
use std::any::Any;
use std::num::NonZeroU32;
use crate::mui::ogl::{GLHandle, NumType, ShaderType, VertexAttrVariant};
use crate::mui::rendering::{compile_shader_from};
use crate::FerriciaResult;
use array_macro::array;
use csgrs::mesh::Mesh;
use csgrs::traits::CSG;
use gl::{ARRAY_BUFFER, DYNAMIC_DRAW, ELEMENT_ARRAY_BUFFER, LINES, STATIC_DRAW, TRIANGLES};
use nalgebra_glm::{identity, look_at, quat_to_mat4, scale, scaling, translate, translation, DMat4, DQuat, DVec3, Mat4, Vec3, Vec4};
use sdl3::pixels::Color;
use std::sync::LazyLock;
use glow::{Buffer, NativeVertexArray, Program, UniformLocation, VertexArray};
use num_traits::FloatConst;

static IDENT_MAT_4: LazyLock<Mat4> = LazyLock::new(identity);
/// It should be rotating about x-axis with -60 degrees, but somehow this function is treating
/// parameters the opposite sign. This literally means 60 degrees, but when computed,
/// it is the result as if the value of -60 degrees is inputted.
static CAMERA_DIR: LazyLock<DMat4> = LazyLock::new(|| DMat4::new_rotation(DVec3::new(f64::PI() / 3., 0., 0.)));
/// The direction of light pointing South with 45 degrees of depression.
static LIGHT_DIR: LazyLock<Vec3> = LazyLock::new(|| Vec3::new(0., -1., 1.).normalize());
static STANDARD_SCALING: f32 = 64.;

pub(crate) struct Camera3d {
	proj_mat: Mat4,
	view_mat: Mat4,
	canvas_size: (u32, u32),
	zoom_level: f32,
}

impl Camera3d {
	/// Position is the position where the Camera is at (position of the Player character).
	pub(super) fn new(canvas_size: (u32, u32), pos: Vec3) -> Self {
		Self {
			proj_mat: ortho_proj_mat(canvas_size, STANDARD_SCALING),
			view_mat: look_view_mat(pos),
			canvas_size,
			zoom_level: 1.0,
		}
	}

	pub(super) fn refresh_canvas_size(&mut self, canvas_size: (u32, u32)) {
		self.proj_mat = ortho_proj_mat(canvas_size, self.zoom_level * STANDARD_SCALING);
		self.canvas_size = canvas_size;
	}

	pub(crate) fn refresh_pos(&mut self, pos: Vec3) {
		self.view_mat = look_view_mat(pos);
	}

	/// Zoom level is the factor based on the Standard Scaling.
	pub(crate) fn set_zoom_level(&mut self, zoom_level: f32) {
		self.proj_mat = ortho_proj_mat(self.canvas_size, zoom_level * STANDARD_SCALING);
		self.zoom_level = zoom_level;
	}

	pub(super) fn draw(&self, gl: &GLHandle, obj: &DrawableWorldObj, program: &impl GwrProgram) {
		obj.prim.apply_vao(&gl);
		program.uniform(gl, &self.proj_mat, &self.view_mat, obj);
		obj.prim.draw(&gl, &obj.efx);
	}
}

fn ortho_proj_mat(size: (u32, u32), scale: f32) -> Mat4 {
	let (width, height) = size;
	let scale = Vec3::new(scale, scale, 0.).cast();
	// Centering offset of Camera
	let offset = DVec3::new(width as f64 / 2., height as f64 / 2., 0.);
	// Using GLM's `ortho` causes problematic result on INF by (0, width, 0, height, -INF, INF)
	// Where bottom-left is the origin,
	// [ 2/(r-l),        0,        0, -(r+l)/(r-l),
	//          0, 2/(t-b),        0, -(t+b)/(t-b),
	//          0,       0, -2/(f-n), -(f+n)/(f-n),
	//          0,       0,        0,            1 ]
	// Substitute w=r, l=0, h=t, b=0, f=∞ and n=-∞,
	// [ 2/w,   0,        0,         -w/w,
	//     0, 2/h,        0,         -h/h,
	//     0,   0, -2/(∞+∞), -(∞-∞)/(∞+∞),
	//     0,   0,        0,            1 ]
	// Note that lim -2/(∞+∞) = lim -1/∞ = 0,
	// and lim -(∞-∞)/(∞+∞) = lim -0/2∞ = 0:
	// [ 2/w,   0, 0, -1,
	//     0, 2/h, 0, -1,
	//     0,   0, 0,  0,
	//     0,   0, 0,  1 ]
	(DMat4::new(
		2. / width as f64, 0., 0., -1.,
		0., 2. / height as f64, 0., -1.,
		0., 0., 0., 0.,
		0., 0., 0., 1.,
	) * translation(&offset) * scaling(&scale)).cast()
}

fn look_view_mat(mut pos: Vec3) -> Mat4 {
	// Originally, look_at(&pos, &CAMERA_TARGET, &CAMERA_UP) was used, but the result is awkward.
	// Where:
	//   INCLINATION = Mat4::new_rotation(Vec3::new(PI / 3., 0., 0.));
	//   CAMERA_UP = (INCLINATION * Vec4::new(0., 0., -1., 0.)).xyz();
	//   CAMERA_TARGET = (INCLINATION * Vec4::new(0., -1., 0., 0.)).xyz();
	// This means, applying a rotation about x-axis for PI/3 (60 degrees) on the camera axes that
	// the up axis is negative z-axis and the target axis is negative y-axis.
	// Instead, directly applying a rotation about the original camera (facing -z with up +y)
	// would be used. Without applying tilting, it should be -90 degrees about x-axis,
	// then the camera faces -y with up -z; after applying tilting,
	// it becomes -60 degrees about x-axis, with 30-degree rotation about x-axis on the former.
	// Inversion of coordinates is done to cancel out the offsets of camera.
	pos = -pos;
	(*CAMERA_DIR * translation(&pos.cast())).cast()
}

/// Gameplay World Rendering (GWR) Program
pub(crate) trait GwrProgram {
	fn id(&self) -> u32;

	fn apply(&self, gl: &GLHandle);

	fn uniform(&self, gl: &GLHandle, proj: &Mat4, view: &Mat4, obj: &DrawableWorldObj);
}

pub(crate) struct GwrGeoProgram {
	id: Program,
	model_pos: UniformLocation,
	view_pos: UniformLocation,
	projection_pos: UniformLocation,
	filter_pos: UniformLocation,
	light_dir_pos: UniformLocation,
}

impl GwrGeoProgram {
	pub(crate) fn new(gl: &GLHandle, vsh: String, fsh: String) -> FerriciaResult<Self> {
		let id = gl.new_shader_program([
			compile_shader_from(gl, ShaderType::Vertex, vsh)?,
			compile_shader_from(gl, ShaderType::Fragment, fsh)?,
		]);
		Ok(Self {
			model_pos: gl.get_uniform_location(id, "model"),
			view_pos: gl.get_uniform_location(id, "view"),
			projection_pos: gl.get_uniform_location(id, "projection"),
			filter_pos: gl.get_uniform_location(id, "filter"),
			light_dir_pos: gl.get_uniform_location(id, "lightDir"),
			id,
		})
	}
}

impl GwrProgram for GwrGeoProgram {
	fn id(&self) -> u32 {
		self.id.0.get()
	}

	#[inline]
	fn apply(&self, gl: &GLHandle) {
		gl.use_program(self.id);
	}

	fn uniform(&self, gl: &GLHandle, proj: &Mat4, view: &Mat4, obj: &DrawableWorldObj) {
		gl.use_uniform_mat_4(&self.projection_pos, proj);
		gl.use_uniform_mat_4(&self.view_pos, view);
		gl.use_uniform_mat_4(&self.model_pos, &obj.model);
		gl.use_uniform_mat_4(&self.filter_pos, &IDENT_MAT_4);
		gl.use_uniform_vec_3(&self.light_dir_pos, &LIGHT_DIR);
	}
}

/// Buffers and Vertices of this remain immutable
pub(crate) trait Render3dPrimitive: Any {
	fn vao(&self) -> u32;

	#[inline]
	fn apply_vao(&self, gl: &GLHandle) {
		gl.use_vao(NativeVertexArray(NonZeroU32::new(self.vao()).unwrap()));
	}

	fn draw(&self, gl: &GLHandle, efx: &Render3DEfx);
}

pub(crate) enum Render3DEfx {
	Color(Color),
}

pub(super) trait Geom : Render3dPrimitive {}

pub(crate) struct DrawableWorldObj<'a> {
	prim: &'a dyn Render3dPrimitive,
	efx: Render3DEfx,
	/// It is assumed that object movement has a lower updating frequency than graphics,
	/// so having computed the Model matrix per some frames may be more efficient.
	/// All parameters must be provided by Kryon for the update of this value.
	model: Mat4,
}

impl<'a> DrawableWorldObj<'a> {
	pub(crate) fn new(prim: &'a(dyn Render3dPrimitive + 'static), efx: Render3DEfx) -> Self {
		// static COUNTER: AtomicUsize = AtomicUsize::new(0);
		Self {
			// id: OpaqueId::new(&COUNTER),
			prim,
			efx,
			model: *IDENT_MAT_4,
			// _pin: PhantomPinned,
		}
	}

	/// Scaling matrix from `[-1, 1]` to a form; for example, times .5 to size of one meter.
	/// Position must be based on values of the Center of Gravity (CoG).
	pub fn update_model(&mut self, pos: DVec3, q: DQuat, scale: DVec3) {
		// Translation from Origin to World Coordinates by CoG, after scaling and rotation
		self.model = (translation(&pos) * quat_to_mat4(&q) * scaling(&scale)).cast();
	}
}

// Before having a better methodology, coordinates of vertices in RenderPrimitive must be in [-1, 1]
// to be transformed by a Model matrix to World coordinates, including its object size in World.

/// Linear Geom with only two points and one color. This uses `LINES`.
pub(crate) struct SimpleLine3dGeom {
	gl: &'static GLHandle,
	vao: VertexArray,
	vbo: Buffer,
}

impl SimpleLine3dGeom {
	const NUM_VERTICES: u32 = 2;
	pub(crate) fn new(gl: &'static GLHandle, points: [Vec3; 2]) -> Self {
		let vao = gl.with_new_vert_arr();
		let vbo = gl.gen_buf_obj();
		let vertices = [
			points[0].x, points[0].y, points[0].z,
			points[1].x, points[1].y, points[1].z,
		];
		gl.buf_obj_with_data(ARRAY_BUFFER, vbo, &vertices, DYNAMIC_DRAW);
		gl.vert_attr_arr(0, 3, NumType::Float, 3, 0); // Position
		Self { gl, vao, vbo } // Note: Binding to the VAO remains
	}
}

impl Render3dPrimitive for SimpleLine3dGeom {
	fn vao(&self) -> u32 {
		self.vao.0.get()
	}

	fn draw(&self, gl: &GLHandle, efx: &Render3DEfx) {
		gl.vert_attr(1, VertexAttrVariant::Float3(0., 0., 0.));
		gl.vert_attr(2, VertexAttrVariant::UbyteNorm4.call(match efx {
			Render3DEfx::Color(color) => color.rgba(),
		})); // Color
		gl.draw_arrays(LINES, Self::NUM_VERTICES);
	}
}

impl Geom for SimpleLine3dGeom {}

pub(crate) struct SimpleQuad3dGeom {
	gl: &'static GLHandle,
	vao: VertexArray,
	vbo: Buffer,
	ebo: Buffer,
}

impl SimpleQuad3dGeom {
	const INDICES: [u32; Self::NUM_ELEMENTS as usize] = [
		0, 1, 2, // first triangle
		0, 2, 3  // second triangle
	];

	const NUM_ELEMENTS: u32 = 6;

	pub(crate) fn new(gl: &'static GLHandle, points: [Vec3; 4]) -> Self {
		let vao = gl.with_new_vert_arr();
		let [vbo, ebo] = gl.gen_buf_objs();
		let vertices = points.iter().flat_map(|e| e.as_slice()).cloned().collect::<Vec<_>>();
		gl.buf_obj_with_data(ARRAY_BUFFER, vbo, vertices.as_slice(), DYNAMIC_DRAW);
		gl.buf_obj_with_data(ELEMENT_ARRAY_BUFFER, ebo, &Self::INDICES, STATIC_DRAW);
		gl.vert_attr_arr(0, 3, NumType::Float, 3, 0); // Position
		Self { gl, vao, vbo, ebo } // Note: Binding to the VAO remains
	}
}

impl Render3dPrimitive for SimpleQuad3dGeom {
	fn vao(&self) -> u32 {
		self.vao.0.get()
	}

	fn draw(&self, gl: &GLHandle, efx: &Render3DEfx) {
		gl.vert_attr(1, VertexAttrVariant::Float3(0., 0., 0.));
		gl.vert_attr(2, VertexAttrVariant::UbyteNorm4.call(match efx {
			Render3DEfx::Color(color) => color.rgba(),
		})); // Color
		gl.draw_elements(TRIANGLES, Self::NUM_ELEMENTS);
	}
}

impl Geom for SimpleQuad3dGeom {}

pub(crate) struct SimpleBox3dGeom {
	vao: VertexArray,
	vbo: Buffer,
	ebo: Buffer,
}

impl SimpleBox3dGeom {
	// 24 Vertices from 12 Triangles; each two triangles form one face
	const INDICES: [u32; Self::NUM_ELEMENTS as usize] = array![x => ((x / 6 * 4) + match x % 3 {
		1|2 if (x / 3) % 2 == 1 => (x % 3) + 1, // Refers to SimpleQuad3dGeom::INDICES
		_ => x % 3,
	}) as u32; Self::NUM_ELEMENTS as usize];

	const NUM_ELEMENTS: u32 = 36; // Each triangle contains three elements

	pub(crate) fn new(points: [Vec3; 2]) -> Self {
		todo!("TBA");
	}
}

/// Utilizing CSG's [Mesh]
pub(crate) struct SimpleMesh3dGeom {
	vao: VertexArray,
	vbo: Buffer,
	ebo: Buffer,
	mesh: Mesh<()>,
	num_vertices: u32,
}

impl SimpleMesh3dGeom {
	pub(crate) fn new_cube(gl: &GLHandle, width: f32) -> Self {
		// Has to be centered for Rotation matrix to work correctly, if correct.
		let mesh = Mesh::cube(width, None).translate(-width / 2.0, -width / 2.0, -width / 2.0);
		Self::new_mesh_centered(gl, mesh)
	}

	fn new_mesh_centered(gl: &GLHandle, mesh: Mesh<()>) -> Self {
		let vao = gl.with_new_vert_arr();
		let [vbo, ebo] = gl.gen_buf_objs();
		// Refers to Mesh::get_vertices_and_indices
		let tri_csg = mesh.triangulate();
		let vertices = tri_csg
			.polygons
			.iter()
			.flat_map(|p| [
				p.vertices[0].pos.iter(),
				p.vertices[0].normal.iter(),
				p.vertices[1].pos.iter(),
				p.vertices[1].normal.iter(),
				p.vertices[2].pos.iter(),
				p.vertices[2].normal.iter(),
			])
			.flatten()
			.cloned()
			.collect::<Vec<_>>();
		let indices = (0..tri_csg.polygons.len())
			.flat_map(|i| {
				let offset = i as u32 * 3;
				[offset, offset + 1, offset + 2]
			})
			.collect::<Vec<_>>();
		gl.buf_obj_with_data(ARRAY_BUFFER, vbo, vertices.as_slice(), DYNAMIC_DRAW);
		gl.buf_obj_with_data(ELEMENT_ARRAY_BUFFER, ebo, indices.as_slice(), STATIC_DRAW);
		gl.vert_attr_arr(0, 3, NumType::Float, 6, 0);
		gl.vert_attr_arr(1, 3, NumType::Float, 6, 3);
		Self { vao, vbo, ebo, num_vertices: (vertices.len() / 2) as u32, mesh }
	}

	pub(crate) fn new_sphere(gl: &GLHandle, radius: f32) -> Self {
		let mesh = Mesh::sphere(radius, 20, 10, None);
		Self::new_mesh_centered(gl, mesh)
	}
}

impl Render3dPrimitive for SimpleMesh3dGeom {
	fn vao(&self) -> u32 {
		self.vao.0.get()
	}

	fn draw(&self, gl: &GLHandle, efx: &Render3DEfx) {
		gl.vert_attr(2, VertexAttrVariant::UbyteNorm4.call(match efx {
			Render3DEfx::Color(color) => color.rgba(),
		})); // Color
		gl.draw_elements(TRIANGLES, self.num_vertices);
	}
}

impl Geom for SimpleMesh3dGeom {}
