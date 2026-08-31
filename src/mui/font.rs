/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use crate::mui::ogl::{GLHandle, NumType, VertexAttrVariant};
use crate::util::OpaqueId;
use array_macro::array;
use bymsdfgen_core::{generate_msdf, Bitmap, Bounds, Contour, EdgeSegment, Projection, SdfTransformation, Shape, Vector2};
use cosmic_text::{fontdb, Attrs, Buffer, CacheKey, CacheKeyFlags, Color, Command, FontSystem, PhysicalGlyph, Renderer, Shaping, SubpixelBin, SwashCache};
use crunch::Rotation;
use glow::{VertexArray, ARRAY_BUFFER, DYNAMIC_DRAW, ELEMENT_ARRAY_BUFFER, STATIC_DRAW, TRIANGLES};
use nalgebra_glm::{scaling, translation, Mat4, Vec3};
use num_traits::Num;
use rect_iter::RectRange;
use std::collections::HashMap;
use std::ops::Range;

/// Manager for Font Sources
pub(crate) struct FontManager {
	font_system: FontSystem,
}

/// Literally `32..=126`
const PRINTABLE_ASCII: [char; 95] = array![x => (x + 32) as u8 as _; 126 - 32 + 1];

// trait GlyphMap {
// 	fn add_glyph(&mut self,);
// }

struct GlyphItem {
	id: OpaqueId,
	out_size: (usize, usize),
	bitmap: Bitmap<f32, 3>,
}

struct CrunchGlyphMap {
	bitmap: Bitmap<f32, 3>,
	rect: crunch::Rect,
	packer: crunch::Packer<PartialCacheKey>,
	items: HashMap<PartialCacheKey, Bitmap<f32, 3>>,
	packed: HashMap<PartialCacheKey, PaddedRect>,
}

/// Rect with bottom-left as anchor (OpenGL Textures)
struct Rect {
	x: usize,
	y: usize,
	width: usize,
	height: usize,
}

impl Rect {
	#[inline]
	fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
		Self { x, y, width, height }
	}

	/// Automatically flipping coordinate space.
	fn from_crunch(rect: crunch::Rect) -> Self {
		Self::new(rect.x, rect.y, rect.w, rect.h)
	}

	/// `rect` must be in non-negative coordinates.
	/// Automatically flipping coordinate space.
	fn from_rect_pack(rect: rect_packer::Rect) -> Self {
		Self::new(rect.x as _, rect.y as _, rect.width as _, rect.height as _)
	}

	fn compute_out(&self, padding: usize) -> Self {
		Self::new(self.x - padding, self.y - padding, self.width + padding * 2, self.height + padding * 2)
	}

	fn compute_ctn(&self, padding: usize) -> Self {
		Self::new(self.x + padding, self.y + padding, self.width - padding * 2, self.height - padding * 2)
	}

	fn iter_outline(&self) -> RectOutlineIter<usize> {
		RectOutlineIter::from(
			RectRange::from_ranges(self.x..self.x + self.width, self.y..self.y + self.height).unwrap()
		)
	}
}

struct PaddedRect {
	ctn: Rect,
	out: Rect,
}

impl PaddedRect {
	fn with_ctn(ctn: Rect, padding: usize) -> Self {
		Self { out: ctn.compute_out(padding), ctn }
	}

	fn with_out(out: Rect, padding: usize) -> Self {
		Self { ctn: out.compute_ctn(padding), out }
	}
}

/// Implementation refers to [rect_iter::RectIter].
struct RectOutlineIter<T: Num + PartialOrd + Copy> {
	front: (T, T),
	end: bool,
	x_range: Range<T>,
	y_range: Range<T>,
}

impl<T: Num + PartialOrd + Copy> RectOutlineIter<T> {
	fn from(rect: RectRange<T>) -> Self {
		let x_range = rect.get_x();
		let y_range = rect.get_y();
		RectOutlineIter {
			front: (x_range.start, y_range.start),
			end: false,
			x_range: x_range.clone(),
			y_range: y_range.clone(),
		}
	}
}

impl<T: Num + PartialOrd + Copy> Iterator for RectOutlineIter<T> {
	type Item = (T, T);
	fn next(&mut self) -> Option<Self::Item> {
		if self.end {
			return None;
		}
		let before = self.front;
		// Clockwise
		if self.front.0 == self.x_range.start && self.front.1 < self.y_range.end - T::one() {
			self.front.1 = self.front.1 + T::one();
		} else if self.front.1 == self.y_range.end - T::one() && self.front.0 < self.x_range.end - T::one() {
			self.front.0 = self.front.0 + T::one();
		} else if self.front.0 == self.x_range.end - T::one() && self.front.1 > self.y_range.start {
			self.front.1 = self.front.1 - T::one();
		} else if self.front.1 == self.y_range.start && self.front.0 > self.x_range.start {
			self.front.0 = self.front.0 - T::one();
			if self.front.0 == self.x_range.start {
				self.end = true;
			}
		} else {
			self.end = true;
		}
		Some(before)
	}
}

