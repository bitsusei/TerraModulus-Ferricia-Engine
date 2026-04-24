/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use std::collections::LinkedList;
use std::ops::{Deref, Range, RangeInclusive};
use std::rc::Rc;
use by_address::ByAddress;
use getset::Getters;
use nalgebra_glm::{DVec3, DVec4};
use ordermap::OrderSet;
use crate::phy::ode::{OdeBody, OdeContactManager, OdeHandle, OdePlaceabilityMarker, OdePlane, OdeWorld};
pub use crate::phy::ode::{OdeBox, OdeGeom, OdeGeomNonPlaceable, OdeGeomPlaceable, OdeMass, OdeNonPlaceableGeom, OdeNonPlaceableMarker, OdePlaceableGeom, OdePlaceableMarker, OdeSpace, OdeSphere};

mod ode;

static TICK_FREQUENCY: f64 = 20.0; // 20 Hz OR 1 / 0.05 s

pub struct PhyEnv {
	ode_handle: OdeHandle,
}

impl Default for PhyEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl PhyEnv {
	pub fn new() -> Self {
		Self {
			ode_handle: OdeHandle::new(),
		}
	}

	pub fn create_world(&self) -> PhyWorld {
		PhyWorld::new(&self.ode_handle)
	}
}

pub struct PhyCollisionManager {
	contact_manager: OdeContactManager,
}

impl Default for PhyCollisionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PhyCollisionManager {
	pub fn new() -> Self {
		Self {
			contact_manager: OdeContactManager::new(),
		}
	}

	pub fn process(&mut self, world: &PhyWorld) {
		self.contact_manager.process(&world.data)
	}

	pub fn omit_space(&mut self, space: &OdeSpace) {
		self.contact_manager.omit_space(space);
	}
}

#[derive(Getters)]
pub struct PhyWorld {
	data: OdeWorld,
	#[get = "pub"]
	space: TopLevelSpace,
	objs: OrderSet<ByAddress<Rc<Box<dyn PhyObj>>>>,
}

impl PhyWorld {
	fn new(handle: &OdeHandle) -> Self {
		Self {
			data: handle.create_world(),
			space: TopLevelSpace::new(),
			objs: OrderSet::default(),
		}
	}

	pub fn new_body(&self, mass: OdeMass) -> PhyBody {
		PhyBody::new_body(&self.data, mass)
	}

	pub fn tick(&self, collision_manager: &mut PhyCollisionManager) {
		self.data.step(1.0 / TICK_FREQUENCY);
		self.collide(collision_manager);
	}
}

pub struct TopLevelSpace {
	data: OdeSpace,
}

#[allow(clippy::new_without_default)]
impl TopLevelSpace {
	pub fn new() -> Self {
		Self {
			data: OdeSpace::new_hash(None, 4, 12)
		}
	}

	// In production, those `create_*` functions are not likely used, but within a lower-level OdeSpace.
	// However, these would be useful in testing or demonstration for rapid creation without a need of sub-Space.

	pub fn create_sphere(&self, radius: f64) -> OdeSphere {
		OdeSphere::new(Some(&self.data), radius)
	}

	pub fn create_box(&self, lengths: DVec3) -> OdeBox {
		OdeBox::new(Some(&self.data), lengths)
	}

	pub fn create_plane(&self, params: DVec4) -> OdePlane {
		OdePlane::new(Some(&self.data), params)
	}

	pub fn create_space(&self, range: impl Into<RangeInclusive<i32>>) -> OdeSpace {
		let range = range.into();
		OdeSpace::new_hash(Some(&self.data), *range.start(), *range.end())
	}

	pub fn collide(&self, collision_manager: &mut PhyCollisionManager) {
		self.data.collide(&mut collision_manager.contact_manager)
	}
}

impl Deref for PhyWorld {
	type Target = TopLevelSpace;
	fn deref(&self) -> &Self::Target {
		&self.space
	}
}

/// Acting as a simple wrapper handle over complex handling of OdeGeom its referencing.
pub struct PhyRawGeom<P: OdePlaceabilityMarker> {
	data: Rc<Box<dyn OdeGeom<Placeability=P>>>,
}

