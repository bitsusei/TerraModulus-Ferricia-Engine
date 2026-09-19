/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use csgrs::mesh::Mesh;
use csgrs::traits::CSG;
use nalgebra_glm::DVec3;

pub(crate) struct CameraSpace {
	parallelepiped: Mesh<()>,
}

impl CameraSpace {
	const ANGLE: f64 = std::f64::consts::PI / 6.; // Rotation about x-axis

	/// Data about a parallelepiped, with an angle used in [`crate::mui::rendering3d`].
	pub(crate) fn new(center: DVec3, dims: DVec3) -> Self {
		let z_shift = dims.y / 2.0 * Self::ANGLE.tan();
		let x_min = center.x - dims.x / 2.0;
		let x_max = center.x + dims.x / 2.0;
		let y_min = center.y - dims.y / 2.0;
		let y_max = center.y + dims.y / 2.0;
		let z_lower_min = center.y - dims.z / 2.0 - z_shift;
		let z_upper_min = center.y - dims.z / 2.0 + z_shift;
		let z_lower_max = center.y + dims.z / 2.0 - z_shift;
		let z_upper_max = center.y + dims.z / 2.0 + z_shift;
		Self {
			parallelepiped: Mesh::polyhedron(&[
				[x_min, y_min, z_lower_min],
				[x_min, y_min, z_lower_max],
				[x_max, y_min, z_lower_max],
				[x_max, y_min, z_lower_min],
				[x_max, y_max, z_upper_min],
				[x_max, y_max, z_upper_max],
				[x_min, y_max, z_upper_max],
				[x_min, y_max, z_upper_min],
			], &[
				&[0, 1, 2, 3],
				&[2, 3, 4, 5],
				&[1, 2, 5, 6],
				&[0, 1, 6, 7],
				&[4, 5, 6, 7],
				&[0, 3, 4, 7],
			], None).unwrap(),
		}
	}

	pub(crate) fn intersects(&self, min: DVec3, max: DVec3) -> bool {
		// Maybe compare performance difference between this and a test with Separating Axis Theorem (SAT)
		let cuboid = Mesh::cuboid(max.x - min.x, max.y - min.y, max.z - min.z, None)
			.translate(min.x, min.y, min.z);
		!self.parallelepiped.intersection(&cuboid).polygons.is_empty()
	}
}