fn place_glyph(atlas: &mut Bitmap<f32, 3>, glyph: &Bitmap<f32, 3>, rect: &PaddedRect) {
	for (y, row) in glyph.data().chunks(glyph.width * 3).enumerate() {
		let atlas_y = rect.out.y + y;
		let atlas_x = rect.out.x;
		let start = (atlas_y * atlas.width + atlas_x) * 3;
		atlas.data_mut()[start..start + row.len()].copy_from_slice(row);
	}
}

impl CrunchGlyphMap {
	fn new(width: usize, height: usize) -> Self {
		Self {
			bitmap: Bitmap::new(width, height),
			rect: crunch::Rect::of_size(width, height),
			packer: crunch::Packer::new(),
			items: HashMap::new(),
			packed: HashMap::new(),
		}
	}

	fn push(&mut self, key: PartialCacheKey, item: Bitmap<f32, 3>) {
		self.packer.push(crunch::Item::new(key, item.width, item.height, Rotation::None));
		self.items.insert(key, item);
	}

	/// DEMO: This should be called only once to avoid troublesome handling of texture data.
	fn pack(&mut self) {
		let mut packed = HashMap::new();
		for i in self.packer.pack(self.rect).unwrap_or_else(|_| panic!("should have been succeeded")) {
			packed.insert(i.data, PaddedRect::with_out(Rect::from_crunch(i.rect), 1));
		}
		for (k, v) in &packed {
			place_glyph(&mut self.bitmap, self.items.get(k).unwrap(), v);
		}
		self.packed.extend(packed);
	}

	fn get_glyph(&self, key: PartialCacheKey) -> Option<&PaddedRect> {
		self.packed.get(&key)
	}
}

struct RectPackGlyphMap {
	bitmap: Bitmap<f32, 3>,
	packer: rect_packer::DensePacker,
	items: HashMap<PartialCacheKey, (Bitmap<f32, 3>, PaddedRect)>,
}

impl RectPackGlyphMap {
	fn new(width: usize, height: usize) -> Self {
		Self {
			bitmap: Bitmap::new(width, height),
			packer: rect_packer::DensePacker::new(width as _, height as _),
			items: HashMap::new(),
		}
	}

	fn push(&mut self, key: PartialCacheKey, item: Bitmap<f32, 3>) -> &PaddedRect {
		let rect = self.packer.pack(item.width as _, item.height as _, false).unwrap();
		let rect = PaddedRect::with_out(Rect::from_rect_pack(rect), 1);
		place_glyph(&mut self.bitmap, &item, &rect);
		self.items.insert(key, (item, rect));
		&self.items.get(&key).unwrap().1
	}

	fn get_glyph(&self, key: PartialCacheKey) -> Option<&PaddedRect> {
		self.items.get(&key).map(|(_, rect)| rect)
	}
}

pub(crate) struct GlyphManager {
	/// Only commands are used, without its rasterization feature.
	swash_cache: SwashCache,
	pre_glyphs: CrunchGlyphMap,
	tmp_glyphs: RectPackGlyphMap,
}

/// [`CacheKey`] but without font size and pixel binning for MSDF glyph caching.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct PartialCacheKey {
	/// Font ID
	pub font_id: fontdb::ID,
	/// Glyph ID
	pub glyph_id: u16,
	/// Font weight
	pub font_weight: fontdb::Weight,
}

impl PartialCacheKey {
	fn to_internal(&self) -> CacheKey {
		CacheKey {
			font_id: self.font_id,
			glyph_id: self.glyph_id,
			font_size_bits: 64.0.to_bits(),
			// Zero binning for pixel perfect
			x_bin: SubpixelBin::Zero,
			y_bin: SubpixelBin::Zero,
			font_weight: self.font_weight,
			// Whether MSDF with disabled hinting or specific rasterization with hinting is better is still unknown,
			// but in MSDF, standard sizing without hinting would still be used.
			flags: CacheKeyFlags::DISABLE_HINTING,
		}
	}

	fn from_internal(key: CacheKey) -> Self {
		Self {
			font_id: key.font_id,
			glyph_id: key.glyph_id,
			font_weight: key.font_weight,
		}
	}
}

pub(super) struct TextRenderer {
	gl: &'static GLHandle,
	rect_geom: SimpleRectGeom,
	text_mesh: GlyphMesh,
}

