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
use glow::{Texture, VertexArray, ARRAY_BUFFER, DYNAMIC_DRAW, ELEMENT_ARRAY_BUFFER, STATIC_DRAW, TRIANGLES};
use nalgebra_glm::{scaling, translation, Mat4, UVec2, Vec3, Vec4};
use num_traits::Num;
use rect_iter::RectRange;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use crate::mui::rendering::{CanvasHandle, GeoProgram, TxtProgram};

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
	// Though actually in fact, packing efficiency would be the same as using rect_packer,
	// since those items are all squares, and skyline would already be sufficient.
	atlas: MsdfTexture,
	rect: crunch::Rect,
	packer: crunch::Packer<PartialCacheKey>,
	items: HashMap<PartialCacheKey, (Bitmap<f32, 3>, (f64, f64))>,
	packed: HashMap<PartialCacheKey, PaddedRect>,
}

/// Rect with bottom-left as anchor (OpenGL Textures)
#[derive(Clone, Copy, Debug)]
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
	fn new(gl: Arc<GLHandle>, width: usize, height: usize) -> Self {
		Self {
			atlas: MsdfTexture::new(gl, Bitmap::new(width, height)),
			rect: crunch::Rect::of_size(width, height),
			packer: crunch::Packer::new(),
			items: HashMap::new(),
			packed: HashMap::new(),
		}
	}

	fn push(&mut self, key: PartialCacheKey, item: Bitmap<f32, 3>, size: (f64, f64)) {
		self.packer.push(crunch::Item::new(key, item.width, item.height, Rotation::None));
		self.items.insert(key, (item, size));
	}

	/// DEMO: This should be called only once to avoid troublesome handling of texture data.
	fn pack(&mut self) {
		let mut packed = HashMap::new();
		for i in self.packer.pack(self.rect).unwrap_or_else(|_| panic!("should have been succeeded")) {
			packed.insert(i.data, PaddedRect::with_out(Rect::from_crunch(i.rect), 1));
		}
		for (k, v) in &packed {
			place_glyph(&mut self.atlas.bitmap, &self.items.get(k).unwrap().0, v);
		}
		self.packed.extend(packed);
		self.atlas.update_full();
	}

	fn get_glyph(&self, key: PartialCacheKey) -> Option<GlyphRect> {
		self.packed.get(&key).map(|v| GlyphRect {
			ctn_rect: v.ctn,
			size: self.items.get(&key).unwrap().1
		})
	}
}

struct RectPackGlyphMap {
	atlas: MsdfTexture,
	packer: rect_packer::DensePacker,
	items: HashMap<PartialCacheKey, (Bitmap<f32, 3>, PaddedRect, (f64, f64))>,
}

impl RectPackGlyphMap {
	fn new(gl: Arc<GLHandle>, width: usize, height: usize) -> Self {
		Self {
			atlas: MsdfTexture::new(gl, Bitmap::new(width, height)),
			packer: rect_packer::DensePacker::new(width as _, height as _),
			items: HashMap::new(),
		}
	}

	fn push(&mut self, key: PartialCacheKey, item: Bitmap<f32, 3>, size: (f64, f64)) -> GlyphRect {
		let rect = self.packer.pack(item.width as _, item.height as _, false).unwrap();
		let rect = PaddedRect::with_out(Rect::from_rect_pack(rect), 1);
		place_glyph(&mut self.atlas.bitmap, &item, &rect);
		self.atlas.update_part(UVec2::new(rect.out.x as _, rect.out.y as _), &item);
		self.items.insert(key, (item, rect, size));
		GlyphRect {
			ctn_rect: self.items.get(&key).unwrap().1.ctn,
			size,
		}
	}

	fn get_glyph(&self, key: PartialCacheKey) -> Option<GlyphRect> {
		self.items.get(&key).map(|(_, rect, size)| GlyphRect {
			ctn_rect: rect.ctn,
			size: *size,
		})
	}
}

struct GlyphRect {
	/// [Rect] for MSDF of the glyph to be rendered
	ctn_rect: Rect,
	/// Base size of the glyph
	size: (f64, f64),
}

struct MsdfTexture {
	gl: Arc<GLHandle>,
	bitmap: Bitmap<f32, 3>,
	texture: Texture,
}

impl MsdfTexture {
	fn new(gl: Arc<GLHandle>, bitmap: Bitmap<f32, 3>) -> Self {
		Self {
			texture: gl.new_msdf_texture(&bitmap),
			bitmap,
			gl,
		}
	}

