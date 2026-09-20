/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use bytemuck::cast_slice;
use csgrs::float_types::parry3d::na::Point3;
use csgrs::mesh::Mesh;
use csgrs::traits::CSG;
use itertools::Itertools;
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

	/// Intersection test by using Separating Axis Theorem (SAT).
	pub(crate) fn intersects(&self, min: DVec3, max: DVec3) -> bool {
		let cuboid: Mesh<()> = Mesh::cuboid(max.x - min.x, max.y - min.y, max.z - min.z, None)
			.translate(min.x, min.y, min.z);
		let mut axes = Vec::<DVec3>::new();
		axes.extend(self.parallelepiped.polygons
			.iter()
			.map(|f| f.plane.normal())
			.unique_by(|v| cast_slice::<_, u8>(v.as_slice()).to_vec()));
		axes.extend(cuboid.polygons
			.iter()
			.map(|f| f.plane.normal())
			.unique_by(|v| cast_slice::<_, u8>(v.as_slice()).to_vec()));
		for ea in self.parallelepiped.polygons
			.iter()
			.flat_map(|f| f.edges().map(|e| (e.1.pos - e.0.pos).normalize()))
			.unique_by(|v| cast_slice::<_, u8>(v.as_slice()).to_vec()) {
			for eb in cuboid.polygons
				.iter()
				.flat_map(|f| f.edges().map(|e| (e.1.pos - e.0.pos).normalize()))
				.unique_by(|v| cast_slice::<_, u8>(v.as_slice()).to_vec()) {
				let axis = ea.cross(&eb);
				if axis.norm() > f64::EPSILON {
					axes.push(axis.normalize());
				}
			}
		}
		fn range_projection(vertices: &[Point3<f64>], axis: DVec3) -> (f64, f64) {
			let mut min_proj = None;
			let mut max_proj = None;
			for vertex in vertices {
				let proj = vertex.coords.dot(&axis);
				match min_proj {
					None => min_proj = Some(proj),
					Some(min) if proj < min => min_proj = Some(proj),
					_ => {}
				}
				match max_proj {
					None => max_proj = Some(proj),
					Some(max) if proj > max => max_proj = Some(proj),
					_ => {}
				}
			}
			(min_proj.unwrap(), max_proj.unwrap())
		}
		let a_vertices = self.parallelepiped.vertices().iter().map(|v| v.pos).collect::<Vec<_>>();
		let b_vertices = cuboid.vertices().iter().map(|v| v.pos).collect::<Vec<_>>();
		for axis in axes {
			let a = range_projection(&a_vertices, axis);
			let b = range_projection(&b_vertices, axis);
			if a.0 > b.1 || a.1 < b.0 {
				return false; // gap exists thus no intersection
			}
		}
		true // gap not found this intersection
	}
}