impl TextRenderer {
	pub(super) fn new(gl: &'static GLHandle) -> Self {
		Self {
			rect_geom: SimpleRectGeom::new(&gl),
			text_mesh: GlyphMesh::new(&gl),
			gl,
		}
	}

	fn render_rect(&self, x: i32, y: i32, w: u32, h: u32, color: Color) {
		
	}

	fn render_glyph(&mut self, physical_glyph: PhysicalGlyph, color: Color) {
		// self.glyph_manager.render_glyph(self.font_system, physical_glyph);
	}
}

fn compute_model_mat(pos: Vec3, scale: Vec3) -> Mat4 {
	(translation(&pos) * scaling(&scale)).cast()
}

impl GlyphManager {
	pub(crate) fn new() -> Self {
		Self {
			swash_cache: SwashCache::new(),
			pre_glyphs: CrunchGlyphMap::new(8192, 8192),
			tmp_glyphs: RectPackGlyphMap::new(4096, 4096),
		}
	}

	pub(crate) fn render_text(&mut self,
	                          font_system: &mut FontSystem,
	                          renderer: &mut GlyphRenderer,
	                          ctx: &mut TextRenderingContext,
	) {
		ctx.buffer.render(font_system, renderer, ctx.color);

	}

	fn render_glyph(&mut self, font_system: &mut FontSystem, glyph: PhysicalGlyph) {

	}

	fn gen_glyph(&mut self, font_system: &mut FontSystem, cache_key: PartialCacheKey) -> Bitmap<f32, 3> {
		let mut shape = Shape::new();
		let mut contour: Option<Contour> = None;
		let mut prev_pt: Option<Vector2> = None;
		for cmd in self.swash_cache.get_outline_commands(font_system, cache_key.to_internal()).unwrap() {
			match cmd {
				Command::MoveTo(pt) => {
					contour = Some(Contour::new());
					prev_pt = Some(Vector2::new(pt.x as _, pt.y as _));
				},
				Command::LineTo(pt) => contour.as_mut().unwrap().add_edge(EdgeSegment::line(
					prev_pt.unwrap(),
					Vector2::new(pt.x as _, pt.y as _),
				)),
				Command::CurveTo(cp1, cp2, pt) => contour.as_mut().unwrap().add_edge(EdgeSegment::cubic(
					prev_pt.unwrap(),
					Vector2::new(cp1.x as _, cp1.y as _),
					Vector2::new(cp2.x as _, cp2.y as _),
					Vector2::new(pt.x as _, pt.y as _),
				)),
				Command::QuadTo(cp, pt) => contour.as_mut().unwrap().add_edge(EdgeSegment::quadratic(
					prev_pt.unwrap(),
					Vector2::new(cp.x as _, cp.y as _),
					Vector2::new(pt.x as _, pt.y as _),
				)),
				Command::Close => {
					prev_pt = None;
					shape.add_contour(contour.take().unwrap());
				}
			}
		}
		// let scale = GLYPH_RESOLUTION as f64 / self.font_system
		// 	.get_font(physical_glyph.cache_key.font_id, physical_glyph.cache_key.font_weight).unwrap()
		// 	.metrics().units_per_em as f64;
		// Unfortunately this crate does not support using a section reference.
		let mut bitmap = Bitmap::new(GLYPH_RESOLUTION + 2.0, GLYPH_RESOLUTION + 2.0);
		let projection = SdfTransformation::new(
			compute_projection(shape.get_bounds(0.0), GLYPH_RESOLUTION as _, 1.0),
			Default::default(),
		);
		// TODO yet default but later may offer options for this
		let cfg = Default::default();
		generate_msdf(&mut bitmap, &shape, &projection, &cfg);
		bitmap
	}
}

pub(crate) struct SimpleRectGeom {
	gl: &'static GLHandle,
	vao: VertexArray,
	vbo: glow::Buffer,
	ebo: glow::Buffer,
}

impl SimpleRectGeom {
	const INDICES: [u32; 6] = [
		0, 1, 2, // first triangle
		0, 2, 3  // second triangle
	];

	const VERTICES: [f32; 8] = [
		0.0, 1.0, // top-left
		0.0, 0.0, // bottom-left
		1.0, 0.0, // bottom-right
		1.0, 1.0, // top-right
	];

	const NUM_ELEMENTS: u32 = 6;

