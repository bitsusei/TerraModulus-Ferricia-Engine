/*
 * SPDX-FileCopyrightText: 2025 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */

#![feature(fn_traits, ptr_metadata, downcast_unchecked, generic_const_exprs)]
#![feature(box_into_inner)]

#[cfg(feature = "client")]
mod mui;
mod util;
pub mod phy;

#[cfg(feature = "client")]
use crate::mui::{
	window::WindowHandle,
	rendering::{
		PrimModelTransform,
		ScalingCenteredTranslateParam,
		SmartScaling,
		DrawableSet,
		GeoProgram,
		SimpleLineGeom,
		TexProgram,
		clear_canvas,
		set_clear_color,
		AlphaFilter,
		PrimColorFilter,
		SpriteMesh,
		CanvasHandle,
		SimpleTranslation,
	},
	MuiEvent,
	SdlHandle,
};
use derive_more::From;
use jni::objects::{JClass, JDoubleArray, JFloatArray, JIntArray, JObject, JString, ReleaseMode};
use jni::sys::{jbyte, jdoubleArray, jfloat, jfloatArray, jint, jintArray, jlong, jlongArray, jobjectArray, jsize, jstring};
use jni::JNIEnv;
use paste::paste;
use sdl3::pixels::Color;
use std::backtrace::Backtrace;
use std::cell::Cell;
use std::env::set_var;
use std::fmt::Display;
use std::panic::{catch_unwind, take_hook, AssertUnwindSafe};
use std::ptr::{from_raw_parts, null};
use nalgebra_glm::DVec3;
use crate::mui::rendering::{FullScaling, SimpleRectGeom};
use crate::mui::rendering3d::Camera3d;
use crate::phy::{OdePlaceableGeom, PhyBody, PhyEnv, PhyGeom, PhyRawGeom, PhyRawGeomPlaceable, PhyWorld};

#[derive(From)]
struct FerriciaError(String);

impl FerriciaError {
	fn throw_jni(self, env: &mut JNIEnv) {
		handle_jni_error(env.throw_new("net/terramodulus/util/exception/FerriciaEngineFault", self.0), env);
	}
}

#[allow(unused_variables)]
fn handle_jni_error<E: Display>(result: Result<(), E>, env: &mut JNIEnv) {
	match result {
		Ok(_) => {},
		Err(err) => {
			#[cfg(debug_assertions)]
			panic!("{}", err);
			#[cfg(not(debug_assertions))]
			FerriciaError(err.to_string()).throw_jni(env);
		}
	}
}

type FerriciaResult<T> = Result<T, FerriciaError>;

macro_rules! resolve_res {
	($res:expr, $t: ty, $env:expr) => {
		match $res {
			Ok(v) => v,
			Err(err) => {
				err.throw_jni($env);
				return jni_null!($t)
			}
		}
	};
}

macro_rules! jni_null {
	($t: ty) => {
		null::<()>() as $t
	};
}

#[inline]
fn jni_ref_ptr<'a, T>(ptr: jlong) -> &'a mut T {
	unsafe { &mut *(ptr as *mut T) }
}

/// This may drop the owned value; use this with caution.
fn jni_from_ptr<T>(ptr: jlong) -> T {
	unsafe { *Box::from_raw(ptr as *mut T) }
}

fn jni_res_to_ptr<T>(result: FerriciaResult<T>, env: &mut JNIEnv) -> jlong {
	match result {
		Ok(v) => jni_to_ptr(v),
		Err(err) => {
			err.throw_jni(env);
			jni_null!(jlong)
		}
	}
}

fn jni_to_ptr<T>(val: T) -> jlong {
	Box::into_raw(Box::new(val)) as jlong
}

fn jni_to_wide_ptr<T: ?Sized>(val: &T) -> jlong {
	jni_to_ptr((val as *const T).to_raw_parts())
}

fn jni_ref_wide_ptr<'a, T: ?Sized>(ptr: jlong) -> &'a T {
	unsafe { &*from_raw_parts::<T>.call(*jni_ref_ptr::<(*const (), _)>(ptr)) }
}

