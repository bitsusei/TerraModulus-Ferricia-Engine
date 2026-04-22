/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use crate::util::{concat_arrays, create_file_c, str_from_c};
use getset::{Getters, MutGetters};
use nalgebra_glm::{DMat3, DMat4, DMat4x3, DQuat, DVec3, DVec4};
use ode_sys::{dBodyAddForce, dBodyAddForceAtPos, dBodyAddForceAtRelPos, dBodyAddRelForce, dBodyAddRelForceAtPos, dBodyAddRelForceAtRelPos, dBodyAddTorque, dBodyCreate, dBodyDestroy, dBodyDisable, dBodyEnable, dBodyGetAngularVel, dBodyGetForce, dBodyGetGravityMode, dBodyGetLinearVel, dBodyGetPosition, dBodyGetQuaternion, dBodyGetRotation, dBodyID, dBodyIsEnabled, dBodyIsKinematic, dBodySetAngularVel, dBodySetDynamic, dBodySetGravityMode, dBodySetKinematic, dBodySetLinearVel, dBodySetMass, dBodySetMovedCallback, dBodySetPosition, dBodySetQuaternion, dBodySetRotation, dCloseODE, dCollide, dContact, dContactGeom, dCreateBox, dCreateCapsule, dCreateCylinder, dCreatePlane, dCreateRay, dCreateSphere, dGeomBoxSetLengths, dGeomCapsuleSetParams, dGeomClearOffset, dGeomCylinderSetParams, dGeomDestroy, dGeomDisable, dGeomEnable, dGeomGetAABB, dGeomGetBody, dGeomGetOffsetPosition, dGeomGetOffsetQuaternion, dGeomGetOffsetRotation, dGeomGetPosition, dGeomGetQuaternion, dGeomGetRotation, dGeomID, dGeomIsEnabled, dGeomIsSpace, dGeomPlaneSetParams, dGeomRaySet, dGeomRaySetBackfaceCull, dGeomRaySetClosestHit, dGeomRaySetFirstContact, dGeomRaySetLength, dGeomRaySetParams, dGeomSetBody, dGeomSetCategoryBits, dGeomSetCollideBits, dGeomSetOffsetPosition, dGeomSetOffsetQuaternion, dGeomSetOffsetRotation, dGeomSetOffsetWorldPosition, dGeomSetOffsetWorldQuaternion, dGeomSetOffsetWorldRotation, dGeomSetPosition, dGeomSetQuaternion, dGeomSetRotation, dGeomSphereSetRadius, dGetConfiguration, dHashSpaceCreate, dHashSpaceSetLevels, dInitODE, dJointCreateContact, dJointDestroy, dJointGroupCreate, dJointGroupDestroy, dJointGroupID, dJointID, dMass, dMassAdd, dMassAdjust, dMassRotate, dMassSetBox, dMassSetBoxTotal, dMassSetCapsule, dMassSetCapsuleTotal, dMassSetCylinder, dMassSetCylinderTotal, dMassSetParameters, dMassSetSphere, dMassSetSphereTotal, dMassSetTrimesh, dMassSetTrimeshTotal, dMassSetZero, dMassTranslate, dNormalize3, dQuadTreeSpaceCreate, dSimpleSpaceCreate, dSpaceAdd, dSpaceCollide, dSpaceCollide2, dSpaceDestroy, dSpaceGetNumGeoms, dSpaceID, dSpaceQuery, dSpaceRemove, dSurfaceParameters, dSweepAndPruneSpaceCreate, dWorldCreate, dWorldDestroy, dWorldExportDIF, dWorldGetAutoDisableFlag, dWorldGetCFM, dWorldGetERP, dWorldGetGravity, dWorldID, dWorldImpulseToForce, dWorldSetAutoDisableFlag, dWorldSetCFM, dWorldSetERP, dWorldSetGravity, dWorldStep};
use std::ffi::{c_void, CString};
use std::marker::PhantomData;
use std::mem::{transmute, MaybeUninit};
use std::ptr::{null, null_mut};
use by_address::ByAddress;
use futures::StreamExt;
use ordermap::OrderSet;
use crate::phy::{PhyCollisionManager, PhyWorld};

pub(super) struct OdeHandle {
	_private: PhantomData<()>,
}

impl OdeHandle {
	pub fn new() -> Self {
		unsafe { dInitODE(); }
		Self { _private: Default::default() }
	}

	pub fn create_world(&self) -> OdeWorld {
		OdeWorld::new()
	}
}

pub fn get_configuration() -> &'static str {
	str_from_c(unsafe { dGetConfiguration() })
}

impl Drop for OdeHandle {
	fn drop(&mut self) {
		unsafe { dCloseODE() }
	}
}

pub(super) struct OdeWorld {
	id: dWorldID,
}

impl OdeWorld {
	fn new() -> Self {
		Self {
			id: unsafe { dWorldCreate() }
		}
	}

	pub(crate) fn new_body(&self, mass: OdeMass) -> OdeBody {
		OdeBody::new(self.id, mass)
	}

