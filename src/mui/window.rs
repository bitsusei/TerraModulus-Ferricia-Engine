/*
 * SPDX-FileCopyrightText: 2025 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */
use crate::mui::ogl::GLHandle;
use crate::mui::SdlHandle;
use crate::{FerriciaError, FerriciaResult};
use gl::COLOR_BUFFER_BIT;
use sdl3::video::{SwapInterval, Window, WindowBuildError};
use std::ptr::null;
use std::rc::Rc;
use std::sync::Arc;
use getset::Getters;
use semver::Version;
use crate::mui::rendering3d::Camera3d;
use crate::mui::rendering::CanvasHandle;

impl From<WindowBuildError> for FerriciaError {
	fn from(value: WindowBuildError) -> Self {
		value.to_string().into()
	}
}

/// Handles top level functionalities of OpenGL
#[derive(Getters)]
pub(crate) struct WindowHandle {
	window: Window,
	/// Must be internally immutable upon initialization.
	#[get = "pub(super)"]
	gl_handle: Arc<GLHandle>,
}

const MIN_WIDTH: u32 = 800;
const MIN_HEIGHT: u32 = 480;

impl WindowHandle {
	pub(crate) fn new(sdl_handle: &SdlHandle) -> FerriciaResult<Self> {
		let mut window = sdl_handle.video.window("TerraModulus", MIN_WIDTH, MIN_HEIGHT)
			.opengl()
			.hidden()
			.position_centered()
			.resizable()
			.build()?;
		window.set_minimum_size(MIN_WIDTH, MIN_HEIGHT)?;
		let gl_context = window.gl_create_context()?;
		window.gl_make_current(&gl_context)?;
		gl::load_with(|s| sdl_handle.video.gl_get_proc_address(s).map_or(null::<fn()>(), |f| f as *const _) as *const _);
		let gl_handle = GLHandle::new(gl_context)?;
		gl_handle.gl_resize_viewport(MIN_WIDTH, MIN_HEIGHT);
		Ok(Self {
			gl_handle: Arc::new(gl_handle),
			window,
		})
	}

	pub(crate) fn show_window(&mut self) {
		self.window.show();
	}

	pub(crate) fn gl_resize_viewport(&self, canvas_handle: &mut CanvasHandle, camera: Option<&mut Camera3d>) {
		let (width, height) = self.window.size_in_pixels();
		self.gl_handle.gl_resize_viewport(width, height);
		canvas_handle.refresh_canvas_size(width, height, camera);
	}

	pub(super) fn window_size_in_pixels(&self) -> (u32, u32) {
		self.window.size_in_pixels()
	}

	pub(crate) fn swap_window(&self) {
		self.window.gl_swap_window();
	}

	fn set_icon(&self) {
		todo!()
	}

	fn window_id(&self) -> u32 {
		self.window.id()
	}

	pub(crate) fn full_gl_version(&self) -> &str {
		self.gl_handle.full_gl_version()
	}
}