macro_rules! jni_to_destructed_ptr {
	($val:expr, $tr:ty, $env:ident) => {
		let ptr = Box::into_raw(Box::new($val));
		let arr = $env.new_long_array(2).expect("Cannot create JLongArray");
		$env.set_long_array_region(&arr, 0, &[ptr as jlong, jni_to_wide_ptr(unsafe { &*ptr } as &$tr)])
			.expect("Cannot set Java array elements");
		return arr.into_raw()
	};
}

fn jni_drop_with_ptr<T>(ptr: jlong) {
	drop(unsafe { Box::from_raw(ptr as *mut T) })
}

thread_local! {
	static BACKTRACE: Cell<Option<Backtrace>> = const { Cell::new(None) };
}

macro_rules! run_catch {
	($func:block, $t: ty, $env:expr) => {
		match catch_unwind(AssertUnwindSafe(|| $func)) {
			Ok(v) => v,
			Err(err) => {
				let b = BACKTRACE.take().unwrap();
				if let Some(val) = err.downcast_ref::<String>() {
					FerriciaError(format!("{val:?}\n{b:?}")).throw_jni($env);
				} else {
					FerriciaError(format!("Unknown\n{b:?}")).throw_jni($env);
				}
				jni_null!($t)
			}
		}
	};
	($func:block, $env:expr) => {
		match catch_unwind(AssertUnwindSafe(|| $func)) {
			Ok(v) => v,
			Err(err) => {
				let b = BACKTRACE.take().unwrap();
				if let Some(val) = err.downcast_ref::<String>() {
					FerriciaError(format!("{val:?}\n{b:?}")).throw_jni($env);
				} else {
					FerriciaError(format!("Unknown\n{b:?}")).throw_jni($env);
				}
			}
		}
	};
}

fn jni_get_string(env: &mut JNIEnv, src: JString) -> String {
	env.get_string(&src).expect("Cannot get Java string").into()
}

macro_rules! jni_get_arr {
	($out:ident = $arr:ty; $var:ident, $env:ident) => {
		let $var = unsafe { <$arr>::from_raw($var) };
		let $out = unsafe {
			$env.get_array_elements(&$var, ReleaseMode::NoCopyBack)
				.expect("Cannot get Java array elements")
		};
	};
}

// #[allow(non_snake_case)]
// #[unsafe(no_mangle)]
// pub extern "system" fn Java_terramodulus_engine_ferricia_Demo_hello(
// 	mut env: JNIEnv,
// 	class: JClass,
// 	name: JString,
// ) -> jstring {
// 	let input: String =
// 		env.get_string(&name).expect("Couldn't get java string!").into();
// 	let output = env.new_string(format!("Hello, {}!", input))
// 		.expect("Couldn't create java string!");
// 	output.into_raw()
// }

// #[allow(non_snake_case)]
// #[unsafe(no_mangle)]
// pub extern "system" fn Java_terramodulus_engine_ferricia_Demo_clientOnly(
// 	mut env: JNIEnv,
// 	class: JClass,
// ) -> jint {
// 	0
// 	// unsafe { ode_sys::dInitODE2(0); }
// }