	pub fn set_gravity(&self, x: f64, y: f64, z: f64) {
		unsafe { dWorldSetGravity(self.id, x, y, z); }
	}

	/// Could probably use our own buffer in struct instead
	pub fn get_gravity(&self) -> [f64; 3] {
		let mut gravity: [MaybeUninit<_>; 4] = [MaybeUninit::uninit(); 4];
		unsafe { dWorldGetGravity(self.id, gravity[0].as_mut_ptr()); }
		#[allow(clippy::missing_transmute_annotations)]
		let gravity = unsafe { transmute::<_, [f64; 4]>(gravity) };
		[gravity[0], gravity[1], gravity[2]]
	}

	pub fn set_erp(&self, erp: f64) {
		unsafe { dWorldSetERP(self.id, erp) }
	}

	pub fn get_erp(&self) -> f64 {
		unsafe { dWorldGetERP(self.id) }
	}

	pub fn set_cfm(&self, cfm: f64) {
		unsafe { dWorldSetCFM(self.id, cfm) }
	}

	pub fn get_cfm(&self) -> f64 {
		unsafe { dWorldGetCFM(self.id) }
	}

	pub fn set_auto_disable_flag(&self, auto_disable: bool) {
		unsafe { dWorldSetAutoDisableFlag(self.id, auto_disable.into()) }
	}

	pub fn get_auto_disable_flag(&self) -> bool {
		unsafe { dWorldGetAutoDisableFlag(self.id) != 0 }
	}

	pub fn impulse_to_force(&self, stepsize: f64, ix: f64, iy: f64, iz: f64) -> [f64; 3] {
		let mut force: [MaybeUninit<_>; 4] = [MaybeUninit::uninit(); 4];
		unsafe { dWorldImpulseToForce(self.id, stepsize, ix, iy, iz, force[0].as_mut_ptr()) }
		#[allow(clippy::missing_transmute_annotations)]
		let force = unsafe { transmute::<_, [f64; 4]>(force) };
		[force[0], force[1], force[2]]
	}

	pub fn step(&self, stepsize: f64) {
		if unsafe { dWorldStep(self.id, stepsize) == 0 } {
			println!("step failed") // What to do with this?
		}
	}

	// Currently QuickStep is not considered.

	/// `mass` must not be moved afterwards.
	pub fn create_body(&self, mass: OdeMass) -> OdeBody {
		OdeBody::new(self.id, mass)
	}

	pub fn new_joint_contact(&self, contact: OdeContact) -> OdeJoint {
		let contact = contact.into_inner();
		unsafe { OdeJoint::new(dJointCreateContact(self.id, null_mut(), &raw const contact)) }
	}

	/// `path` must be a path of a file to be created.
	pub fn export_dif(&self, path: impl AsRef<str>) {
		let file = create_file_c(path);
		let name = CString::new("foobar").expect("Cannot create CString");
		unsafe { dWorldExportDIF(self.id, file.as_ptr() as *mut _, name.as_ptr()) }
	}
}

impl Drop for OdeWorld {
	fn drop(&mut self) {
		unsafe { dWorldDestroy(self.id) }
	}
}

#[derive(Getters, MutGetters)]
pub(super) struct OdeBody {
	id: dBodyID,
	/// [`set_mass`][Self::set_mass] must be called whenever mutation is made.
	#[getset(get = "pub", get_mut = "pub")]
	mass: OdeMass,
}

impl OdeBody {
	/// `mass` must not be moved afterwards.
	fn new(world: dWorldID, mass: OdeMass) -> Self {
		let body = Self {
			id: unsafe { dBodyCreate(world) },
			mass,
		};
		body.set_mass();
		body
	}

	pub fn set_position(&self, x: f64, y: f64, z: f64) {
		unsafe { dBodySetPosition(self.id, x, y, z); }
	}

	pub fn set_rotation(&self, r: &OdeMat3) {
		unsafe { dBodySetRotation(self.id, r.as_ptr()); }
	}

	pub fn set_quaternion(&self, q: &OdeQuat) {
		unsafe { dBodySetQuaternion(self.id, q.as_ptr()); }
	}

	pub fn set_linear_vel(&self, x: f64, y: f64, z: f64) {
		unsafe { dBodySetLinearVel(self.id, x, y, z); }
	}

	pub fn set_angular_vel(&self, x: f64, y: f64, z: f64) {
		unsafe { dBodySetAngularVel(self.id, x, y, z); }
	}

	pub fn get_position(&self) -> &[f64; 3] {
		unsafe { &*dBodyGetPosition(self.id).cast() }
	}

	pub fn get_rotation(&self) -> &[f64; 12] {
		unsafe { &*dBodyGetRotation(self.id).cast() }
	}

	pub fn get_quaternion(&self) -> &[f64; 4] {
		unsafe { &*dBodyGetQuaternion(self.id).cast() }
	}

	pub fn get_linear_vel(&self) -> &[f64; 3] {
		unsafe { &*dBodyGetLinearVel(self.id).cast() }
	}

	pub fn get_angular_vel(&self) -> &[f64; 3] {
		unsafe { &*dBodyGetAngularVel(self.id).cast() }
	}

