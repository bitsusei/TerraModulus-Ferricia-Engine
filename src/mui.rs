/*
 * SPDX-FileCopyrightText: 2025 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */

//! MUI - Multimodal User Interface

use crate::{FerriciaError, FerriciaResult};
use sdl3::event::{DisplayEvent, Event, WindowEvent};
use sdl3::keyboard::Scancode;
use sdl3::mouse::MouseButton;
use sdl3::video::{Display, DisplayMode};
use sdl3::{AudioSubsystem, EventPump, EventSubsystem, GamepadSubsystem, HapticSubsystem, JoystickSubsystem, Sdl, VideoSubsystem};
use std::cell::RefCell;
use std::collections::HashMap;
use sdl3::properties::PropertiesError;
use sdl3::rect::Rect;

pub use sdl3::gamepad::Axis as GamepadAxis;
pub use sdl3::gamepad::Button as GamepadButton;
pub use sdl3::joystick::HatState as JoystickHatState;

pub(crate) mod rendering;
pub(crate) mod window;
mod audio;
mod oal;
mod ogl;
pub(crate) mod rendering3d;

pub(crate) struct SdlHandle {
	events: EventSubsystem,
	joystick: JoystickSubsystem,
	haptic: HapticSubsystem,
	gamepad: GamepadSubsystem,
	video: VideoSubsystem,
	event_pump: EventPump,
	sdl_context: Sdl,
	// This is made because Display ID is opaque from sdl3-rs.
	displays: RefCell<HashMap<Display, SdlDisplay>>,
}

impl From<sdl3::Error> for FerriciaError {
	fn from(value: sdl3::Error) -> Self {
		value.to_string().into()
	}
}

impl From<sdl3::IntegerOrSdlError> for FerriciaError {
	fn from(value: sdl3::IntegerOrSdlError) -> Self {
		value.to_string().into()
	}
}

impl SdlHandle {
	pub(crate) fn new() -> FerriciaResult<SdlHandle> {
		let sdl_context = sdl3::init()?;
		let video = sdl_context.video()?;
		let mut displays = HashMap::new();
		video.displays()?.into_iter().for_each(|d| {
			if let Ok(v) = SdlDisplay::new(&d) {
				displays.insert(d, v);
			}
		});
		Ok(Self {
			events: sdl_context.event()?,
			joystick: sdl_context.joystick()?,
			haptic: sdl_context.haptic()?,
			gamepad: sdl_context.gamepad()?,
			video,
			event_pump: sdl_context.event_pump()?,
			sdl_context,
			displays: RefCell::new(displays),
		})
	}

