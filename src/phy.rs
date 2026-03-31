/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use std::collections::LinkedList;
use std::ops::Deref;
use nalgebra_glm::DVec3;
use crate::phy::ode::{OdeBody, OdeBox, OdeHandle, OdeNonPlaceableGeom, OdeNonPlaceableMarker, OdePlaceableGeom, OdePlaceableMarker, OdeSpace, OdeSphere, OdeWorld};

mod ode;

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

pub struct PhyWorld {
	data: OdeWorld,
	space: TopLevelSpace,
	objs: LinkedList<Box<dyn PhyObj>>,
}

impl PhyWorld {
	fn new(handle: &OdeHandle) -> Self {
		Self {
			data: handle.create_world(),
			space: TopLevelSpace::new(),
			objs: LinkedList::new(),
		}
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

	pub fn create_sphere(&self, radius: f64) -> OdeSphere {
		OdeSphere::new(Some(&self.data), radius)
	}

	pub fn create_box(&self, lengths: DVec3) -> OdeBox {
		OdeBox::new(Some(&self.data), lengths)
	}
}

impl Deref for PhyWorld {
	type Target = TopLevelSpace;
	fn deref(&self) -> &Self::Target {
		&self.space
	}
}

pub trait PhyObj {}

pub struct PhyBody {
	data: Option<OdeBody>,
	geoms: LinkedList<Box<dyn OdePlaceableGeom<Placeability = OdePlaceableMarker>>>,
}

impl PhyBody {
	pub fn new_body() -> Self {
		todo!()
	}
}

impl PhyObj for PhyBody {}

pub struct PhyGeom {
	data: Box<dyn OdeNonPlaceableGeom<Placeability = OdeNonPlaceableMarker>>,
}

impl PhyObj for PhyGeom {}