pub type PhyRawGeomPlaceable = PhyRawGeom<OdePlaceableMarker>;
pub type PhyRawGeomNonPlaceable = PhyRawGeom<OdeNonPlaceableMarker>;

impl<P: OdePlaceabilityMarker> PhyRawGeom<P> {
	pub fn new(geom: impl OdeGeom<Placeability=P> + 'static) -> Self {
		Self { data: Rc::new(Box::new(geom)) }
	}

	pub fn new_boxed(geom: Box<dyn OdeGeom<Placeability=P>>) -> Self {
		Self { data: Rc::new(geom) }
	}
}

impl<P: OdePlaceabilityMarker> Deref for PhyRawGeom<P> {
	type Target = dyn OdeGeom<Placeability = P>;
	fn deref(&self) -> &Self::Target {
		self.data.as_ref().as_ref()
	}
}

pub trait PhyObj {}

pub struct PhyBody {
	data: Option<OdeBody>,
	// Super trait (OdeGeom) has to be used instead of subtrait (OdePlaceableGeom).
	geoms: OrderSet<ByAddress<Rc<Box<OdeGeomPlaceable>>>>,
}

impl PhyBody {
	fn new_body(world: &OdeWorld, mass: OdeMass) -> Self {
		Self {
			data: Some(world.new_body(mass)),
			geoms: OrderSet::default(),
		}
	}

	/// # Safety
	///
	/// Caller must make sure this object contains a valid [`OdeBody`].
	pub unsafe fn set_position(&self, x: f64, y: f64, z: f64) {
		self.data.as_ref().unwrap().set_position(x, y, z);
	}

	/// # Safety
	///
	/// Caller must make sure this object contains a valid [`OdeBody`].
	pub unsafe fn get_position(&self) -> &[f64; 3] {
		self.data.as_ref().unwrap().get_position()
	}

	/// # Safety
	///
	/// Caller must make sure this object contains a valid [`OdeBody`].
	pub unsafe fn set_linear_vel(&self, x: f64, y: f64, z: f64) {
		self.data.as_ref().unwrap().set_linear_vel(x, y, z);
	}

	/// # Safety
	///
	/// Caller must make sure this object contains a valid [`OdeBody`].
	pub unsafe fn get_linear_vel(&self) -> &[f64; 3] {
		self.data.as_ref().unwrap().get_linear_vel()
	}

	/// # Safety
	///
	/// Caller must make sure this object contains a valid [`OdeBody`].
	pub unsafe fn add_force(&self, x: f64, y: f64, z: f64) {
		self.data.as_ref().unwrap().add_force(x, y, z);
	}

	pub fn add_geom(&mut self, geom: &PhyRawGeom<OdePlaceableMarker>) {
		if let Some(body) = &self.data {
			geom.data.set_body(body);
		}
		self.geoms.insert(ByAddress(geom.data.clone()));
	}

	pub fn remove_geom(&mut self, geom: &PhyRawGeom<OdePlaceableMarker>) {
		let address = ByAddress(geom.data.clone());
		self.geoms.remove(&address);
	}
}

impl PhyObj for PhyBody {}

pub struct PhyGeom {
	// Since this only handles a single OdeGeom, PhyRawGeom can be unused in this scope.
	data: Box<OdeGeomNonPlaceable>,
}

impl PhyGeom {
	pub fn new(geom: impl OdeNonPlaceableGeom + 'static) -> Self {
		Self { data: Box::new(geom) }
	}

	pub fn from(geom: PhyRawGeom<OdeNonPlaceableMarker>) -> Self {
		Self { data: Rc::into_inner(geom.data).expect("must not be in use") }
	}

	pub fn destruct(self) -> PhyRawGeom<OdeNonPlaceableMarker> {
		PhyRawGeom::new_boxed(self.data)
	}
}

impl Deref for PhyGeom {
	type Target = OdeGeomNonPlaceable;
	fn deref(&self) -> &Self::Target {
		self.data.as_ref()
	}
}

impl PhyObj for PhyGeom {}