	/// This must be called whenever mutation to `self.mass` is done to sync value.
	pub fn set_mass(&self) {
		// This copies `self.mass` to ODE's dBody
		unsafe { dBodySetMass(self.id, &raw const self.mass.data); }
	}

	pub fn add_force(&self, fx: f64, fy: f64, fz: f64) {
		unsafe { dBodyAddForce(self.id, fx, fy, fz); }
	}

	pub fn add_torque(&self, fx: f64, fy: f64, fz: f64) {
		unsafe { dBodyAddTorque(self.id, fx, fy, fz); }
	}

	pub fn add_rel_force(&self, fx: f64, fy: f64, fz: f64) {
		unsafe { dBodyAddRelForce(self.id, fx, fy, fz); }
	}

	pub fn add_rel_torque(&self, fx: f64, fy: f64, fz: f64) {
		unsafe { dBodyAddTorque(self.id, fx, fy, fz); }
	}

	pub fn add_force_at_pos(&self, fx: f64, fy: f64, fz: f64, px: f64, py: f64, pz: f64) {
		unsafe { dBodyAddForceAtPos(self.id, fx, fy, fz, px, py, pz); }
	}

	pub fn add_force_at_rel_pos(&self, fx: f64, fy: f64, fz: f64, px: f64, py: f64, pz: f64) {
		unsafe { dBodyAddForceAtRelPos(self.id, fx, fy, fz, px, py, pz) }
	}

	pub fn add_rel_force_at_pos(&self, fx: f64, fy: f64, fz: f64, px: f64, py: f64, pz: f64) {
		unsafe { dBodyAddRelForceAtPos(self.id, fx, fy, fz, px, py, pz) }
	}

	pub fn add_rel_force_at_rel_pos(&self, fx: f64, fy: f64, fz: f64, px: f64, py: f64, pz: f64) {
		unsafe { dBodyAddRelForceAtRelPos(self.id, fx, fy, fz, px, py, pz) }
	}

	pub fn set_dynamic(&self) {
		unsafe { dBodySetDynamic(self.id) }
	}

	pub fn set_kinematic(&self) {
		unsafe { dBodySetKinematic(self.id) }
	}

	pub fn is_kinematic(&self) -> bool {
		unsafe { dBodyIsKinematic(self.id) == 1 }
	}

	pub fn enable(&self) {
		unsafe { dBodyEnable(self.id) }
	}

	pub fn disable(&self) {
		unsafe { dBodyDisable(self.id) }
	}

	pub fn is_enabled(&self) -> bool {
		unsafe { dBodyIsEnabled(self.id) == 1 }
	}

	pub fn set_moved_callback(&self) {
		unsafe { dBodySetMovedCallback(self.id, todo!()) }
	}

	pub fn set_gravity_mode(&self, mode: bool) {
		unsafe { dBodySetGravityMode(self.id, if mode { 1 } else { 0 }) }
	}

	pub fn get_gravity_mode(&self) -> bool {
		unsafe { dBodyGetGravityMode(self.id) != 0 }
	}
}

impl Drop for OdeBody {
	fn drop(&mut self) {
		unsafe { dBodyDestroy(self.id) }
	}
}

#[derive(Clone, Copy, Debug)]
pub struct OdeMass {
	/// Directly mutating values may cause invalid mass.
	data: dMass,
}

impl Default for OdeMass {
    fn default() -> Self {
        Self::new()
    }
}

impl OdeMass {
	/// Creates a new instance with all parameters set to zero
	/// with the same effect as [`set_to_zero`][Self::set_to_zero].
	pub fn new() -> Self {
		Self {
			data: dMass { // Literally `dMassSetZero`
				mass: 0.0,
				c: [0.0; 4],
				I: [0.0; 12],
			}
		}
	}

	pub fn set_zero(&mut self) {
		unsafe { dMassSetZero(&raw mut self.data) }
	}

	#[allow(clippy::too_many_arguments)]
	pub fn set_parameters(&mut self, mass: f64, cgx: f64, cgy: f64, cgz: f64, i11: f64, i22: f64, i33: f64, i12: f64, i13: f64, i23: f64) {
		unsafe { dMassSetParameters(&raw mut self.data, mass, cgx, cgy, cgz, i11, i22, i33, i12, i13, i23) }
	}

	pub fn set_sphere(&mut self, density: f64, radius: f64) {
		unsafe { dMassSetSphere(&raw mut self.data, density, radius) }
	}

	pub fn set_sphere_total(&mut self, total_mass: f64, radius: f64) {
		unsafe { dMassSetSphereTotal(&raw mut self.data, total_mass, radius) }
	}

	pub fn set_capsule(&mut self, density: f64, direction: i32, radius: f64, length: f64) {
		unsafe { dMassSetCapsule(&raw mut self.data, density, direction, radius, length) }
	}

	pub fn set_capsule_total(&mut self, total_mass: f64, direction: i32, radius: f64, length: f64) {
		unsafe { dMassSetCapsuleTotal(&raw mut self.data, total_mass, direction, radius, length) }
	}

