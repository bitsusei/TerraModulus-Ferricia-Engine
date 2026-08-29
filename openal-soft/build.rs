/*
 * SPDX-FileCopyrightText: 2025 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use std::{env, fs};
use std::fs::copy;
use std::path::PathBuf;

/// Copied from https://github.com/maia-s/sdl3-sys-rs/blob/main/build-common.rs
fn top_level_cargo_target_dir() -> PathBuf {
	use std::path::PathBuf;
	let pkg_name = env::var("CARGO_PKG_NAME").unwrap();
	let out_dir = env::var_os("OUT_DIR").unwrap();
	let mut target = PathBuf::from(&out_dir);
	let pop = |target: &mut PathBuf| assert!(target.pop(), "malformed OUT_DIR: {:?}", out_dir);
	while !target
		.file_name()
		.unwrap()
		.to_string_lossy()
		.contains(&pkg_name)
	{
		pop(&mut target);
	}
	pop(&mut target);
	pop(&mut target);
	target
}

fn main() {
	let dst = cmake::Config::new("openal-soft-src")
		// .define("ODE_WITH_DEMOS", "OFF")
		.build();

	println!("cargo:rustc-link-search=native={}", dst.join("lib").display());

	let lib_name = if cfg!(target_os = "windows") {
		"OpenAL32"
	} else {
		"openal"
	};
	println!("cargo:rustc-link-lib={lib_name}");
	let mut src_file = dst.join(if cfg!(windows) {
		"bin"
	} else {
		"lib"
	}).join(format!("{}{lib_name}.{}", if cfg!(unix) {
		"lib"
	} else {
		""
	}, if cfg!(windows) {
		"dll"
	} else if cfg!(target_os = "macos") {
		"dylib"
	} else if cfg!(target_os = "linux") {
		"so"
	} else {
		unimplemented!("Unsupported OS");
	}));
	if cfg!(unix) {
		src_file = src_file.parent().unwrap().join(src_file.read_link().unwrap());
	};
	copy(&src_file, top_level_cargo_target_dir().join(src_file.file_name().unwrap())).unwrap();

	let bindings = bindgen::Builder::default()
		.headers(fs::read_dir(dst.join("include/AL").to_str().unwrap()).unwrap()
			.map(|v| v.unwrap().path().to_string_lossy().into_owned()))
		.clang_arg(format!("-I{}", dst.join("include").to_str().unwrap()))
		.generate().unwrap();

	let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
	bindings.write_to_file(out_path.join("bindings.rs")).unwrap();
	println!("cargo:rerun-if-changed=openal-soft-src");
}
