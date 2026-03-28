/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use crate::util::str_from_c;
use nalgebra_glm::{DMat3, DMat4, DMat4x3, DQuat};
use ode_sys::{dBodyAddForce, dBodyAddForceAtPos, dBodyAddForceAtRelPos, dBodyAddRelForce, dBodyAddRelForceAtPos, dBodyAddRelForceAtRelPos, dBodyAddTorque, dBodyCreate, dBodyDestroy, dBodyDisable, dBodyEnable, dBodyGetAngularVel, dBodyGetGravityMode, dBodyGetLinearVel, dBodyGetMass, dBodyGetPosition, dBodyGetQuaternion, dBodyGetRotation, dBodyID, dBodyIsEnabled, dBodyIsKinematic, dBodySetAngularVel, dBodySetDynamic, dBodySetGravityMode, dBodySetKinematic, dBodySetLinearVel, dBodySetMass, dBodySetMovedCallback, dBodySetPosition, dBodySetQuaternion, dBodySetRotation, dCloseODE, dGetConfiguration, dInitODE, dMass, dWorldCreate, dWorldDestroy, dWorldExportDIF, dWorldGetAutoDisableFlag, dWorldGetCFM, dWorldGetERP, dWorldGetGravity, dWorldID, dWorldImpulseToForce, dWorldSetAutoDisableFlag, dWorldSetCFM, dWorldSetERP, dWorldSetGravity, dWorldStep};
use std::mem::{transmute, MaybeUninit};

pub(super) struct OdeHandle {
}

impl OdeHandle {
	pub fn new() -> Self {
		unsafe { dInitODE(); }
		Self {}
	}

	pub fn create_world(&self) -> OdeWorld {
		OdeWorld::new()
	}
}

fn get_configuration() -> &'static str {
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

	pub fn create_body(&self) -> OdeBody {
		OdeBody::new(self.id)
	}

	pub fn export_dif(&self) {
		unsafe { dWorldExportDIF(self.id, todo!(), todo!()) }
	}
}

impl Drop for OdeWorld {
	fn drop(&mut self) {
		unsafe { dWorldDestroy(self.id) }
	}
}

pub(super) struct OdeBody {
	id: dBodyID,
}

impl OdeBody {
	fn new(world: dWorldID) -> Self {
		Self { id: unsafe { dBodyCreate(world) }}
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

	pub fn set_mass(&self, mass: &OdeMass) {
		unsafe { dBodySetMass(self.id, mass); }
	}

	pub fn get_mass(&self) -> OdeMass {
		unsafe { dBodyGetMass(self.id) }
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

pub struct OdeMass {
	data: dMass,
}

/// ODE's row-major 3x3 matrix representation
pub struct OdeMat3 {
	data: [f64; 9]
}

impl OdeMat3 {
	pub fn from_arr(arr: [f64; 9]) -> Self {
		Self { data: arr }
	}

	pub fn from_alg(alg: &DMat3) -> Self {
		Self {
			data: [
				alg.m11, alg.m12, alg.m13,
				alg.m21, alg.m22, alg.m23,
				alg.m31, alg.m32, alg.m33,
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
		DMat4::new(
			self.data[0],
			self.data[1],
			self.data[2],
			self.data[3],
			self.data[4],
			self.data[5],
			self.data[6],
			self.data[7],
			self.data[8],
			self.data[9],
			self.data[10],
			self.data[11],
			self.data[12],
			self.data[13],
			self.data[14],
			self.data[15],
		)
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
		DMat4x3::new(
			self.data[0],
			self.data[1],
			self.data[2],
			self.data[3],
			self.data[4],
			self.data[5],
			self.data[6],
			self.data[7],
			self.data[8],
			self.data[9],
			self.data[10],
			self.data[11],
		)
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