	pub fn set_cylinder(&mut self, density: f64, direction: i32, radius: f64, length: f64) {
		unsafe { dMassSetCylinder(&raw mut self.data, density, direction, radius, length) }
	}

	pub fn set_cylinder_total(&mut self, total_mass: f64, direction: i32, radius: f64, length: f64) {
		unsafe { dMassSetCylinderTotal(&raw mut self.data, total_mass, direction, radius, length) }
	}

	pub fn set_box(&mut self, density: f64, lx: f64, ly: f64, lz: f64) {
		unsafe { dMassSetBox(&raw mut self.data, density, lx, ly, lz) }
	}

	pub fn set_box_total(&mut self, total_mass: f64, lx: f64, ly: f64, lz: f64) {
		unsafe { dMassSetBoxTotal(&raw mut self.data, total_mass, lx, ly, lz) }
	}

	pub fn set_trimesh(&mut self, density: f64, g: &impl OdePlaceableGeom) {
		unsafe { dMassSetTrimesh(&raw mut self.data, density, g.id()) }
	}

	pub fn set_trimesh_total(&mut self, total_mass: f64, g: &impl OdePlaceableGeom) {
		unsafe { dMassSetTrimeshTotal(&raw mut self.data, total_mass, g.id()) }
	}

	pub fn adjust(&mut self, new_mass: f64) {
		unsafe { dMassAdjust(&raw mut self.data, new_mass) }
	}

	pub fn translate(&mut self, x: f64, y: f64, z: f64) {
		unsafe { dMassTranslate(&raw mut self.data, x, y, z) }
	}

	pub fn rotate(&mut self, r: &OdeMat3) {
		unsafe { dMassRotate(&raw mut self.data, r.as_ptr()) }
	}

	pub fn add(&mut self, b: &mut OdeMass) {
		unsafe { dMassAdd(&mut self.data, &b.data) }
	}
}

mod private {
	pub trait Sealed {}
}

/// Source: https://stackoverflow.com/a/57749870
pub trait OdePlaceabilityMarker : private::Sealed {}

pub enum OdePlaceableMarker {}
pub enum OdeNonPlaceableMarker {}

impl<T: OdePlaceabilityMarker> private::Sealed for T {}

impl OdePlaceabilityMarker for OdePlaceableMarker {}
impl OdePlaceabilityMarker for OdeNonPlaceableMarker {}

pub trait OdeGeom : Drop {
	type Placeability: OdePlaceabilityMarker;

	/// For internal use only.
	fn id(&self) -> dGeomID;

	/// This must be called by [Drop] impl.
	fn drop(&mut self) {
		unsafe { dGeomDestroy(self.id()) }
	}

	fn set_category_bits(&self, bits: u32) {
		unsafe { dGeomSetCategoryBits(self.id(), bits) }
	}

	fn set_collide_bits(&self, bits: u32) {
		unsafe { dGeomSetCollideBits(self.id(), bits) }
	}

	fn enable(&self) {
		unsafe { dGeomEnable(self.id()) }
	}

	fn disable(&self) {
		unsafe { dGeomDisable(self.id()) }
	}

	fn is_enabled(&self) -> bool {
		unsafe { dGeomIsEnabled(self.id()) == 1 }
	}
}

pub type OdeGeomPlaceable = dyn OdeGeom<Placeability = OdePlaceableMarker>;
pub type OdeGeomNonPlaceable = dyn OdeGeom<Placeability = OdeNonPlaceableMarker>;

pub trait OdeNonPlaceableGeom : OdeGeom<Placeability = OdeNonPlaceableMarker> {}

impl<T: OdeGeom<Placeability = OdeNonPlaceableMarker>> OdeNonPlaceableGeom for T {}
impl<T: ?Sized + OdeGeom<Placeability = OdePlaceableMarker>> OdePlaceableGeom for T {}

pub trait OdePlaceableGeom : OdeGeom<Placeability = OdePlaceableMarker> {
	fn set_body(&self, body: &OdeBody) {
		unsafe { dGeomSetBody(self.id(), body.id) }
	}

	fn set_position(&self, x: f64, y: f64, z: f64) {
		unsafe { dGeomSetPosition(self.id(), x, y, z) }
	}

	fn set_rotation(&self, r: &OdeMat3) {
		unsafe { dGeomSetRotation(self.id(), r.as_ptr()) }
	}

	fn set_quaternion(&self, q: &OdeQuat) {
		unsafe { dGeomSetQuaternion(self.id(), q.as_ptr()) }
	}

	fn get_position(&self) -> &[f64; 3] {
		unsafe { &*dGeomGetPosition(self.id()).cast() }
	}

	fn get_rotation(&self) -> &[f64; 12] {
		unsafe { &*dGeomGetRotation(self.id()).cast() }
	}

	fn get_quaternion(&self) -> [f64; 4] {
		let mut q: [MaybeUninit<_>; 4] = [MaybeUninit::uninit(); 4];
		unsafe { dGeomGetQuaternion(self.id(), q[0].as_mut_ptr()) };
		#[allow(clippy::missing_transmute_annotations)]
		unsafe { transmute::<_, [f64; 4]>(q) }
	}

