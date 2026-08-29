/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use crate::util::OpaqueId;
use bymsdfgen_core::{generate_msdf, Bitmap, Bounds, Contour, EdgeSegment, Projection, SdfTransformation, Shape, Vector2};
use cosmic_text::{Attrs, Buffer, Color, Command, FontSystem, PhysicalGlyph, Renderer, Shaping, SwashCache};
use crunch::Rotation;
use num_traits::{clamp, Num};
use rect_iter::RectRange;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::AtomicUsize;

/// Manager for Font Sources
pub(crate) struct FontManager {
	font_system: FontSystem,
}

// trait GlyphMap {
// 	fn add_glyph(&mut self,);
// }

struct GlyphItem {
	id: OpaqueId,
	out_size: (usize, usize),
	bitmap: Bitmap<f64, 3>,
}

struct CrunchGlyphMap {
	bitmap: Bitmap<f64, 3>,
	rect: crunch::Rect,
	packer: crunch::Packer<OpaqueId>,
	id_counter: AtomicUsize,
	items: HashMap<OpaqueId, GlyphItem>,
	packed: HashMap<OpaqueId, PaddedRect>,
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

	fn from_crunch(rect: crunch::Rect) -> Self {
		Self::new(rect.x, rect.bottom(), rect.w, rect.h)
	}

	/// `rect` must be in non-negative coordinates.
	fn from_rect_pack(rect: rect_packer::Rect) -> Self {
		Self::new(rect.x as _, rect.bottom() as _, rect.width as _, rect.height as _)
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

fn place_glyph(atlas: &mut Bitmap<f64, 3>, glyph: &Bitmap<f64, 3>, rect: &PaddedRect) {
	for (y, row) in glyph.data().chunks(glyph.width * 3).enumerate() {
		let atlas_y = rect.ctn.y + y;
		let atlas_x = rect.ctn.x;
		let start = (atlas_y * atlas.width + atlas_x) * 3;
		atlas.data_mut()[start..start + row.len()].copy_from_slice(row);
	}
	for (x, y) in rect.out.iter_outline() {
		atlas.pixel_mut(x, y).copy_from_slice(glyph.pixel(
			clamp(x, rect.ctn.x, rect.ctn.x + rect.ctn.width - 1) - rect.ctn.x,
			clamp(y, rect.ctn.y, rect.ctn.y + rect.ctn.height - 1) - rect.ctn.y,
		));
	}
}

impl CrunchGlyphMap {
	fn new(width: usize, height: usize) -> Self {
		Self {
			bitmap: Bitmap::new(width, height),
			rect: crunch::Rect::of_size(width, height),
			packer: crunch::Packer::new(),
			id_counter: AtomicUsize::new(0),
			items: HashMap::new(),
			packed: HashMap::new(),
		}
	}

	fn push(&mut self, item: Bitmap<f64, 3>) {
		let item = GlyphItem {
			id: OpaqueId::new(&mut self.id_counter),
			out_size: (item.width + 2, item.height + 2),
			bitmap: item,
		};
		self.packer.push(crunch::Item::new(item.id, item.out_size.0, item.out_size.1, Rotation::None));
		self.items.insert(item.id, item);
	}

	/// DEMO: This should be called only once to avoid troublesome handling of texture data.
	fn pack(&mut self) {
		for i in self.packer.pack(self.rect).unwrap_or_else(|_| panic!("should have been succeeded")) {
			self.packed.insert(i.data, PaddedRect::with_out(Rect::from_crunch(i.rect), 1));
		}
		for (k, v) in &self.packed {
			let i = self.items.get(k).unwrap();
			place_glyph(&mut self.bitmap, &i.bitmap, v);
		}
	}
}

struct RectPackGlyphMap {}

pub(crate) struct GlyphManager {
	/// Only commands are used, without its rasterization feature.
	swash_cache: SwashCache,
	pre_glyphs: CrunchGlyphMap,
	tmp_glyphs: RectPackGlyphMap,
}

impl GlyphManager {
	pub(crate) fn new() -> Self {
		Self {
			swash_cache: SwashCache::new(),
			pre_glyphs: CrunchGlyphMap::new(8192, 8192),
			tmp_glyphs: RectPackGlyphMap {},
		}
	}

	pub(crate) fn render_text(&mut self,
	                          font_system: &mut FontSystem,
	                          renderer: &mut TextRenderer,
	                          ctx: &mut TextRenderingContext,
	) {
		ctx.buffer.render(font_system, renderer, ctx.color);

	}
}

impl FontManager {
	pub(crate) fn new() -> Self {
		// TODO Only used in DEMO
		Self {
			font_system: FontSystem::new(),
		}
	}

	pub(super) fn render_text(&mut self, mut renderer: TextRenderer, ctx: &mut TextRenderingContext) {
		ctx.buffer.render(&mut self.font_system, &mut renderer, ctx.color)
	}
}

pub(super) struct TextRenderer<'a> {
	swash_cache: &'a mut SwashCache,
	font_system: &'a mut FontSystem,
}

const GLYPH_RESOLUTION: usize = 16;

impl Renderer for TextRenderer<'_> {
	fn rectangle(&mut self, x: i32, y: i32, w: u32, h: u32, color: Color) {
		todo!()
	}

	fn glyph(&mut self, physical_glyph: PhysicalGlyph, color: Color) {
		let mut shape = Shape::new();
		let mut contour: Option<Contour> = None;
		let mut prev_pt: Option<Vector2> = None;
		for cmd in self.swash_cache.get_outline_commands(self.font_system, physical_glyph.cache_key).unwrap() {
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
		let mut bitmap = Bitmap::new(GLYPH_RESOLUTION, GLYPH_RESOLUTION);
		let projection = SdfTransformation::new(
			compute_projection(shape.get_bounds(0.0), GLYPH_RESOLUTION as _),
			Default::default(),
		);
		// TODO yet default but later may offer options for this
		let cfg = Default::default();
		generate_msdf(&mut bitmap, &shape, &projection, &cfg);
	}
}

#[inline]
fn compute_projection(src: Bounds, bitmap_size: f64) -> Projection {
	let sx = (src.r - src.l) / bitmap_size;
	let sy = (src.t - src.b) / bitmap_size;
	Projection::new(Vector2::new(sx, sy), Vector2::new(0.0 - src.l, 0.0 - src.b))
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
