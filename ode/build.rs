/*
 * SPDX-FileCopyrightText: 2025 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use bindgen::callbacks::{MacroParsingBehavior, ParseCallbacks};
use std::collections::HashSet;
use std::path::PathBuf;
use std::env;
use std::fs::copy;

// Source: https://github.com/rust-lang/rust-bindgen/issues/687#issuecomment-1312298570
const IGNORE_MACROS: [&str; 20] = [
	"FE_DIVBYZERO",
	"FE_DOWNWARD",
	"FE_INEXACT",
	"FE_INVALID",
	"FE_OVERFLOW",
	"FE_TONEAREST",
	"FE_TOWARDZERO",
	"FE_UNDERFLOW",
	"FE_UPWARD",
	"FP_INFINITE",
	"FP_INT_DOWNWARD",
	"FP_INT_TONEAREST",
	"FP_INT_TONEARESTFROMZERO",
	"FP_INT_TOWARDZERO",
	"FP_INT_UPWARD",
	"FP_NAN",
	"FP_NORMAL",
	"FP_SUBNORMAL",
	"FP_ZERO",
	"IPPORT_RESERVED",
];

#[derive(Debug)]
struct IgnoreMacros(HashSet<String>);

impl ParseCallbacks for IgnoreMacros {
	fn will_parse_macro(&self, name: &str) -> MacroParsingBehavior {
		if self.0.contains(name) {
			MacroParsingBehavior::Ignore
		} else {
			MacroParsingBehavior::Default
		}
	}
}

impl IgnoreMacros {
	fn new() -> Self {
		Self(IGNORE_MACROS.into_iter().map(|s| s.to_owned()).collect())
	}
}

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
	let dst = cmake::Config::new("ode-src")
		.define("ODE_WITH_DEMOS", "OFF")
		.define("ODE_WITH_TESTS", "OFF")
		.define("ODE_WIN32_LIB_OUTPUT_NAME_BASED_ON_FLOAT_SIZE", "OFF")
		.build();

	println!("cargo:rustc-link-search=native={}", dst.join("lib").display());

	let lib_name = if cfg!(all(debug_assertions, windows)) {
		"oded" // debug build
	} else {
		"ode"
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
		.header(dst.join("include/ode/ode.h").to_string_lossy())
		.parse_callbacks(Box::new(IgnoreMacros::new()))
		.allowlist_function("d.*")
		.clang_arg(format!("-I{}", dst.join("include").to_str().unwrap()))
		.layout_tests(false)
		.generate().unwrap();

	let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
	bindings.write_to_file(out_path.join("bindings.rs")).unwrap();
	println!("cargo:rerun-if-changed=ode-src");
}