	// Since offset functions below require Geom to be attached to a Body, so must be placeable.

	fn set_offset_position(&self, x: f64, y: f64, z: f64) {
		unsafe { dGeomSetOffsetPosition(self.id(), x, y, z) }
	}

	fn set_offset_rotation(&self, r: &OdeMat3) {
		unsafe { dGeomSetOffsetRotation(self.id(), r.as_ptr()) }
	}

	fn set_offset_quaternion(&self, q: &OdeQuat) {
		unsafe { dGeomSetOffsetQuaternion(self.id(), q.as_ptr()) }
	}

	fn set_offset_world_position(&self, x: f64, y: f64, z: f64) {
		unsafe { dGeomSetOffsetWorldPosition(self.id(), x, y, z) }
	}

	fn set_offset_world_rotation(&self, r: &OdeMat3) {
		unsafe { dGeomSetOffsetWorldRotation(self.id(), r.as_ptr()) }
	}

	fn set_offset_world_quaternion(&self, q: &OdeQuat) {
		unsafe { dGeomSetOffsetWorldQuaternion(self.id(), q.as_ptr()) }
	}

	fn get_offset_position(&self) -> &[f64; 3] {
		unsafe { &*dGeomGetOffsetPosition(self.id()).cast() }
	}

	fn get_offset_rotation(&self) -> &[f64; 9] {
		unsafe { &*dGeomGetOffsetRotation(self.id()).cast() }
	}

	fn get_offset_quaternion(&self) -> [f64; 4] {
		let mut q: [MaybeUninit<_>; 4] = [MaybeUninit::uninit(); 4];
		unsafe { dGeomGetOffsetQuaternion(self.id(), q[0].as_mut_ptr()) };
		#[allow(clippy::missing_transmute_annotations)]
		unsafe { transmute::<_, [f64; 4]>(q) }
	}

	fn clear_offset(&self) {
		unsafe { dGeomClearOffset(self.id()) }
	}

	fn get_aabb(&self) -> [f64; 6] {
		geom_get_aabb(self)
	}
}

/// Used only for placeable Geom and Space
fn geom_get_aabb(geom: &(impl OdeGeom + ?Sized)) -> [f64; 6] {
	let mut q: [MaybeUninit<_>; 6] = [MaybeUninit::uninit(); 6];
	unsafe { dGeomGetAABB(geom.id(), q[0].as_mut_ptr()) };
	#[allow(clippy::missing_transmute_annotations)]
	unsafe { transmute::<_, [f64; 6]>(q) }
}

#[derive(Getters)]
pub struct OdeSphere {
	id: dGeomID,
	#[get = "pub"]
	radius: f64,
}

impl Drop for OdeSphere {
	fn drop(&mut self) {
		OdeGeom::drop(self)
	}
}

impl OdeGeom for OdeSphere {
	type Placeability = OdePlaceableMarker;

	fn id(&self) -> dGeomID {
		self.id
	}
}

impl OdeSphere {
	pub fn new(space: Option<&OdeSpace>, radius: f64) -> Self {
		Self { id: unsafe { dCreateSphere(space.into_space_id(), radius) }, radius }
	}

	pub fn set_radius(&mut self, radius: f64) {
		self.radius = radius;
		unsafe { dGeomSphereSetRadius(self.id, radius); }
	}
}

#[derive(Getters)]
pub struct OdeBox {
	id: dGeomID,
	#[get = "pub"]
	lengths: DVec3,
}

impl Drop for OdeBox {
	fn drop(&mut self) {
		OdeGeom::drop(self)
	}
}

impl OdeGeom for OdeBox {
	type Placeability = OdePlaceableMarker;

	fn id(&self) -> dGeomID {
		self.id
	}
}

impl OdeBox {
	pub fn new(space: Option<&OdeSpace>, lengths: DVec3) -> Self {
		Self { id: unsafe { dCreateBox(space.into_space_id(), lengths.x, lengths.y, lengths.z) }, lengths }
	}

	pub fn set_lengths(&mut self, lengths: DVec3) {
		self.lengths = lengths;
		unsafe { dGeomBoxSetLengths(self.id, lengths.x, lengths.y, lengths.z) }
	}
}

#[derive(Getters)]
pub struct OdeCapsule {
	id: dGeomID,
	#[get = "pub"]
	radius: f64,
	#[get = "pub"]
	length: f64,
}

impl Drop for OdeCapsule {
	fn drop(&mut self) {
		OdeGeom::drop(self)
	}
}

impl OdeGeom for OdeCapsule {
	type Placeability = OdePlaceableMarker;

	fn id(&self) -> dGeomID {
		self.id
	}
}

impl OdeCapsule {
	pub fn new(space: Option<&OdeSpace>, radius: f64, length: f64) -> Self {
		Self { id: unsafe { dCreateCapsule(space.into_space_id(), radius, length) }, radius, length }
	}

	pub fn set_params(&mut self, radius: f64, length: f64) {
		self.radius = radius;
		self.length = length;
		unsafe { dGeomCapsuleSetParams(self.id, radius, length); }
	}
}

