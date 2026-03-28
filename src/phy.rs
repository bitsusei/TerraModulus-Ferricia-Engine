/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use crate::phy::ode::OdeHandle;

mod ode;

pub struct PhyEnv {
	ode_handle: OdeHandle,
}

impl PhyEnv {
	pub fn new() -> Self {
		Self {
			ode_handle: OdeHandle::new(),
		}
	}

	pub fn create_world(&self) {

	}
}
