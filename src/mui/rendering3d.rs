/*
 * SPDX-FileCopyrightText: 2026 TerraModulus Team and Contributors
 * SPDX-License-Identifier: LGPL-3.0-only
 */

//! ## Rendering in 3D
//!
//! There may be a generic rendering module regardless of dimensions, to support
//! flexible rendering of 3D in 2D environment in various places and scenarios.
//! Basically, 3D rendering would be mainly in GameplayScreen, but there may be occasions
//! where 3D objects may be rendered in 2D menus with a special supporting menu object.
//!
//! Moreover, the utilities like [Geom][super::rendering::Geom] and [Mesh][super::rendering::Mesh]
//! shall be generalized for specialized supports in 2D and 3D. However, coordinates and rendering
//! in 2D should only be recommended in 2D coordinates instead of being in 3D to prevent coordination
//! space conflict between entities in different dimensional spaces, in different aspects.
//!
//! When 2D objects are not in the 3D space logically, those should be regarded like 2D objects in
//! an environment where all 2D reside. In this case, utilities should also be used in 2D way,
//! but having generic utilities without dimensional constrain (as in 3D) may be problematic while handling.
//!
//! Therefore, several rendering utilties must be separately handled in 2D and 3D rendering modules
//! with their own implementations and collections of utilities. This may result in vast codebase and engine
//! to be ported to Kryon-native interface.
//!
//! At the end of rendering, all rendering environments are summerized into a single [Canvas],
//! so basically, there is still a need to separate mathing and transformation in different environments.
//! For 2D objects, those may only exist in GUI, so they shall always be explained in screen coordinates,
//! unless a case where isolated environments are required (probably keeping the flexibility?);
//! for 3D objects, separate environments must be needed, especially for one single environment for Gameplay.
//! The 2D environment must be created when a [Canvas] is to be initialized for all GUI elements to be in.
//! A 3D rendering environment must be created for a 3D canvas in GUI, or when a World is to be initialized.
//!
//! When a [PhyEnv][crate::phy::PhyEnv] is associated with a 3D rendering environment, a helper between
//! them must be used to interpret the objects in physics into the 3D rendering representations.
//! Shader programs aid in the interpretation, for various types of rendering logics and properties,
//! including "2.5D" objects (most likely particles) where the 2D textures always face to the camera.
//!
//! [Canvas]: super::rendering::CanvasHandle