#[derive(Getters)]
pub struct OdeCylinder {
	id: dGeomID,
	#[get = "pub"]
	radius: f64,
	#[get = "pub"]
	length: f64,
}

impl Drop for OdeCylinder {
	fn drop(&mut self) {
		OdeGeom::drop(self)
	}
}

impl OdeGeom for OdeCylinder {
	type Placeability = OdePlaceableMarker;

	fn id(&self) -> dGeomID {
		self.id
	}
}

impl OdeCylinder {
	pub fn new(space: Option<&OdeSpace>, radius: f64, length: f64) -> Self {
		Self { id: unsafe { dCreateCylinder(space.into_space_id(), radius, length) }, radius, length }
	}

	pub fn set_params(&mut self, radius: f64, length: f64) {
		self.radius = radius;
		self.length = length;
		unsafe { dGeomCylinderSetParams(self.id, radius, length); }
	}
}

#[derive(Getters)]
pub struct OdePlane {
	id: dGeomID,
	#[get = "pub"]
	params: DVec4,
}

impl Drop for OdePlane {
	fn drop(&mut self) {
		OdeGeom::drop(self)
	}
}

impl OdeGeom for OdePlane {
	type Placeability = OdeNonPlaceableMarker;

	fn id(&self) -> dGeomID {
		self.id
	}
}

impl OdePlane {
	pub fn new(space: Option<&OdeSpace>, params: DVec4) -> Self {
		Self { id: unsafe { dCreatePlane(space.into_space_id(), params.x, params.y, params.z, params.w) }, params }
	}

	pub fn set_params(&mut self, params: DVec4) {
		self.params = params;
		unsafe { dGeomPlaneSetParams(self.id, params.x, params.y, params.z, params.w); }
	}
}

#[derive(Getters)]
pub struct OdeRay {
	id: dGeomID,
	#[get = "pub"]
	length: f64,
	#[get = "pub"]
	pos: DVec3,
	#[get = "pub"]
	dir: DVec3,
	#[get = "pub"]
	first_contact: bool,
	#[get = "pub"]
	backface_cull: bool,
	#[get = "pub"]
	closest_hit: bool,
}

impl Drop for OdeRay {
	fn drop(&mut self) {
		OdeGeom::drop(self)
	}
}

impl OdeGeom for OdeRay {
	type Placeability = OdePlaceableMarker;

	fn id(&self) -> dGeomID {
		self.id
	}
}

impl OdeRay {
	pub fn new(space: Option<&OdeSpace>, length: f64, pos: DVec3, dir: DVec3) -> Self {
		let id = unsafe { dCreateRay(space.into_space_id(), length) };
		let mut ray = Self {
			id,
			length,
			pos,
			dir,
			first_contact: false,
			backface_cull: false,
			closest_hit: false,
		};
		ray.set(pos, dir);
		ray
	}

	pub fn set_length(&mut self, length: f64) {
		self.length = length;
		unsafe { dGeomRaySetLength(self.id, length) }
	}

	/// Caveat: If this is called before [`set_body`][OdePlaceableGeom::set_body],
	/// these values becomes invalid and the values from the Body would be used instead.
	pub fn set(&mut self, pos: DVec3, mut dir: DVec3) {
		self.pos = pos;
		unsafe { dNormalize3(dir.as_mut_ptr()) }
		self.dir = dir;
		unsafe { dGeomRaySet(self.id, pos.x, pos.y, pos.z, dir.x, dir.y, dir.z); }
	}

	pub fn set_params(&mut self, first_contact: bool, backface_cull: bool) {
		self.first_contact = first_contact;
		self.backface_cull = backface_cull;
		unsafe { dGeomRaySetParams(self.id, first_contact.into(), backface_cull.into()); }
	}

	pub fn set_first_contact(&mut self, first_contact: bool) {
		self.first_contact = first_contact;
		unsafe { dGeomRaySetFirstContact(self.id, first_contact.into()); }
	}

	pub fn set_backface_cull(&mut self, backface_cull: bool) {
		self.backface_cull = backface_cull;
		unsafe { dGeomRaySetBackfaceCull(self.id, backface_cull.into()); }
	}

	pub fn set_closest_hit(&mut self, closest_hit: bool) {
		self.closest_hit = closest_hit;
		unsafe { dGeomRaySetClosestHit(self.id, closest_hit.into()); }
	}
}

pub struct OdeTrimesh {
	id: dGeomID,
}

impl Drop for OdeTrimesh {
	fn drop(&mut self) {
		OdeGeom::drop(self)
	}
}

impl OdeGeom for OdeTrimesh {
	type Placeability = OdePlaceableMarker;

	fn id(&self) -> dGeomID {
		self.id
	}
}

impl OdeTrimesh {
	// Incomplete; TBD when this is really required, too complicated.
}

pub struct OdeSpace {
	id: dSpaceID,
}

impl Drop for OdeSpace {
	fn drop(&mut self) {
		unsafe { dSpaceDestroy(self.id) }
	}
}