	pub(crate) fn poll(&mut self) -> Vec<MuiEvent> {
		self.event_pump.pump_events();
		let mut events = Vec::new();
		self.event_pump.poll_iter().for_each(|event| {
			if let Some(v) = match event {
				// Only one window is available, so the window ID is ignored.
				// SDL only reports events made through the window created by this application.
				Event::Window { win_event, .. } => match win_event {
					WindowEvent::Shown => Some(MuiEvent::WindowShown),
					WindowEvent::Hidden => Some(MuiEvent::WindowHidden),
					WindowEvent::Exposed => Some(MuiEvent::WindowExposed),
					WindowEvent::Moved(x, y) => Some(MuiEvent::WindowMoved(x, y)),
					WindowEvent::Resized(w, h) => Some(MuiEvent::WindowResized(w, h)),
					WindowEvent::PixelSizeChanged(w, h) => Some(MuiEvent::WindowPixelSizeChanged(w, h)),
					WindowEvent::Minimized => Some(MuiEvent::WindowMinimized),
					WindowEvent::Maximized => Some(MuiEvent::WindowMaximized),
					WindowEvent::Restored => Some(MuiEvent::WindowRestored),
					WindowEvent::MouseEnter => Some(MuiEvent::WindowMouseEnter),
					WindowEvent::MouseLeave => Some(MuiEvent::WindowMouseLeave),
					WindowEvent::FocusGained => Some(MuiEvent::WindowFocusGained),
					WindowEvent::FocusLost => Some(MuiEvent::WindowFocusLost),
					WindowEvent::CloseRequested => Some(MuiEvent::WindowCloseRequested),
					WindowEvent::ICCProfChanged => Some(MuiEvent::WindowIccProfChanged),
					_ => None,
				}
				Event::KeyDown { scancode, repeat, which, .. } =>
					scancode.filter(|v| !repeat || v != &Scancode::Unknown).and_then(KeyboardKey::from_sdl)
						.map(|v| MuiEvent::KeyboardKeyDown(which, v)),
				Event::KeyUp { scancode, repeat, which, .. } =>
					scancode.filter(|v| !repeat || v != &Scancode::Unknown).and_then(KeyboardKey::from_sdl)
						.map(|v| MuiEvent::KeyboardKeyUp(which, v)),
				Event::TextEditing { text, start, length, .. } => Some(MuiEvent::TextEditing(text, start, length)),
				Event::TextInput { text, .. } => Some(MuiEvent::TextInput(text)),
				Event::MouseMotion { which, xrel, yrel, .. } => Some(MuiEvent::MouseMotion(which, xrel, yrel)),
				Event::MouseButtonDown { which, mouse_btn, .. } =>
					MouseKey::from_sdl(mouse_btn).map(|v| MuiEvent::MouseButtonDown(which, v)),
				Event::MouseButtonUp { which, mouse_btn, .. } =>
					MouseKey::from_sdl(mouse_btn).map(|v| MuiEvent::MouseButtonUp(which, v)),
				Event::MouseWheel { which, x, y, .. } => Some(MuiEvent::MouseWheel(which, x, -y)),
				Event::JoyAxisMotion { which, axis_idx, value, .. } =>
					Some(MuiEvent::JoystickAxisMotion(which, axis_idx, value)),
				Event::JoyHatMotion { which, hat_idx, state, .. } =>
					Some(MuiEvent::JoystickHatMotion(which, hat_idx, state)),
				Event::JoyButtonDown { which, button_idx, .. } =>
					Some(MuiEvent::JoystickButtonDown(which, button_idx)),
				Event::JoyButtonUp { which, button_idx, .. } =>
					Some(MuiEvent::JoystickButtonUp(which, button_idx)),
				Event::JoyDeviceAdded { which, .. } => Some(MuiEvent::JoystickAdded(which)),
				Event::JoyDeviceRemoved { which, .. } => Some(MuiEvent::JoystickRemoved(which)),
				Event::ControllerAxisMotion { which, axis, value, .. } =>
					Some(MuiEvent::GamepadAxisMotion(which, axis, value)),
				Event::ControllerButtonDown { which, button, .. } =>
					Some(MuiEvent::GamepadButtonDown(which, button)),
				Event::ControllerButtonUp { which, button, .. } =>
					Some(MuiEvent::GamepadButtonUp(which, button)),
				Event::ControllerDeviceAdded { which, .. } => Some(MuiEvent::GamepadAdded(which)),
				Event::ControllerDeviceRemoved { which, .. } => Some(MuiEvent::GamepadRemoved(which)),
				Event::ControllerDeviceRemapped { which, .. } => Some(MuiEvent::GamepadRemapped(which)),
				Event::ControllerTouchpadDown { which, touchpad, finger, x, y, pressure, .. } =>
					Some(MuiEvent::GamepadTouchpadDown(which, touchpad, finger, x, y, pressure)),
				Event::ControllerTouchpadMotion { which, touchpad, finger, x, y, pressure, .. } =>
					Some(MuiEvent::GamepadTouchpadMotion(which, touchpad, finger, x, y, pressure)),
				Event::ControllerTouchpadUp { which, touchpad, finger, x, y, pressure, .. } =>
					Some(MuiEvent::GamepadTouchpadUp(which, touchpad, finger, x, y, pressure)),
				Event::DropFile { filename, .. } => Some(MuiEvent::DropFile(filename)),
				Event::DropText { filename: text, .. } => Some(MuiEvent::DropText(text)),
				Event::DropBegin { .. } => Some(MuiEvent::DropBegin),
				Event::DropComplete { .. } => Some(MuiEvent::DropComplete),
				Event::RenderTargetsReset { .. } => Some(MuiEvent::RenderTargetsReset),
				Event::RenderDeviceReset { .. } => Some(MuiEvent::RenderDeviceReset),
				Event::Display { display, display_event, .. } => match display_event {
					DisplayEvent::Added => Some(MuiEvent::DisplayAdded(DisplayHandle { display })),
					DisplayEvent::Removed => Some(MuiEvent::DisplayRemoved(DisplayHandle { display })),
					DisplayEvent::Moved => Some(MuiEvent::DisplayMoved(DisplayHandle { display })),
					_ => None,
				},
				_ => None,
			} {
				events.push(v);
			}
		});
		events
	}
}