macro_rules! jni_ferricia {
	{ $class:ident.$function:ident( mut $env:ident: JNIEnv, $($params:tt)* ) $body:block } => {
		paste! {
			#[allow(unused_mut)]
			#[allow(unused_variables)]
			#[allow(non_snake_case)]
			#[allow(clippy::not_unsafe_ptr_arg_deref)]
			#[unsafe(no_mangle)]
			pub extern "system" fn [<Java_terramodulus_engine_ferricia_ $class _ $function>]
			(mut $env: JNIEnv, $($params)*) {
				run_catch!($body, &mut $env);
			}
		}
	};
	{ $class:ident.$function:ident( mut $env:ident: JNIEnv, $($params:tt)* ) -> $ret:ty $body:block } => {
		paste! {
			#[allow(unused_mut)]
			#[allow(unused_variables)]
			#[allow(non_snake_case)]
			#[allow(clippy::not_unsafe_ptr_arg_deref)]
			#[unsafe(no_mangle)]
			pub extern "system" fn [<Java_terramodulus_engine_ferricia_ $class _ $function>]
			(mut $env: JNIEnv, $($params)*) -> $ret {
				return run_catch!($body, $ret, &mut $env);
			}
		}
	};
	{ client:$class:ident.$function:ident( mut $env:ident: JNIEnv, $($params:tt)* ) $body:block } => {
		paste! {
			#[allow(unused_mut)]
			#[allow(unused_variables)]
			#[allow(non_snake_case)]
			#[allow(clippy::not_unsafe_ptr_arg_deref)]
			#[unsafe(no_mangle)]
			#[cfg(feature = "client")]
			pub extern "system" fn [<Java_terramodulus_engine_ferricia_ $class _ $function>]
			(mut $env: JNIEnv, $($params)*) {
				run_catch!($body, &mut $env);
			}
		}
	};
	{ client:$class:ident.$function:ident( mut $env:ident: JNIEnv, $($params:tt)* ) -> $ret:ty $body:block } => {
		paste! {
			#[allow(unused_mut)]
			#[allow(unused_variables)]
			#[allow(non_snake_case)]
			#[allow(clippy::not_unsafe_ptr_arg_deref)]
			#[unsafe(no_mangle)]
			#[cfg(feature = "client")]
			pub extern "system" fn [<Java_terramodulus_engine_ferricia_ $class _ $function>]
			(mut $env: JNIEnv, $($params)*) -> $ret {
				return run_catch!($body, $ret, &mut $env);
			}
		}
	};
	{ server:$class:ident.$function:ident( mut $env:ident: JNIEnv, $($params:tt)* ) $body:block } => {
		paste! {
			#[allow(unused_mut)]
			#[allow(unused_variables)]
			#[allow(non_snake_case)]
			#[allow(clippy::not_unsafe_ptr_arg_deref)]
			#[unsafe(no_mangle)]
			#[cfg(feature = "server")]
			pub extern "system" fn [<Java_terramodulus_engine_ferricia_ $class _ $function>]
			(mut $env: JNIEnv, $($params)*) {
				run_catch!($body, &mut $env);
			}
		}
	};
	{ server:$class:ident.$function:ident( mut $env:ident: JNIEnv, $($params:tt)* ) -> $ret:ty $body:block } => {
		paste! {
			#[allow(unused_mut)]
			#[allow(unused_variables)]
			#[allow(non_snake_case)]
			#[allow(clippy::not_unsafe_ptr_arg_deref)]
			#[unsafe(no_mangle)]
			#[cfg(feature = "server")]
			pub extern "system" fn [<Java_terramodulus_engine_ferricia_ $class _ $function>]
			(mut $env: JNIEnv, $($params)*) -> $ret {
				return run_catch!($body, $ret, &mut $env);
			}
		}
	};
}

jni_ferricia! {
	Core.init(mut env: JNIEnv, class: JClass) {
		// Source: https://stackoverflow.com/a/73711057
		let orig_hook = take_hook();
		std::panic::set_hook(Box::new(move |panic_info| {
			BACKTRACE.set(Some(Backtrace::force_capture()));
			orig_hook(panic_info);
		}));
		#[cfg(debug_assertions)]
		unsafe { set_var("RUST_BACKTRACE", "full"); }
	}
}

jni_ferricia! {
	client:Mui.initSdlHandle(mut env: JNIEnv, class: JClass) -> jlong {
		jni_res_to_ptr(SdlHandle::new(), &mut env) as jlong
	}
}

jni_ferricia! {
	client:Mui.dropSdlHandle(mut env: JNIEnv, class: JClass, handle: jlong) {
		jni_drop_with_ptr::<SdlHandle>(handle);
	}
}

jni_ferricia! {
	client:Mui.initWindowHandle(mut env: JNIEnv, class: JClass, handle: jlong) -> jlong {
		jni_res_to_ptr(WindowHandle::new(jni_ref_ptr(handle)), &mut env)
	}
}

jni_ferricia! {
	client:Mui.dropWindowHandle(mut env: JNIEnv, class: JClass, handle: jlong) {
		jni_drop_with_ptr::<WindowHandle>(handle);
	}
}