impl OdeGeom for OdeSpace {
	type Placeability = OdeNonPlaceableMarker;

	fn id(&self) -> dGeomID {
		self.id as _
	}
}

const MAX_CONTACTS: u32 = 256;

// Source: https://ode.org/wiki/index.php/Manual
unsafe extern "C" fn near_callback(data: *mut c_void, o1: dGeomID, o2: dGeomID) {
	unsafe {
		let is_1_space = dGeomIsSpace (o1) != 0;
		let is_2_space = dGeomIsSpace (o2) != 0;
		if is_1_space || is_2_space {
			// colliding a space with something :
			dSpaceCollide2 (o1, o2, data, Some(near_callback));
			let contact_manager = &mut *(data as *mut OdeContactManager);
			for space in match (is_1_space, is_2_space) {
				(true, true) => vec![o1, o2],
				(true, false) => vec![o1],
				(false, true) => vec![o2],
				(false, false) => vec![],
			} {
				let v = ByAddress(Box::new(space as _));
				if contact_manager.omitted_spaces.contains(&v) { return }
				// collide all geoms internal to the space(s)
				dSpaceCollide(space as _, data, Some(near_callback));
			}
		} else {
			// colliding two non-space geoms, so generate contact
			// points between o1 and o2
			let g1 = {
				let g = dGeomGetBody(o1);
				if g.is_null() { None } else { Some(g) }
			};
			let g2 = {
				let g = dGeomGetBody(o2);
				if g.is_null() { None } else { Some(g) }
			};
			if g1.is_none() && g2.is_none() { return }
			fn is_moving(body: dBodyID) -> bool {
				fn is_zero(vec: *const f64) -> bool {
					unsafe { *vec == 0.0 && *vec.add(1) == 0.0 && *vec.add(2) == 0.0 }
				}
				unsafe {
					is_zero(dBodyGetLinearVel(body)) && is_zero(dBodyGetForce(body))
				}
			}
			if (g1.is_none() || g1.is_some() && !is_moving(g1.unwrap())) &&
				(g2.is_none() || g2.is_some() && !is_moving(g2.unwrap())) { return }
			let mut contact_array = [const { MaybeUninit::uninit() }; MAX_CONTACTS as _];
			let num_contact = dCollide(o1, o2, MAX_CONTACTS as _, contact_array[0].as_mut_ptr(), size_of::<dContactGeom>() as _);
			// add these contact points to the simulation ...
			let contact_manager = &mut *(data as *mut OdeContactManager);
			contact_array[0..(num_contact as _)]
				.iter_mut()
				.map(|e| e.assume_init_read())
				.for_each(|g| contact_manager.buf.push(g));
		}
	}
}

#[allow(clippy::identity_op)]
pub enum OdeSapAxisOrder {
	Xyz = (0)|(1<<2)|(2<<4),
	Xzy = (0)|(2<<2)|(1<<4),
	Yxz = (1)|(0<<2)|(2<<4),
	Yzx = (1)|(2<<2)|(0<<4),
	Zxy = (2)|(0<<2)|(1<<4),
	Zyx = (2)|(1<<2)|(0<<4),
}

trait IntoSpaceId {
	fn into_space_id(self) -> dSpaceID;
}

impl IntoSpaceId for Option<&OdeSpace> {
	fn into_space_id(self) -> dSpaceID {
		match self {
			None => null_mut() as _,
			Some(parent) => parent.id,
		}
	}
}

impl OdeSpace {
	pub fn new_simple(parent: Option<&Self>) -> Self {
		Self { id: unsafe { dSimpleSpaceCreate(parent.into_space_id()) } }
	}

	pub fn new_hash(parent: Option<&Self>, min_level: i32, max_level: i32) -> Self {
		let id = unsafe { dHashSpaceCreate(parent.into_space_id()) };
		unsafe { dHashSpaceSetLevels(id, min_level, max_level) };
		Self { id }
	}

	pub fn new_quadtree(parent: Option<&Self>, center: DVec3, extends: DVec3, depth: i32) -> Self {
		Self { id: unsafe { dQuadTreeSpaceCreate(parent.into_space_id(), center.as_ptr(), extends.as_ptr(), depth) } }
	}

	pub fn new_sap(parent: Option<&Self>, order: OdeSapAxisOrder) -> Self {
		Self { id: unsafe { dSweepAndPruneSpaceCreate(parent.into_space_id(), order as _) } }
	}

	pub fn get_aabb(&self) -> [f64; 6] {
		geom_get_aabb(self)
	}

	pub fn collide(&self, contact_manager: &mut OdeContactManager) {
		unsafe { dSpaceCollide(self.id, contact_manager as *mut _ as _, Some(near_callback)) }
	}

	pub fn add(&self, geom: &impl OdeGeom) {
		unsafe { dSpaceAdd(self.id, geom.id()) }
	}

	pub fn remove(&self, geom: &impl OdeGeom) {
		unsafe { dSpaceRemove(self.id, geom.id()) }
	}

	pub fn query(&self, geom: &impl OdeGeom) -> i32 {
		unsafe { dSpaceQuery(self.id, geom.id()) }
	}