/// This list is made and filtered according to SDL 3 documentation of `SDL_EventType`.
pub(crate) enum MuiEvent {
	// Display orientation, content scale and display mode monitoring are not used, so skipped.
	DisplayAdded(DisplayHandle),
	DisplayRemoved(DisplayHandle),
	DisplayMoved(DisplayHandle),
	WindowShown,
	WindowHidden,
	WindowExposed,
	WindowMoved(i32, i32),
	WindowResized(i32, i32),
	WindowPixelSizeChanged(i32, i32), // Only use this to update size of viewport
	WindowMetalViewResized, // Not yet ported by sdl3-rs
	WindowMinimized,
	WindowMaximized,
	WindowRestored,
	WindowMouseEnter,
	WindowMouseLeave,
	WindowFocusGained,
	WindowFocusLost,
	WindowCloseRequested,
	WindowIccProfChanged,
	WindowOccluded, // Not yet ported by sdl3-rs
	WindowEnterFullscreen, // Not yet ported to sdl3-rs
	WindowLeaveFullscreen, // Not yet ported to sdl3-rs
	WindowDestroyed, // Not yet ported to sdl3-rs
	WindowHdrStateChanged, // Not yet ported to sdl3-rs
	KeyboardKeyDown(u32, KeyboardKey),
	KeyboardKeyUp(u32, KeyboardKey),
	TextEditing(String, i32, i32),
	TextInput(String),
	KeymapChanged, // Not yet ported to sdl3-rs; not used at the moment
	KeyboardAdded, // Not yet ported by sdl3-rs
	KeyboardRemoved, // Not yet ported by sdl3-rs
	TextEditingCandidates, // Not yet ported to sdl3-rs
	MouseMotion(u32, f32, f32),
	MouseButtonDown(u32, MouseKey),
	MouseButtonUp(u32, MouseKey),
	MouseWheel(u32, f32, f32), // y is positive to the down to be aligned with coordinates; i.e., inverted.
	MouseAdded, // Not yet ported to sdl3-rs
	MouseRemoved, // Not yet ported to sdl3-rs
	JoystickAxisMotion(u32, u8, i16),
	JoystickBallMotion, // Not yet ported to sdl3-rs
	JoystickHatMotion(u32, u8, JoystickHatState),
	JoystickButtonDown(u32, u8),
	JoystickButtonUp(u32, u8),
	JoystickAdded(u32),
	JoystickRemoved(u32),
	JoystickBatteryUpdated, // Not yet ported to sdl3-rs
	GamepadAxisMotion(u32, GamepadAxis, i16),
	GamepadButtonDown(u32, GamepadButton),
	GamepadButtonUp(u32, GamepadButton),
	GamepadAdded(u32),
	GamepadRemoved(u32),
	GamepadRemapped(u32),
	// No idea how to use touchpad, so just kept as is.
	GamepadTouchpadDown(u32, i32, i32, f32, f32, f32),
	GamepadTouchpadMotion(u32, i32, i32, f32, f32, f32),
	GamepadTouchpadUp(u32, i32, i32, f32, f32, f32),
	GamepadSteamHandleUpdated, // Not yet ported to sdl3-rs
	DropFile(String),
	DropText(String),
	DropBegin,
	DropComplete,
	DropPosition, // Not yet ported to sdl3-rs
	RenderTargetsReset,
	RenderDeviceReset,
	RenderDeviceLost, // Not yet ported to sdl3-rs
}

pub(crate) struct DisplayHandle {
	display: Display,
}

pub(crate) struct SdlDisplay {
	name: String,
	bounds: Rect,
	usable_bounds: Rect,
	fullscreen_modes: Vec<DisplayMode>,
	hdr_enabled: bool,
}

impl SdlDisplay {
	pub(crate) fn new(display: &Display) -> Result<Self, sdl3::Error> {
		Ok(Self {
			name: display.get_name()?,
			bounds: display.get_bounds()?,
			usable_bounds: display.get_usable_bounds()?,
			fullscreen_modes: display.get_fullscreen_modes()?,
			hdr_enabled: display.get_properties().map_err(|e| match e {
				PropertiesError::SdlError(e) => e,
				_ => panic!("{:?}", e),
			})?.contains("SDL.display.HDR_enabled").map_err(|e| match e {
				PropertiesError::SdlError(e) => e,
				_ => panic!("{:?}", e),
			})?,
		})
	}
	