	fn update_full(&self) {
		self.gl.update_msdf_texture(self.texture, UVec2::zeros(), &self.bitmap)
	}

	fn update_part(&self, offset: UVec2, bitmap: &Bitmap<f32, 3>) {
		self.gl.update_msdf_texture(self.texture, offset, bitmap)
	}
}

pub(crate) struct GlyphManager {
	/// Only commands are used, without its rasterization feature.
	swash_cache: SwashCache,
	pre_glyphs: CrunchGlyphMap,
	tmp_glyphs: RectPackGlyphMap,
	font: fontdb::ID, // only used in DEMO
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
			font_size_bits: 64.0_f32.to_bits(),
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
	gl: Arc<GLHandle>,
	geo_program: &'static GeoProgram,
	txt_program: &'static TxtProgram,
	rect_geom: SimpleRectGeom,
	text_mesh: GlyphMesh,
}

impl TextRenderer {
	pub(super) fn new(
		gl: Arc<GLHandle>,
		geo_program: &'static GeoProgram,
		txt_program: &'static TxtProgram,
	) -> Self {
		Self {
			geo_program,
			txt_program,
			rect_geom: SimpleRectGeom::new(gl.clone()),
			text_mesh: GlyphMesh::new(gl.clone()),
			gl,
		}
	}

	fn render_rect(&self, canvas_handle: &CanvasHandle, x: i32, y: i32, w: u32, h: u32, color: Color) {
		canvas_handle.draw_text_rect(
			&self.rect_geom,
			self.geo_program,
			compute_model_mat(Vec3::new(x as _, y as _, 0.0), Vec3::new(w as _, h as _, 1.0)),
			color,
		);
	}

	fn render_glyph(&mut self, canvas_handle: &CanvasHandle, pos: (i32, i32), size: (f32, f32), texture: Texture, rect: Rect, color: Color) {
		// self.glyph_manager.render_glyph(self.font_system, physical_glyph);
		self.text_mesh.update_vertices(
			[rect.x as _, rect.y as _, (rect.x + rect.width) as _, (rect.y + rect.height) as _]
		);
		canvas_handle.draw_text_glyph(
			&self.text_mesh,
			self.txt_program,
			compute_model_mat(Vec3::new(pos.0 as _, pos.1 as _, 0.0), Vec3::new(size.0, size.1, 1.0)),
			Vec4::from(color.as_rgba().map(|v| v as f32 / 255.0)),
			texture,
		);
	}
}

fn compute_model_mat(pos: Vec3, scale: Vec3) -> Mat4 {
	(translation(&pos) * scaling(&scale)).cast()
}

impl GlyphManager {
	pub(crate) fn new(font_manager: &mut FontManager, gl: &Arc<GLHandle>) -> Self {
		// TODO only used in DEMO
		let font_system = &mut font_manager.font_system;
		let mut fonts: Vec<(_, u8)> = font_system.db().faces().filter_map(|f| {
			if f.families.iter().any(|family| family.0 == "Noto Sans") {
				Some((f.id, 5))
			} else if f.families.iter().any(|family| family.0 == "Microsoft Sans Serif") {
				Some((f.id, 4))
			} else if f.families.iter().any(|family| family.0 == "Liberation Sans") {
				Some((f.id, 3))
			} else if f.families.iter().any(|family| family.0 == "Source Sans Pro") {
				Some((f.id, 2))
			} else if f.families.iter().any(|family| family.0 == "DejaVu Sans") {
				Some((f.id, 1))
			} else if f.families.iter().any(|family| family.0 == "Arial") {
				Some((f.id, 0))
			} else {
				None
			}
		}).collect();
		if fonts.is_empty() { panic!("no matched fonts found."); }
		fonts.sort_by(|a, b| a.1.cmp(&b.1));
		fonts.reverse();
		let font = fonts.first().unwrap().0;
		if font_system.get_font(font, fontdb::Weight::NORMAL).is_none() {
			panic!("font mismatched for {:?}.", font_system.db().face(font).unwrap());
		}
		let mut new = Self {
			swash_cache: SwashCache::new(),
			pre_glyphs: CrunchGlyphMap::new(gl.clone(), 8192, 8192),
			tmp_glyphs: RectPackGlyphMap::new(gl.clone(), 4096, 4096),
			font,
		};
		for k in PRINTABLE_ASCII.map(|c| PartialCacheKey {
			font_id: font,
			glyph_id: c as u16,
			font_weight: Default::default(),
		}) {
			let glyph = new.gen_glyph(font_system, k);
			new.pre_glyphs.push(k, glyph.1, glyph.0);
		}
		new.pre_glyphs.pack();
		new
	}