	pub fn get_nums_geom(&self) -> i32 {
		unsafe { dSpaceGetNumGeoms(self.id) }
	}
}

pub struct OdeContactManager {
	buf: Vec<dContactGeom>,
	omitted_spaces: OrderSet<ByAddress<Box<dSpaceID>>>,
}

impl Default for OdeContactManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OdeContactManager {
	pub fn new() -> Self {
		Self {
			buf: Vec::new(),
			omitted_spaces: OrderSet::new(),
		}
	}

	pub fn omit_space(&mut self, space: &OdeSpace) {
		self.omitted_spaces.insert(ByAddress(Box::new(space.id)));
	}

	pub(super) fn process(&mut self, world: &OdeWorld) {
		for geom in self.buf.drain(..) {
			world.new_joint_contact(OdeContact::new(geom));
		}
	}
}

pub struct OdeContact {
	geom: dContactGeom,
}

impl OdeContact {
	fn new(geom: dContactGeom) -> Self {
		Self { geom }
	}

	fn into_inner(self) -> dContact {
		// TODO fill in other fields
		let mut surface = MaybeUninit::<dSurfaceParameters>::uninit();
		unsafe { (&raw mut (*surface.as_mut_ptr()).mu).write(f64::INFINITY); }
		let surface = unsafe { surface.assume_init() };
		#[allow(clippy::uninit_assumed_init, invalid_value)]
		dContact {
			surface,
			geom: self.geom,
			fdir1: unsafe { MaybeUninit::uninit().assume_init() },
		}
	}
}

pub struct OdeJoint {
	id: dJointID,
}

impl OdeJoint {
	fn new(id: dJointID) -> Self {
		Self { id }
	}
}

impl Drop for OdeJoint {
	fn drop(&mut self) {
		unsafe { dJointDestroy(self.id); }
	}
}

/// ODE's row-major 3x3 matrix representation with padding
pub struct OdeMat3 {
	data: [f64; 12]
}

impl OdeMat3 {
	pub fn from_arr(arr: [f64; 9]) -> Self {
		Self { data: concat_arrays(arr, [0.0; 3]) }
	}

	pub fn from_alg(alg: &DMat3) -> Self {
		Self {
			data: [
				alg.m11, alg.m12, alg.m13,
				alg.m21, alg.m22, alg.m23,
				alg.m31, alg.m32, alg.m33,
				0.0, 0.0, 0.0,
			]
		}
	}

	pub fn to_alg(&self) -> DMat3 {
		DMat3::new(
			self.data[0],
			self.data[1],
			self.data[2],
			self.data[3],
			self.data[4],
			self.data[5],
			self.data[6],
			self.data[7],
			self.data[8],
		)
	}

	pub fn as_ptr(&self) -> *const f64 {
		self.data.as_ptr()
	}
}

/// ODE's row-major 4x4 matrix representation
pub struct OdeMat4 {
	data: [f64; 16]
}

impl OdeMat4 {
	pub fn from_arr(arr: [f64; 16]) -> Self {
		Self { data: arr }
	}

	pub fn from_alg(alg: &DMat4) -> Self {
		Self {
			data: [
				alg.m11, alg.m12, alg.m13, alg.m14,
				alg.m21, alg.m22, alg.m23, alg.m24,
				alg.m31, alg.m32, alg.m33, alg.m34,
				alg.m41, alg.m42, alg.m43, alg.m44,
			]
		}
	}

	pub fn to_alg(&self) -> DMat4 {
		DMat4::from_row_slice(&self.data)
	}

	pub fn as_ptr(&self) -> *const f64 {
		self.data.as_ptr()
	}
}

/// ODE's row-major 4x4 matrix representation
pub struct OdeMat4x3 {
	data: [f64; 12]
}

impl OdeMat4x3 {
	pub fn from_arr(arr: [f64; 12]) -> Self {
		Self { data: arr }
	}

	pub fn from_alg(alg: &DMat4x3) -> Self {
		Self {
			data: [
				alg.m11, alg.m12, alg.m13,
				alg.m21, alg.m22, alg.m23,
				alg.m31, alg.m32, alg.m33,
				alg.m41, alg.m42, alg.m43,
			]
		}
	}

	pub fn to_alg(&self) -> DMat4x3 {
		DMat4x3::from_row_slice(&self.data)
	}

	pub fn as_ptr(&self) -> *const f64 {
		self.data.as_ptr()
	}
}

/// ODE's quaternion representation (w, x, y, z)
pub struct OdeQuat {
	data: [f64; 4]
}

impl OdeQuat {
	pub fn from_arr(arr: [f64; 4]) -> Self {
		Self { data: arr }
	}

	pub fn from_alg(alg: &DQuat) -> Self {
		Self {
			data: [alg.w, alg.i, alg.j, alg.k]
		}
	}

	pub fn to_alg(&self) -> DQuat {
		DQuat::new(
			self.data[0],
			self.data[1],
			self.data[2],
			self.data[3],
		)
	}

	pub fn as_ptr(&self) -> *const f64 {
		self.data.as_ptr()
	}
}