	fn update_bounds(&mut self, display: Display) -> Result<(), sdl3::Error> {
		self.bounds = display.get_bounds()?;
		self.usable_bounds = display.get_usable_bounds()?;
		Ok(())
	}
}

/// This list is made and filtered according to SDL 3 documentation of `SDL_Scancode`.
pub(crate) enum KeyboardKey {
	// Unknown is skipped.
	A,
	B,
	C,
	D,
	E,
	F,
	G,
	H,
	I,
	J,
	K,
	L,
	M,
	N,
	O,
	P,
	Q,
	R,
	S,
	T,
	U,
	V,
	W,
	X,
	Y,
	Z,
	_1,
	_2,
	_3,
	_4,
	_5,
	_6,
	_7,
	_8,
	_9,
	_0,
	Return,
	Escape,
	Backspace,
	Tab,
	Space,
	Minus,
	Equals,
	LeftBracket,
	RightBracket,
	Backslash,
	NonUsHash,
	Semicolon,
	Apostrophe,
	Grave,
	Comma,
	Period,
	Slash,
	CapsLock,
	F1,
	F2,
	F3,
	F4,
	F5,
	F6,
	F7,
	F8,
	F9,
	F10,
	F11,
	F12,
	PrintScreen,
	ScrollLock,
	Pause,
	Insert,
	Home,
	PageUp,
	Delete,
	End,
	PageDown,
	Right,
	Left,
	Down,
	Up,
	NumLockClear,
	KpDivide,
	KpMultiply,
	KpMinus,
	KpPlus,
	KpEnter,
	Kp1,
	Kp2,
	Kp3,
	Kp4,
	Kp5,
	Kp6,
	Kp7,
	Kp8,
	Kp9,
	Kp0,
	KpPeriod,
	NonUsBackslash,
	Application,
	Power,
	KpEquals,
	F13,
	F14,
	F15,
	F16,
	F17,
	F18,
	F19,
	F20,
	F21,
	F22,
	F23,
	F24,
	Execute,
	Help,
	Menu,
	Select,
	Stop,
	Again,
	Undo,
	Cut,
	Copy,
	Paste,
	Find,
	Mute,
	VolumeUp,
	VolumeDown,
	KpComma,
	KpEqualsAs400,
	International1,
	International2,
	International3,
	International4,
	International5,
	International6,
	International7,
	International8,
	International9,
	Lang1,
	Lang2,
	Lang3,
	Lang4,
	Lang5,
	Lang6,
	Lang7,
	Lang8,
	Lang9,
	AltErase,
	SysReq,
	Cancel,
	Clear,
	Prior,
	Return2,
	Separator,
	Out,
	Oper,
	ClearAgain,
	CrSel,
	ExSel,
	Kp00,
	Kp000,
	ThousandsSeparator,
	DecimalSeparator,
	CurrencyUnit,
	CurrencySubunit,
	KpLeftParen,
	KpRightParen,
	KpLeftBrace,
	KpRightBrace,
	KpTab,
	KpBackspace,
	KpA,
	KpB,
	KpC,
	KpD,
	KpE,
	KpF,
	KpXor,
	KpPower,
	KpPercent,
	KpLess,
	KpGreater,
	KpAmpersand,
	KpDblAmpersand,
	KpVerticalBar,
	KpDblVerticalBar,
	KpColon,
	KpHash,
	KpSpace,
	KpAt,
	KpExclam,
	KpMemStore,
	KpMemRecall,
	KpMemClear,
	KpMemAdd,
	KpMemSubtract,
	KpMemMultiply,
	KpMemDivide,
	KpPlusMinus,
	KpClear,
	KpClearEntry,
	KpBinary,
	KpOctal,
	KpDecimal,
	KpHexadecimal,
	LCtrl,
	LShift,
	LAlt,
	LGui,
	RCtrl,
	RShift,
	RAlt,
	RGui,
	Mode,
	Sleep,
	Wake,
	ChannelIncrement,
	ChannelDecrement,
	MediaPlay,
	MediaPause,
	MediaRecord,
	MediaFastForward,
	MediaRewind,
	MediaNextTrack,
	MediaPreviousTrack,
	MediaStop,
	MediaEject,
	MediaPlayPause,
	MediaSelect,
	AcNew,
	AcOpen,
	AcClose,
	AcExit,
	AcSave,
	AcPrint,
	AcProperties,
	AcSearch,
	AcHome,
	AcBack,
	AcForward,
	AcStop,
	AcRefresh,
	AcBookmarks,
	// Mobile and reserved keys are skipped.
}