jni_ferricia! {
	client:Mui.getGLVersion(mut env: JNIEnv, class: JClass, handle: jlong) -> jstring {
		env.new_string(jni_ref_ptr::<WindowHandle>(handle).full_gl_version())
			.expect("Cannot create Java string")
			.into_raw()
	}
}

jni_ferricia! {
	client:Mui.sdlPoll(mut env: JNIEnv, class: JClass, handle: jlong) -> jobjectArray {
		let v = jni_ref_ptr::<SdlHandle>(handle).poll();
		let a = env.new_object_array(v.len() as jsize, "net/terramodulus/engine/MuiEvent", JObject::null())
			.expect("Cannot create Java array");
		v.into_iter().enumerate().for_each(|(i, e)| {
			let v = match e {
				MuiEvent::DisplayAdded(handle) => {
					let p = vec!(jni_to_ptr(handle).into());
					env.new_object("net/terramodulus/engine/MuiEvent$DisplayAdded", "(J)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::DisplayRemoved(handle) => {
					let p = vec!(jni_to_ptr(handle).into());
					env.new_object("net/terramodulus/engine/MuiEvent$DisplayRemoved", "(J)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::DisplayMoved(handle) => {
					let p = vec!(jni_to_ptr(handle).into());
					env.new_object("net/terramodulus/engine/MuiEvent$DisplayMoved", "(J)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::WindowShown => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowShown";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowHidden => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowHidden";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowExposed => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowExposed";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowMoved(x, y) => {
					let p = vec!(x.into(), y.into());
					env.new_object("net/terramodulus/engine/MuiEvent$WindowMoved", "(II)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::WindowResized(w, h) => {
					let p = vec!(w.into(), h.into());
					env.new_object("net/terramodulus/engine/MuiEvent$WindowResized", "(II)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::WindowPixelSizeChanged(w, h) => {
					let p = vec!(w.into(), h.into());
					env.new_object("net/terramodulus/engine/MuiEvent$WindowPixelSizeChanged", "(II)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::WindowMetalViewResized => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowMetalViewResized";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowMinimized => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowMinimized";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowMaximized => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowMaximized";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowRestored => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowRestored";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowMouseEnter => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowMouseEnter";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowMouseLeave => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowMouseLeave";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowFocusGained => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowFocusGained";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowFocusLost => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowFocusLost";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowCloseRequested => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowCloseRequested";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowIccProfChanged => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowIccProfChanged";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowOccluded => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowOccluded";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowEnterFullscreen => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowEnterFullscreen";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowLeaveFullscreen => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowLeaveFullscreen";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowDestroyed => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowDestroyed";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::WindowHdrStateChanged => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$WindowHdrStateChanged";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::KeyboardKeyDown(id, k) => {
					let p = vec!((id as jint).into(), (k as u32 as jint).into());
					env.new_object("net/terramodulus/engine/MuiEvent$KeyboardKeyDown", "(II)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::KeyboardKeyUp(id, k) => {
					let p = vec!((id as jint).into(), (k as u32 as jint).into());
					env.new_object("net/terramodulus/engine/MuiEvent$KeyboardKeyUp", "(II)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::TextEditing(t, s, l) => {
					let ss = env.new_string(t).expect("Cannot create Java string");
					let p = vec!((&ss).into(), s.into(), l.into());
					env.new_object("net/terramodulus/engine/MuiEvent$TextEditing", "(Ljava/lang/String;II)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::TextInput(t) => {
					let ss = env.new_string(t).expect("Cannot create Java string");
					let p = vec!((&ss).into());
					env.new_object("net/terramodulus/engine/MuiEvent$TextInput", "(Ljava/lang/String;)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::KeymapChanged => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$KeymapChanged";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::KeyboardAdded => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$KeyboardAdded";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::KeyboardRemoved => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$KeyboardRemoved";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::TextEditingCandidates => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$TextEditingCandidates";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::MouseMotion(id, x, y) => {
					let p = vec!((id as jint).into(), x.into(), y.into());
					env.new_object("net/terramodulus/engine/MuiEvent$MouseMotion", "(IFF)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::MouseButtonDown(id, k) => {
					let p = vec!((id as jint).into(), (k as u8 as jbyte).into());
					env.new_object("net/terramodulus/engine/MuiEvent$MouseButtonDown", "(IB)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::MouseButtonUp(id, k) => {
					let p = vec!((id as jint).into(), (k as u8 as jbyte).into());
					env.new_object("net/terramodulus/engine/MuiEvent$MouseButtonUp", "(IB)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::MouseWheel(id, x, y) => {
					let p = vec!((id as jint).into(), x.into(), y.into());
					env.new_object("net/terramodulus/engine/MuiEvent$MouseWheel", "(IFF)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::MouseAdded => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$MouseAdded";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::MouseRemoved => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$MouseRemoved";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::JoystickAxisMotion(id, a , v) => {
					let p = vec!((id as jint).into(), (a as jbyte).into(), v.into());
					env.new_object("net/terramodulus/engine/MuiEvent$JoystickAxisMotion", "(IBS)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::JoystickBallMotion => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$JoystickBallMotion";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::JoystickHatMotion(id, h , s) => {
					let p = vec!((id as jint).into(), (h as jbyte).into(), (s as u8 as jbyte).into());
					env.new_object("net/terramodulus/engine/MuiEvent$JoystickHatMotion", "(IBB)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::JoystickButtonDown(id, b) => {
					let p = vec!((id as jint).into(), (b as jbyte).into());
					env.new_object("net/terramodulus/engine/MuiEvent$JoystickButtonDown", "(IB)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::JoystickButtonUp(id, b) => {
					let p = vec!((id as jint).into(), (b as jbyte).into());
					env.new_object("net/terramodulus/engine/MuiEvent$JoystickButtonUp", "(IB)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::JoystickAdded(id) => {
					let p = vec!((id as jint).into());
					env.new_object("net/terramodulus/engine/MuiEvent$JoystickAdded", "(I)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::JoystickRemoved(id) => {
					let p = vec!((id as jint).into());
					env.new_object("net/terramodulus/engine/MuiEvent$JoystickRemoved", "(I)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::JoystickBatteryUpdated => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$JoystickBatteryUpdated";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::GamepadAxisMotion(id, a , v) => {
					let p = vec!((id as jint).into(), (a as u8 as jbyte).into(), v.into());
					env.new_object("net/terramodulus/engine/MuiEvent$GamepadAxisMotion", "(IBS)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::GamepadButtonDown(id, b) => {
					let p = vec!((id as jint).into(), (b as jbyte).into());
					env.new_object("net/terramodulus/engine/MuiEvent$GamepadButtonDown", "(IB)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::GamepadButtonUp(id, b) => {
					let p = vec!((id as jint).into(), (b as jbyte).into());
					env.new_object("net/terramodulus/engine/MuiEvent$GamepadButtonUp", "(IB)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::GamepadAdded(id) => {
					let p = vec!((id as jint).into());
					env.new_object("net/terramodulus/engine/MuiEvent$GamepadAdded", "(I)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::GamepadRemoved(id) => {
					let p = vec!((id as jint).into());
					env.new_object("net/terramodulus/engine/MuiEvent$GamepadRemoved", "(I)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::GamepadRemapped(id) => {
					let p = vec!((id as jint).into());
					env.new_object("net/terramodulus/engine/MuiEvent$GamepadRemapped", "(I)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::GamepadTouchpadDown(id, t, f, x, y, p) => {
					let p = vec!((id as jint).into(), t.into(), f.into(), x.into(), y.into(), p.into());
					env.new_object("net/terramodulus/engine/MuiEvent$GamepadTouchpadDown", "(IIIFFF)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::GamepadTouchpadMotion(id, t, f, x, y, p) => {
					let p = vec!((id as jint).into(), t.into(), f.into(), x.into(), y.into(), p.into());
					env.new_object("net/terramodulus/engine/MuiEvent$GamepadTouchpadMotion", "(IIIFFF)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::GamepadTouchpadUp(id, t, f, x, y, p) => {
					let p = vec!((id as jint).into(), t.into(), f.into(), x.into(), y.into(), p.into());
					env.new_object("net/terramodulus/engine/MuiEvent$GamepadTouchpadUp", "(IIIFFF)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::GamepadSteamHandleUpdated => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$GamepadSteamHandleUpdated";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::DropFile(f) => {
					let ss = env.new_string(f).expect("Cannot create Java string");
					let p = vec!((&ss).into());
					env.new_object("net/terramodulus/engine/MuiEvent$DropFile", "(Ljava/lang/String;)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::DropText(t) => {
					let ss = env.new_string(t).expect("Cannot create Java string");
					let p = vec!((&ss).into());
					env.new_object("net/terramodulus/engine/MuiEvent$DropText", "(Ljava/lang/String;)V", p.as_slice())
						.expect("Cannot create Java object")
				}
				MuiEvent::DropBegin => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$DropBegin";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::DropComplete => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$DropComplete";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::DropPosition => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$DropPosition";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::RenderTargetsReset => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$RenderTargetsReset";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::RenderDeviceReset => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$RenderDeviceReset";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
				MuiEvent::RenderDeviceLost => {
					const CLASS: &str = "net/terramodulus/engine/MuiEvent$RenderDeviceLost";
					env.get_static_field(CLASS, "INSTANCE", format!("L{CLASS};"))
						.expect("Cannot get static field")
						.l()
						.expect("JObject is expected")
				}
			};
			env.set_object_array_element(&a, i as jsize, v).expect("Cannot set Java object array");
		});
		a.into_raw()
	}
}

jni_ferricia! {
	client:Mui.resizeGLViewport(mut env: JNIEnv, class: JClass, handle: jlong, canvas_handle: jlong) {
		jni_ref_ptr::<WindowHandle>(handle).gl_resize_viewport(jni_ref_ptr::<CanvasHandle>(canvas_handle), None);
	}
}

jni_ferricia! {
	client:Mui.resizeGLViewportCamera(mut env: JNIEnv, class: JClass, handle: jlong, canvas_handle: jlong, camera_handle: jlong) {
		jni_ref_ptr::<WindowHandle>(handle).gl_resize_viewport(jni_ref_ptr::<CanvasHandle>(canvas_handle), Some(jni_ref_ptr::<Camera3d>(camera_handle)));
	}
}

jni_ferricia! {
	client:Mui.showWindow(mut env: JNIEnv, class: JClass, handle: jlong) {
		jni_ref_ptr::<WindowHandle>(handle).show_window()
	}
}

jni_ferricia! {
	client:Mui.swapWindow(mut env: JNIEnv, class: JClass, handle: jlong) {
		jni_ref_ptr::<WindowHandle>(handle).swap_window()
	}
}

jni_ferricia! {
	client:Mui.initCanvasHandle(mut env: JNIEnv, class: JClass, handle: jlong) -> jlong {
		jni_to_ptr(CanvasHandle::new(jni_ref_ptr::<WindowHandle>(handle)))
	}
}

jni_ferricia! {
	client:Mui.dropCanvasHandle(mut env: JNIEnv, class: JClass, handle: jlong) {
		jni_drop_with_ptr::<CanvasHandle>(handle);
	}
}

jni_ferricia! {
	client:Mui.loadImageToCanvas(mut env: JNIEnv, class: JClass, handle: jlong, path: JString) -> jint {
		jni_ref_ptr::<CanvasHandle>(handle).load_image(env.get_string(&path)
			.expect("Cannot get Java string").into()) as jint
	}
}

jni_ferricia! {
	client:Mui.clearCanvas(mut env: JNIEnv, class: JClass) {
		clear_canvas()
	}
}

jni_ferricia! {
	client:Mui.setCanvasClearColor(mut env: JNIEnv, class: JClass, r: jfloat, g: jfloat, b: jfloat, a: jfloat) {
		set_clear_color((r, g, b, a));
	}
}

jni_ferricia! {
	client:Mui.geoShaders(mut env: JNIEnv, class: JClass, vsh: JString, fsh: JString) -> jlong {
		jni_res_to_ptr(GeoProgram::new(jni_get_string(&mut env, vsh), jni_get_string(&mut env, fsh)), &mut env)
	}
}

jni_ferricia! {
	client:Mui.texShaders(mut env: JNIEnv, class: JClass, vsh: JString, fsh: JString) -> jlong {
		jni_res_to_ptr(TexProgram::new(jni_get_string(&mut env, vsh), jni_get_string(&mut env, fsh)), &mut env)
	}
}

jni_ferricia! {
	client:Mui.newSimpleLineGeom(mut env: JNIEnv, class: JClass, data: jintArray) -> jlong {
		jni_get_arr!(arr = JIntArray; data, env);
		jni_to_ptr(DrawableSet::new(SimpleLineGeom::new(
			[(arr[0] as f32, arr[1] as f32), (arr[2] as f32, arr[3] as f32)],
			Color::RGBA(arr[4] as u8, arr[5] as u8, arr[6] as u8, arr[7] as u8),
		)))
	}
}

jni_ferricia! {
	client:Mui.newSimpleRectGeom(mut env: JNIEnv, class: JClass, data: jintArray) -> jlong {
		jni_get_arr!(arr = JIntArray; data, env);
		jni_to_ptr(DrawableSet::new(SimpleRectGeom::new(
			[arr[0] as f32, arr[1] as f32, arr[2] as f32, arr[3] as f32],
			Color::RGBA(arr[4] as u8, arr[5] as u8, arr[6] as u8, arr[7] as u8),
		)))
	}
}

jni_ferricia! {
	client:Mui.newSpriteMesh(mut env: JNIEnv, class: JClass, data: jintArray) -> jlong {
		jni_get_arr!(arr = JIntArray; data, env);
		jni_to_ptr(DrawableSet::new(SpriteMesh::new([arr[0] as _, arr[1] as _, arr[2] as _, arr[3] as _])))
	}
}

jni_ferricia! {
	client:Mui.modelSmartScaling(mut env: JNIEnv, class: JClass, data: jintArray) -> jlongArray {
		jni_get_arr!(arr = JIntArray; data, env);
		jni_to_destructed_ptr!(SmartScaling::new((arr[0] as _, arr[1] as _), match arr[2] {
			0 => None,
			1 => Some((ScalingCenteredTranslateParam::X, (arr[3] as _, arr[4] as _))),
			2 => Some((ScalingCenteredTranslateParam::Y, (arr[3] as _, arr[4] as _))),
			3 => Some((ScalingCenteredTranslateParam::Both, (arr[3] as _, arr[4] as _))),
			_ => panic!("Invalid Smart Scaling parameter"),
		}), dyn PrimModelTransform, env);
	}
}

jni_ferricia! {
	client:Mui.modelFullScaling(mut env: JNIEnv, class: JClass, data: jintArray) -> jlongArray {
		jni_get_arr!(arr = JIntArray; data, env);
		jni_to_destructed_ptr!(FullScaling::new((arr[0] as _, arr[1] as _)), dyn PrimModelTransform, env);
	}
}

jni_ferricia! {
	client:Mui.modelSimpleTranslation(mut env: JNIEnv, class: JClass, data: jfloatArray) -> jlongArray {
		jni_get_arr!(arr = JFloatArray; data, env);
		jni_to_destructed_ptr!(SimpleTranslation::new(arr[0], arr[1]), dyn PrimModelTransform, env);
	}
}

jni_ferricia! {
	client:Mui.filterAlphaFilter(mut env: JNIEnv, class: JClass, data: jfloat) -> jlongArray {
		jni_to_destructed_ptr!(AlphaFilter::new(data), dyn PrimColorFilter, env);
	}
}

jni_ferricia! {
	client:Mui.editAlphaFilter(mut env: JNIEnv, class: JClass, filter: jlong, data: jfloat) {
		jni_ref_ptr::<AlphaFilter>(filter).set_alpha(data as _);
	}
}

jni_ferricia! {
	client:Mui.addModelTransform(mut env: JNIEnv, class: JClass, set_handle: jlong, model_handle: jlong) {
		jni_ref_ptr::<DrawableSet>(set_handle).add_model_transform(jni_ref_wide_ptr(model_handle))
	}
}

jni_ferricia! {
	client:Mui.removeModelTransform(mut env: JNIEnv, class: JClass, set_handle: jlong, model_handle: jlong) {
		jni_ref_ptr::<DrawableSet>(set_handle).remove_model_transform(jni_ref_wide_ptr(model_handle))
	}
}

jni_ferricia! {
	client:Mui.addColorFilter(mut env: JNIEnv, class: JClass, set_handle: jlong, filter_handle: jlong) {
		jni_ref_ptr::<DrawableSet>(set_handle).add_filter_transform(jni_ref_wide_ptr(filter_handle))
	}
}

jni_ferricia! {
	client:Mui.removeColorFilter(mut env: JNIEnv, class: JClass, set_handle: jlong, filter_handle: jlong) {
		jni_ref_ptr::<DrawableSet>(set_handle).remove_filter_transform(jni_ref_wide_ptr(filter_handle))
	}
}

jni_ferricia! {
	client:Mui.drawGuiGeo(
		mut env: JNIEnv,
		class: JClass,
		canvas_handle: jlong,
		drawable_handle: jlong,
		program_handle: jlong,
	) {
		jni_ref_ptr::<CanvasHandle>(canvas_handle)
			.draw_gui(jni_ref_ptr::<DrawableSet>(drawable_handle), jni_ref_ptr::<GeoProgram>(program_handle), None)
	}
}

jni_ferricia! {
	client:Mui.drawGuiTex(
		mut env: JNIEnv,
		class: JClass,
		canvas_handle: jlong,
		drawable_handle: jlong,
		program_handle: jlong,
		texture_handle: jint,
	) {
		jni_ref_ptr::<CanvasHandle>(canvas_handle).draw_gui(
			jni_ref_ptr::<DrawableSet>(drawable_handle),
			jni_ref_ptr::<TexProgram>(program_handle),
			Some(texture_handle as _),
		)
	}
}

jni_ferricia! {
	Physics.newPhyEnv(mut env: JNIEnv, class: JClass) -> jlong {
		jni_to_ptr(PhyEnv::new())
	}
}

jni_ferricia! {
	Physics.dropPhyEnv(mut env: JNIEnv, class: JClass, handle: jlong) {
		jni_drop_with_ptr::<PhyEnv>(handle)
	}
}

jni_ferricia! {
	Physics.newPhyWorld(mut env: JNIEnv, class: JClass, handle: jlong) -> jlong {
		jni_to_ptr(jni_ref_ptr::<PhyEnv>(handle).create_world())
	}
}

jni_ferricia! {
	Physics.newWorldPhyGeomBox(mut env: JNIEnv, class: JClass, handle: jlong, lengths: jdoubleArray) -> jlong {
		jni_get_arr!(arr = JDoubleArray; lengths, env);
		jni_to_ptr(PhyRawGeom::new(
			jni_ref_ptr::<PhyWorld>(handle).space().create_box(DVec3::new(arr[0] as _, arr[1] as _, arr[2] as _))
		))
	}
}

jni_ferricia! {
	Physics.setPhyRawGeomPlaceablePosition(mut env: JNIEnv, class: JClass, handle: jlong, pos: jdoubleArray) {
		jni_get_arr!(arr = JDoubleArray; pos, env);
		jni_ref_ptr::<PhyRawGeomPlaceable>(handle).set_position(arr[0] as _, arr[1] as _, arr[2] as _);
	}
}

jni_ferricia! {
	Physics.getPhyRawGeomPlaceablePosition(mut env: JNIEnv, class: JClass, handle: jlong) -> jdoubleArray {
		let r = jni_ref_ptr::<PhyRawGeomPlaceable>(handle).get_position();
		let arr = env.new_double_array(3).expect("Cannot create Java double array");
		env.set_double_array_region(&arr, 0, r).expect("Cannot set Java double array");
		arr.into_raw()
	}
}
