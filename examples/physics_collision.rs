/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use nalgebra_glm::DVec3;
use ferricia::phy::{OdeBox, OdePlaceableGeom, PhyCollisionManager, PhyEnv, PhyRawGeom};

const SPACE_OMIT: bool = false;
const BITS: bool = false;

//noinspection RsConstantConditionIf
fn main() {
	let env = PhyEnv::new();
	let world = env.create_world();
	let space = world.create_space(1..=12);
	let mut geoms = Vec::new();
	let sphere = PhyRawGeom::new(world.create_sphere(1.));
	sphere.set_position(0., 0., 0.);
	if BITS {
		sphere.set_category_bits(2);
		sphere.set_collide_bits(1);
	}
	for x in -10..10 {
		for y in -10..10 {
			for z in -10..10 {
				let geom = PhyRawGeom::new(OdeBox::new(Some(&space), DVec3::new(2., 2., 2.)));
				geom.set_position(x as _, y as _, z as _);
				if BITS {
					geom.set_category_bits(1);
					geom.set_collide_bits(0);
				}
				geoms.push(geom);
			}
		}
	}
	let mut cm = PhyCollisionManager::new();
	if SPACE_OMIT { cm.omit_space(&space); }
	for _i in 1..100 {
		world.tick(&mut cm);
		cm.process(&world);
	}
}