impl KeyboardKey {
	fn from_sdl(scancode: Scancode) -> Option<Self> {
		match scancode {
			Scancode::A => Some(KeyboardKey::A),
			Scancode::B => Some(KeyboardKey::B),
			Scancode::C => Some(KeyboardKey::C),
			Scancode::D => Some(KeyboardKey::D),
			Scancode::E => Some(KeyboardKey::E),
			Scancode::F => Some(KeyboardKey::F),
			Scancode::G => Some(KeyboardKey::G),
			Scancode::H => Some(KeyboardKey::H),
			Scancode::I => Some(KeyboardKey::I),
			Scancode::J => Some(KeyboardKey::J),
			Scancode::K => Some(KeyboardKey::K),
			Scancode::L => Some(KeyboardKey::L),
			Scancode::M => Some(KeyboardKey::M),
			Scancode::N => Some(KeyboardKey::N),
			Scancode::O => Some(KeyboardKey::O),
			Scancode::P => Some(KeyboardKey::P),
			Scancode::Q => Some(KeyboardKey::Q),
			Scancode::R => Some(KeyboardKey::R),
			Scancode::S => Some(KeyboardKey::S),
			Scancode::T => Some(KeyboardKey::T),
			Scancode::U => Some(KeyboardKey::U),
			Scancode::V => Some(KeyboardKey::V),
			Scancode::W => Some(KeyboardKey::W),
			Scancode::X => Some(KeyboardKey::X),
			Scancode::Y => Some(KeyboardKey::Y),
			Scancode::Z => Some(KeyboardKey::Z),
			Scancode::_1 => Some(KeyboardKey::_1),
			Scancode::_2 => Some(KeyboardKey::_2),
			Scancode::_3 => Some(KeyboardKey::_3),
			Scancode::_4 => Some(KeyboardKey::_4),
			Scancode::_5 => Some(KeyboardKey::_5),
			Scancode::_6 => Some(KeyboardKey::_6),
			Scancode::_7 => Some(KeyboardKey::_7),
			Scancode::_8 => Some(KeyboardKey::_8),
			Scancode::_9 => Some(KeyboardKey::_9),
			Scancode::_0 => Some(KeyboardKey::_0),
			Scancode::Return => Some(KeyboardKey::Return),
			Scancode::Escape => Some(KeyboardKey::Escape),
			Scancode::Backspace => Some(KeyboardKey::Backspace),
			Scancode::Tab => Some(KeyboardKey::Tab),
			Scancode::Space => Some(KeyboardKey::Space),
			Scancode::Minus => Some(KeyboardKey::Minus),
			Scancode::Equals => Some(KeyboardKey::Equals),
			Scancode::LeftBracket => Some(KeyboardKey::LeftBracket),
			Scancode::RightBracket => Some(KeyboardKey::RightBracket),
			Scancode::Backslash => Some(KeyboardKey::Backslash),
			Scancode::NonUsHash => Some(KeyboardKey::NonUsHash),
			Scancode::Semicolon => Some(KeyboardKey::Semicolon),
			Scancode::Apostrophe => Some(KeyboardKey::Apostrophe),
			Scancode::Grave => Some(KeyboardKey::Grave),
			Scancode::Comma => Some(KeyboardKey::Comma),
			Scancode::Period => Some(KeyboardKey::Period),
			Scancode::Slash => Some(KeyboardKey::Slash),
			Scancode::CapsLock => Some(KeyboardKey::CapsLock),
			Scancode::F1 => Some(KeyboardKey::F1),
			Scancode::F2 => Some(KeyboardKey::F2),
			Scancode::F3 => Some(KeyboardKey::F3),
			Scancode::F4 => Some(KeyboardKey::F4),
			Scancode::F5 => Some(KeyboardKey::F5),
			Scancode::F6 => Some(KeyboardKey::F6),
			Scancode::F7 => Some(KeyboardKey::F7),
			Scancode::F8 => Some(KeyboardKey::F8),
			Scancode::F9 => Some(KeyboardKey::F9),
			Scancode::F10 => Some(KeyboardKey::F10),
			Scancode::F11 => Some(KeyboardKey::F11),
			Scancode::F12 => Some(KeyboardKey::F12),
			Scancode::PrintScreen => Some(KeyboardKey::PrintScreen),
			Scancode::ScrollLock => Some(KeyboardKey::ScrollLock),
			Scancode::Pause => Some(KeyboardKey::Pause),
			Scancode::Insert => Some(KeyboardKey::Insert),
			Scancode::Home => Some(KeyboardKey::Home),
			Scancode::PageUp => Some(KeyboardKey::PageUp),
			Scancode::Delete => Some(KeyboardKey::Delete),
			Scancode::End => Some(KeyboardKey::End),
			Scancode::PageDown => Some(KeyboardKey::PageDown),
			Scancode::Right => Some(KeyboardKey::Right),
			Scancode::Left => Some(KeyboardKey::Left),
			Scancode::Down => Some(KeyboardKey::Down),
			Scancode::Up => Some(KeyboardKey::Up),
			Scancode::NumLockClear => Some(KeyboardKey::NumLockClear),
			Scancode::KpDivide => Some(KeyboardKey::KpDivide),
			Scancode::KpMultiply => Some(KeyboardKey::KpMultiply),
			Scancode::KpMinus => Some(KeyboardKey::KpMinus),
			Scancode::KpPlus => Some(KeyboardKey::KpPlus),
			Scancode::KpEnter => Some(KeyboardKey::KpEnter),
			Scancode::Kp1 => Some(KeyboardKey::Kp1),
			Scancode::Kp2 => Some(KeyboardKey::Kp2),
			Scancode::Kp3 => Some(KeyboardKey::Kp3),
			Scancode::Kp4 => Some(KeyboardKey::Kp4),
			Scancode::Kp5 => Some(KeyboardKey::Kp5),
			Scancode::Kp6 => Some(KeyboardKey::Kp6),
			Scancode::Kp7 => Some(KeyboardKey::Kp7),
			Scancode::Kp8 => Some(KeyboardKey::Kp8),
			Scancode::Kp9 => Some(KeyboardKey::Kp9),
			Scancode::Kp0 => Some(KeyboardKey::Kp0),
			Scancode::KpPeriod => Some(KeyboardKey::KpPeriod),
			Scancode::NonUsBackslash => Some(KeyboardKey::NonUsBackslash),
			Scancode::Application => Some(KeyboardKey::Application),
			Scancode::Power => Some(KeyboardKey::Power),
			Scancode::KpEquals => Some(KeyboardKey::KpEquals),
			Scancode::F13 => Some(KeyboardKey::F13),
			Scancode::F14 => Some(KeyboardKey::F14),
			Scancode::F15 => Some(KeyboardKey::F15),
			Scancode::F16 => Some(KeyboardKey::F16),
			Scancode::F17 => Some(KeyboardKey::F17),
			Scancode::F18 => Some(KeyboardKey::F18),
			Scancode::F19 => Some(KeyboardKey::F19),
			Scancode::F20 => Some(KeyboardKey::F20),
			Scancode::F21 => Some(KeyboardKey::F21),
			Scancode::F22 => Some(KeyboardKey::F22),
			Scancode::F23 => Some(KeyboardKey::F23),
			Scancode::F24 => Some(KeyboardKey::F24),
			Scancode::Execute => Some(KeyboardKey::Execute),
			Scancode::Help => Some(KeyboardKey::Help),
			Scancode::Menu => Some(KeyboardKey::Menu),
			Scancode::Select => Some(KeyboardKey::Select),
			Scancode::Stop => Some(KeyboardKey::Stop),
			Scancode::Again => Some(KeyboardKey::Again),
			Scancode::Undo => Some(KeyboardKey::Undo),
			Scancode::Cut => Some(KeyboardKey::Cut),
			Scancode::Copy => Some(KeyboardKey::Copy),
			Scancode::Paste => Some(KeyboardKey::Paste),
			Scancode::Find => Some(KeyboardKey::Find),
			Scancode::Mute => Some(KeyboardKey::Mute),
			Scancode::VolumeUp => Some(KeyboardKey::VolumeUp),
			Scancode::VolumeDown => Some(KeyboardKey::VolumeDown),
			Scancode::KpComma => Some(KeyboardKey::KpComma),
			Scancode::KpEqualsAs400 => Some(KeyboardKey::KpEqualsAs400),
			Scancode::International1 => Some(KeyboardKey::International1),
			Scancode::International2 => Some(KeyboardKey::International2),
			Scancode::International3 => Some(KeyboardKey::International3),
			Scancode::International4 => Some(KeyboardKey::International4),
			Scancode::International5 => Some(KeyboardKey::International5),
			Scancode::International6 => Some(KeyboardKey::International6),
			Scancode::International7 => Some(KeyboardKey::International7),
			Scancode::International8 => Some(KeyboardKey::International8),
			Scancode::International9 => Some(KeyboardKey::International9),
			Scancode::Lang1 => Some(KeyboardKey::Lang1),
			Scancode::Lang2 => Some(KeyboardKey::Lang2),
			Scancode::Lang3 => Some(KeyboardKey::Lang3),
			Scancode::Lang4 => Some(KeyboardKey::Lang4),
			Scancode::Lang5 => Some(KeyboardKey::Lang5),
			Scancode::Lang6 => Some(KeyboardKey::Lang6),
			Scancode::Lang7 => Some(KeyboardKey::Lang7),
			Scancode::Lang8 => Some(KeyboardKey::Lang8),
			Scancode::Lang9 => Some(KeyboardKey::Lang9),
			Scancode::AltErase => Some(KeyboardKey::AltErase),
			Scancode::SysReq => Some(KeyboardKey::SysReq),
			Scancode::Cancel => Some(KeyboardKey::Cancel),
			Scancode::Clear => Some(KeyboardKey::Clear),
			Scancode::Prior => Some(KeyboardKey::Prior),
			Scancode::Return2 => Some(KeyboardKey::Return2),
			Scancode::Separator => Some(KeyboardKey::Separator),
			Scancode::Out => Some(KeyboardKey::Out),
			Scancode::Oper => Some(KeyboardKey::Oper),
			Scancode::ClearAgain => Some(KeyboardKey::ClearAgain),
			Scancode::CrSel => Some(KeyboardKey::CrSel),
			Scancode::ExSel => Some(KeyboardKey::ExSel),
			Scancode::Kp00 => Some(KeyboardKey::Kp00),
			Scancode::Kp000 => Some(KeyboardKey::Kp000),
			Scancode::ThousandsSeparator => Some(KeyboardKey::ThousandsSeparator),
			Scancode::DecimalSeparator => Some(KeyboardKey::DecimalSeparator),
			Scancode::CurrencyUnit => Some(KeyboardKey::CurrencyUnit),
			Scancode::CurrencySubunit => Some(KeyboardKey::CurrencySubunit),
			Scancode::KpLeftParen => Some(KeyboardKey::KpLeftParen),
			Scancode::KpRightParen => Some(KeyboardKey::KpRightParen),
			Scancode::KpLeftBrace => Some(KeyboardKey::KpLeftBrace),
			Scancode::KpRightBrace => Some(KeyboardKey::KpRightBrace),
			Scancode::KpTab => Some(KeyboardKey::KpTab),
			Scancode::KpBackspace => Some(KeyboardKey::KpBackspace),
			Scancode::KpA => Some(KeyboardKey::KpA),
			Scancode::KpB => Some(KeyboardKey::KpB),
			Scancode::KpC => Some(KeyboardKey::KpC),
			Scancode::KpD => Some(KeyboardKey::KpD),
			Scancode::KpE => Some(KeyboardKey::KpE),
			Scancode::KpF => Some(KeyboardKey::KpF),
			Scancode::KpXor => Some(KeyboardKey::KpXor),
			Scancode::KpPower => Some(KeyboardKey::KpPower),
			Scancode::KpPercent => Some(KeyboardKey::KpPercent),
			Scancode::KpLess => Some(KeyboardKey::KpLess),
			Scancode::KpGreater => Some(KeyboardKey::KpGreater),
			Scancode::KpAmpersand => Some(KeyboardKey::KpAmpersand),
			Scancode::KpDblAmpersand => Some(KeyboardKey::KpDblAmpersand),
			Scancode::KpVerticalBar => Some(KeyboardKey::KpVerticalBar),
			Scancode::KpDblVerticalBar => Some(KeyboardKey::KpDblVerticalBar),
			Scancode::KpColon => Some(KeyboardKey::KpColon),
			Scancode::KpHash => Some(KeyboardKey::KpHash),
			Scancode::KpSpace => Some(KeyboardKey::KpSpace),
			Scancode::KpAt => Some(KeyboardKey::KpAt),
			Scancode::KpExclam => Some(KeyboardKey::KpExclam),
			Scancode::KpMemStore => Some(KeyboardKey::KpMemStore),
			Scancode::KpMemRecall => Some(KeyboardKey::KpMemRecall),
			Scancode::KpMemClear => Some(KeyboardKey::KpMemClear),
			Scancode::KpMemAdd => Some(KeyboardKey::KpMemAdd),
			Scancode::KpMemSubtract => Some(KeyboardKey::KpMemSubtract),
			Scancode::KpMemMultiply => Some(KeyboardKey::KpMemMultiply),
			Scancode::KpMemDivide => Some(KeyboardKey::KpMemDivide),
			Scancode::KpPlusMinus => Some(KeyboardKey::KpPlusMinus),
			Scancode::KpClear => Some(KeyboardKey::KpClear),
			Scancode::KpClearEntry => Some(KeyboardKey::KpClearEntry),
			Scancode::KpBinary => Some(KeyboardKey::KpBinary),
			Scancode::KpOctal => Some(KeyboardKey::KpOctal),
			Scancode::KpDecimal => Some(KeyboardKey::KpDecimal),
			Scancode::KpHexadecimal => Some(KeyboardKey::KpHexadecimal),
			Scancode::LCtrl => Some(KeyboardKey::LCtrl),
			Scancode::LShift => Some(KeyboardKey::LShift),
			Scancode::LAlt => Some(KeyboardKey::LAlt),
			Scancode::LGui => Some(KeyboardKey::LGui),
			Scancode::RCtrl => Some(KeyboardKey::RCtrl),
			Scancode::RShift => Some(KeyboardKey::RShift),
			Scancode::RAlt => Some(KeyboardKey::RAlt),
			Scancode::RGui => Some(KeyboardKey::RGui),
			Scancode::Mode => Some(KeyboardKey::Mode),
			Scancode::Sleep => Some(KeyboardKey::Sleep),
			Scancode::Wake => Some(KeyboardKey::Wake),
			Scancode::ChannelIncrement => Some(KeyboardKey::ChannelIncrement),
			Scancode::ChannelDecrement => Some(KeyboardKey::ChannelDecrement),
			Scancode::MediaPlay => Some(KeyboardKey::MediaPlay),
			Scancode::MediaPause => Some(KeyboardKey::MediaPause),
			Scancode::MediaRecord => Some(KeyboardKey::MediaRecord),
			Scancode::MediaFastForward => Some(KeyboardKey::MediaFastForward),
			Scancode::MediaRewind => Some(KeyboardKey::MediaRewind),
			Scancode::MediaNextTrack => Some(KeyboardKey::MediaNextTrack),
			Scancode::MediaPreviousTrack => Some(KeyboardKey::MediaPreviousTrack),
			Scancode::MediaStop => Some(KeyboardKey::MediaStop),
			Scancode::MediaEject => Some(KeyboardKey::MediaEject),
			Scancode::MediaPlayPause => Some(KeyboardKey::MediaPlayPause),
			Scancode::MediaSelect => Some(KeyboardKey::MediaSelect),
			Scancode::AcNew => Some(KeyboardKey::AcNew),
			Scancode::AcOpen => Some(KeyboardKey::AcOpen),
			Scancode::AcClose => Some(KeyboardKey::AcClose),
			Scancode::AcExit => Some(KeyboardKey::AcExit),
			Scancode::AcSave => Some(KeyboardKey::AcSave),
			Scancode::AcPrint => Some(KeyboardKey::AcPrint),
			Scancode::AcProperties => Some(KeyboardKey::AcProperties),
			Scancode::AcSearch => Some(KeyboardKey::AcSearch),
			Scancode::AcHome => Some(KeyboardKey::AcHome),
			Scancode::AcBack => Some(KeyboardKey::AcBack),
			Scancode::AcForward => Some(KeyboardKey::AcForward),
			Scancode::AcStop => Some(KeyboardKey::AcStop),
			Scancode::AcRefresh => Some(KeyboardKey::AcRefresh),
			Scancode::AcBookmarks => Some(KeyboardKey::AcBookmarks),
			_ => None,
		}
	}
}

/// This list is made and filtered according to SDL 3 documentation of `SDL_MouseButtonFlags`.
pub(crate) enum MouseKey {
	// Unknown is skipped.
	Left,
	Middle,
	Right,
	X1,
	X2,
}

impl MouseKey {
	fn from_sdl(mouse_button: MouseButton) -> Option<Self> {
		match mouse_button {
			MouseButton::Left => Some(MouseKey::Left),
			MouseButton::Middle => Some(MouseKey::Middle),
			MouseButton::Right => Some(MouseKey::Right),
			MouseButton::X1 => Some(MouseKey::X1),
			MouseButton::X2 => Some(MouseKey::X2),
			_ => None,
		}
	}
}
