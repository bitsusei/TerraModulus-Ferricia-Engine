/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use nalgebra_glm::DVec3;
#[cfg(unix)]
use pprof::criterion::{Output, PProfProfiler};
use ferricia::phy::{OdeBox, OdePlaceableGeom, OdePlaceableMarker, OdeSpace, PhyCollisionManager, PhyEnv, PhyRawGeom, PhyWorld};

// Caveat: Drop order is important
struct Setup {
	geoms: Vec<PhyRawGeom<OdePlaceableMarker>>,
	cm: PhyCollisionManager,
	sphere: Option<PhyRawGeom<OdePlaceableMarker>>,
	space: Option<OdeSpace>,
	world: PhyWorld,
	env: PhyEnv,
}

impl Setup {
	fn new_simple() -> Self {
		let env = PhyEnv::new();
		let world = env.create_world();
		let geoms = Vec::new();
		let cm = PhyCollisionManager::new();
		Self { env, world, space: None, geoms, cm, sphere: None }
	}

	fn new_with_space(omit_space: bool) -> Self {
		let env = PhyEnv::new();
		let world = env.create_world();
		let space = world.create_space(1..=12);
		let geoms = Vec::new();
		let mut cm = PhyCollisionManager::new();
		if omit_space { cm.omit_space(&space); }
		Self { env, world, space: Some(space), geoms, cm, sphere: None }
	}

	fn gen_cubes(mut self, with_bits: bool) -> Self {
		for x in -10..10 {
			for y in -10..10 {
				for z in -10..10 {
					let geom = PhyRawGeom::new(match &self.space {
						None => self.world.create_box(DVec3::new(2., 2., 2.)),
						Some(space) => OdeBox::new(Some(space), DVec3::new(2., 2., 2.)),
					});
					geom.set_position(x as _, y as _, z as _);
					if with_bits {
						geom.set_category_bits(1);
						geom.set_collide_bits(0);
					}
					self.geoms.push(geom);
				}
			}
		}
		self
	}

	fn with_sphere(&mut self, with_bits: bool) {
		let sphere = PhyRawGeom::new(self.world.create_sphere(1.));
		sphere.set_position(0., 0., 0.);
		if with_bits {
			sphere.set_category_bits(2);
			sphere.set_collide_bits(1);
		}
		self.sphere = Some(sphere);
	}

	fn tick(&mut self) {
		self.world.tick(&mut self.cm);
		self.cm.process(&self.world);
	}
}

fn bench(c: &mut Criterion) {
	// BatchSize::PerIteration is used since PhyEnv can only exist once at a time.
	let mut group = c.benchmark_group("collision env-only");
	group.bench_function("unoptimized", |b| b.iter_batched(
		|| Setup::new_simple().gen_cubes(false),
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.bench_function("with space", |b| b.iter_batched(
		|| Setup::new_with_space(false).gen_cubes(false),
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.bench_function("with space omitted", |b| b.iter_batched(
		|| Setup::new_with_space(true).gen_cubes(false),
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.bench_function("with bits", |b| b.iter_batched(
		|| Setup::new_simple().gen_cubes(true),
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.bench_function("with space and bits", |b| b.iter_batched(
		|| Setup::new_with_space(false).gen_cubes(true),
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.bench_function("with space omitted and bits", |b| b.iter_batched(
		|| Setup::new_with_space(true).gen_cubes(true),
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.finish();
	let mut group = c.benchmark_group("collision with mc");
	group.bench_function("unoptimized", |b| b.iter_batched(
		|| {
			let mut setup = Setup::new_simple().gen_cubes(false);
			setup.with_sphere(false);
			setup
		},
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.bench_function("with space", |b| b.iter_batched(
		|| {
			let mut setup = Setup::new_with_space(false).gen_cubes(false);
			setup.with_sphere(false);
			setup
		},
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.bench_function("with space omitted", |b| b.iter_batched(
		|| {
			let mut setup = Setup::new_with_space(true).gen_cubes(false);
			setup.with_sphere(false);
			setup
		},
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.bench_function("with bits", |b| b.iter_batched(
		|| {
			let mut setup = Setup::new_simple().gen_cubes(true);
			setup.with_sphere(true);
			setup
		},
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.bench_function("with space and bits", |b| b.iter_batched(
		|| {
			let mut setup = Setup::new_with_space(false).gen_cubes(true);
			setup.with_sphere(true);
			setup
		},
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.bench_function("with space omitted and bits", |b| b.iter_batched(
		|| {
			let mut setup = Setup::new_with_space(true).gen_cubes(true);
			setup.with_sphere(true);
			setup
		},
		|mut setup| setup.tick(),
		BatchSize::PerIteration,
	));
	group.finish();
}

#[cfg(unix)]
criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = bench
}
#[cfg(not(unix))]
criterion_group!(benches, bench);
criterion_main!(benches);