	pub(crate) fn render_text(&mut self,
	                          canvas_handle: &CanvasHandle,
	                          text_renderer: &mut TextRenderer,
	                          font_manager: &mut FontManager,
	                          ctx: &mut TextRenderingContext,
	) {
		// SAFE as mutable borrows here are used strictly separately.
		let font_system = &raw mut font_manager.font_system;
		let mut renderer = GlyphRenderer {
			glyph_manager: self,
			font_system: unsafe { &mut *font_system },
			text_renderer,
			canvas_handle,
		};
		ctx.buffer.render(unsafe { &mut *font_system }, &mut renderer, ctx.color);
	}

	fn render_glyph(&mut self,
	                canvas_handle: &CanvasHandle,
	                text_renderer: &mut TextRenderer,
	                font_system: &mut FontSystem,
	                glyph: PhysicalGlyph,
	                color: Color,
	) {
		let (tex, rect) = self.get_glyph(font_system, PartialCacheKey::from_internal(glyph.cache_key));
		// Not sure how font size should be computed here.
		let scale = f32::from_bits(glyph.cache_key.font_size_bits) as f64 / rect.size.0.max(rect.size.1);
		text_renderer.render_glyph(
			canvas_handle,
			(glyph.x, glyph.y),
			((rect.size.0 * scale) as _, (rect.size.1 * scale) as _),
			tex,
			rect.ctn_rect,
			color,
		);
	}

	fn get_glyph(&mut self, font_system: &mut FontSystem, cache_key: PartialCacheKey) -> (Texture, GlyphRect) {
		self.pre_glyphs.get_glyph(cache_key).map(|r| (self.pre_glyphs.atlas.texture, r))
			.unwrap_or_else(|| (self.tmp_glyphs.atlas.texture, self.tmp_glyphs.get_glyph(cache_key).unwrap_or_else(|| {
				let glyph = self.gen_glyph(font_system, cache_key);
				self.tmp_glyphs.push(cache_key, glyph.1, glyph.0)
			})))
	}

	fn gen_glyph(&mut self, font_system: &mut FontSystem, cache_key: PartialCacheKey) -> ((f64, f64), Bitmap<f32, 3>) {
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
		let mut bitmap = Bitmap::new(GLYPH_RESOLUTION + 2, GLYPH_RESOLUTION + 2);
		let bounds = shape.get_bounds(0.0);
		let projection = SdfTransformation::new(
			compute_projection(bounds, GLYPH_RESOLUTION as _, 1.0),
			Default::default(),
		);
		// TODO yet default but later may offer options for this
		let cfg = Default::default();
		generate_msdf(&mut bitmap, &shape, &projection, &cfg);
		((bounds.r - bounds.l, bounds.t - bounds.b), bitmap)
	}
}

pub(crate) struct SimpleRectGeom {
	gl: Arc<GLHandle>,
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
	fn new(gl: Arc<GLHandle>) -> Self {
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
	gl: Arc<GLHandle>,
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

	fn new(gl: Arc<GLHandle>) -> Self {
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
	text_renderer: &'a mut TextRenderer,
	canvas_handle: &'a CanvasHandle,
}

const GLYPH_RESOLUTION: usize = 16;

impl Renderer for GlyphRenderer<'_> {
	fn rectangle(&mut self, x: i32, y: i32, w: u32, h: u32, color: Color) {
		self.text_renderer.render_rect(self.canvas_handle, x, y, w, h, color);
	}

	fn glyph(&mut self, physical_glyph: PhysicalGlyph, color: Color) {
		self.glyph_manager.render_glyph(
			self.canvas_handle,
			self.text_renderer,
			self.font_system,
			physical_glyph,
			color,
		);
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

impl TextRenderingContext<'_> {
	#[inline]
	pub(crate) fn set_text(&mut self, text: &str) {
		self.buffer.set_text(text, &self.attrs, Shaping::Basic, None)
	}
}

// /// Processor for intermediate states of fonts, used during loading of fonts
// pub(crate) struct FontProcessor {}