	/// `[x0, y0, x1, y1]`; (0, 0) as bottom-left
	fn new(gl: &'static GLHandle) -> Self {
		let vao = gl.with_new_vert_arr();
		let [vbo, ebo] = gl.gen_buf_objs();
		gl.buf_obj_with_data(ARRAY_BUFFER, vbo, &Self::VERTICES, STATIC_DRAW);
		gl.buf_obj_with_data(ELEMENT_ARRAY_BUFFER, ebo, &Self::INDICES, STATIC_DRAW);
		gl.vert_attr_arr(0, 2, NumType::Float, 2, 0); // Position
		Self { gl, vao, vbo, ebo }
	}

	pub(super) fn draw(&self, color: Color) {
		self.gl.vert_attr(1, VertexAttrVariant::UbyteNorm4.call(color.as_rgba_tuple())); // Color
		self.gl.draw_elements(TRIANGLES, Self::NUM_ELEMENTS);
	}
	
	pub(super) fn apply_vao(&self) {
		self.gl.use_vao(self.vao);
	}
}

pub(crate) struct GlyphMesh {
	gl: &'static GLHandle,
	vao: VertexArray,
	vbo: glow::Buffer,
	ebo: glow::Buffer,
}

impl GlyphMesh {
	const INDICES: [u32; 6] = [
		0, 1, 2, // first triangle
		0, 2, 3  // second triangle
	];

	const NUM_ELEMENTS: u32 = 6;

	fn new(gl: &'static GLHandle) -> Self {
		let vao = gl.with_new_vert_arr();
		let [vbo, ebo] = gl.gen_buf_objs();
		let vertices: [f32; 16] = [
			// positions
			0.0, 1.0, // top-left
			0.0, 0.0, // bottom-left
			1.0, 0.0, // bottom-right
			1.0, 1.0, // top-right
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
		Self { gl, vao, vbo, ebo } // Note: Binding to the VAO remains
	}

	/// Update for tex coords; `[x0, y0, x1, y1]`; (0, 0) as bottom-left
	fn update_vertices(&self, points: [u32; 4]) {
		// Buffer Orphaning (for OpenGL < 3.0)
		let vertices: [f32; 16] = [
			// positions
			0.0, 1.0, // top-left
			0.0, 0.0, // bottom-left
			1.0, 0.0, // bottom-right
			1.0, 1.0, // top-right
			// tex coords
			points[0] as _, points[3] as _, // top-left
			points[0] as _, points[1] as _, // bottom-left
			points[2] as _, points[1] as _, // bottom-right
			points[2] as _, points[3] as _, // top-right
		];
		self.gl.orphan_and_update_buf_obj(ARRAY_BUFFER, self.vbo, &vertices);
	}

	pub(super) fn draw(&self) {
		self.gl.draw_elements(TRIANGLES, Self::NUM_ELEMENTS);
	}

	pub(super) fn apply_vao(&self) {
		self.gl.use_vao(self.vao);
	}
}

impl FontManager {
	pub(crate) fn new() -> Self {
		// TODO Only used in DEMO
		Self {
			font_system: FontSystem::new(),
		}
	}

	pub(super) fn render_text(&mut self, mut renderer: GlyphRenderer, ctx: &mut TextRenderingContext) {
		ctx.buffer.render(&mut self.font_system, &mut renderer, ctx.color)
	}
}

pub(super) struct GlyphRenderer<'a> {
	glyph_manager: &'a mut GlyphManager,
	font_system: &'a mut FontSystem,
}

const GLYPH_RESOLUTION: usize = 16;

impl Renderer for GlyphRenderer<'_> {
	fn rectangle(&mut self, x: i32, y: i32, w: u32, h: u32, color: Color) {
		todo!()
	}

	fn glyph(&mut self, physical_glyph: PhysicalGlyph, color: Color) {
		self.glyph_manager.render_glyph(self.font_system, physical_glyph);
	}
}

/// Projecting `src.b` and `src.l` to `padding`, `src.t` and `src.r` to `bitmap_size + padding`.
#[inline]
fn compute_projection(src: Bounds, bitmap_size: f64, padding: f64) -> Projection {
	let sx = bitmap_size / (src.r - src.l);
	let sy = bitmap_size / (src.t - src.b);
	Projection::new(Vector2::new(sx, sy), Vector2::new(padding / sx - src.l, padding / sy - src.b))
}

pub(crate) struct TextRenderingContext<'a> {
	pub(crate) attrs: Attrs<'a>,
	pub(crate) buffer: Buffer,
	pub(crate) color: Color,
}

impl TextRenderingContext {
	#[inline]
	pub(crate) fn set_text(&mut self, text: &str) {
		self.buffer.set_text(text, &self.attrs, Shaping::Basic, None)
	}
}


// /// Processor for intermediate states of fonts, used during loading of fonts
// pub(crate) struct FontProcessor {}

// /// Manager for generated glyphs and their metrics
// pub(crate) struct GlyphManager {
//
// }
