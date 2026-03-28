/*
 * SPDX-FileCopyrightText: 2025 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use std::ffi::{c_char, CStr, CString};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Source: https://stackoverflow.com/a/72149089
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
#[repr(transparent)]
pub struct OpaqueId(usize);

impl OpaqueId {
	pub fn new(counter: &'static AtomicUsize) -> Self {
		Self(counter.fetch_add(1, Ordering::Relaxed))
	}

	pub fn id(&self) -> usize {
		self.0
	}
}

pub fn str_from_c(string: *const c_char) -> &'static str {
	unsafe { CStr::from_ptr(string).to_str().expect("should be valid utf8") }
}

/// This must not be dropped immediately for ptr access by `.as_str()`.
pub(super) fn str_to_c(str: impl AsRef<str>) -> CString {
	let str = str.as_ref();
	CString::new(str).expect("Cannot create CString")
}
