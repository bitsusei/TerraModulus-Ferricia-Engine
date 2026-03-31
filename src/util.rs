/*
 * SPDX-FileCopyrightText: 2025 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use std::ffi::{c_char, c_void, CStr, CString};
use std::mem::{forget, MaybeUninit};
use std::sync::atomic::{AtomicUsize, Ordering};
use sdl3::libc::{fclose, fopen, FILE};

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
pub fn str_to_c(str: impl AsRef<str>) -> CString {
	let str = str.as_ref();
	CString::new(str).expect("Cannot create CString")
}

pub struct CFile {
	ptr: *mut FILE,
}

impl CFile {
	pub fn new(path: CString) -> Self {
		let mode = CString::new("w").unwrap();
		let file = unsafe { fopen(path.as_ptr(), mode.as_ptr()) };
		assert!(!file.is_null(), "Cannot open file");
		CFile { ptr: file }
	}

	pub fn as_ptr(&self) -> *mut FILE {
		self.ptr
	}
}

impl Drop for CFile {
	fn drop(&mut self) {
		unsafe { fclose(self.ptr) };
	}
}

pub fn create_file_c(path: impl AsRef<str>) -> CFile {
	CFile::new(str_to_c(path))
}

/// Source: https://stackoverflow.com/a/72461302
pub fn concat_arrays<T, const M: usize, const N: usize>(a: [T; M], b: [T; N]) -> [T; M + N] {
	let mut result = MaybeUninit::uninit();
	let dest = result.as_mut_ptr() as *mut T;
	unsafe {
		std::ptr::copy_nonoverlapping(a.as_ptr(), dest, M);
		std::ptr::copy_nonoverlapping(b.as_ptr(), dest.add(M), N);
		forget(a);
		forget(b);
		result.assume_init()
	}
}
