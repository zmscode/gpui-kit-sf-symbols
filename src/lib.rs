//! Apple [SF Symbols](https://developer.apple.com/sf-symbols/) for
//! [GPUI Kit](https://gpui-kit.com).
//!
//! [`SfSymbol`] plugs into GPUI Kit's `Icon` through the [`IconNamed`] trait,
//! and [`SfSymbolAssets`] serves the matching SVGs. Symbols are not bundled
//! (Apple's license does not allow it): they are rendered as vector paths from
//! the running system, so they look exactly like the OS's own, in any of the
//! nine weights, and tint with the text color like every other GPUI icon.
//!
//! ```no_run
//! use gpui_kit::*;
//! use gpui_kit::assets::Assets;
//! use gpui_kit::component::Icon;
//! use gpui_kit_sf_symbols::{SfSymbol, SfSymbolAssets, Weight};
//!
//! const GEAR: SfSymbol = SfSymbol::from_static("gearshape");
//!
//! struct Example;
//!
//! impl Render for Example {
//!     fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!         div()
//!             .flex()
//!             .gap_2()
//!             .child(Icon::new(GEAR))                                     // regular
//!             .child(Icon::new(SfSymbol::new("heart.fill").bold()))
//!             .child(Icon::new(SfSymbol::new("wifi").variable_value(0.5)))
//!             .child(SfSymbol::new("folder.badge.plus").hierarchical())   // no `Icon` needed
//!     }
//! }
//!
//! fn main() {
//!     // SF Symbols first, then GPUI Kit's own component icons.
//!     gpui_kit::application()
//!         .with_assets(SfSymbolAssets.with(Assets))
//!         .run(|cx| { /* ... */ });
//! }
//! ```
//!
//! # Platform support
//!
//! Rendering needs macOS 13 or later. Elsewhere, or for a name the system does
//! not know, a symbol simply draws nothing, the same as any other missing
//! asset in GPUI; check [`is_available`] or [`SfSymbol::exists`] to choose a
//! fallback icon. The crate builds on every platform.
//!
//! Symbols are rendered with SwiftUI, which only runs on the main thread. GPUI
//! loads icons there while painting, so this costs nothing in practice. Called
//! from another thread, [`SfSymbol::svg`] waits for the main thread and gives
//! up after a couple of seconds if it is busy. Each symbol is rendered once
//! per style and cached.
//!
//! Asset paths have the form `sf-symbols/{style}/{name}.svg`, e.g.
//! `sf-symbols/bold+hierarchical/folder.badge.plus.svg`, so they never collide
//! with GPUI Kit's `icons/…` paths.

#[cfg(feature = "gpui-fast")]
extern crate gpui_fast as gpui;

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock, Mutex},
};

use gpui::{
    AnyElement, App, AssetSource, Element as _, IntoElement, RenderOnce, SharedString, Styled,
    ViewElement, Window, svg,
};
pub use gpui_kit_assets::IconNamed;

// Compile-check the README's examples.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

/// Prefix shared by every asset path this crate serves.
pub const PATH_PREFIX: &str = "sf-symbols/";

/// A symbol weight, matching the system font weights.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Weight {
    UltraLight,
    Thin,
    Light,
    #[default]
    Regular,
    Medium,
    Semibold,
    Bold,
    Heavy,
    Black,
}

impl Weight {
    /// Every weight, lightest first.
    pub const ALL: [Weight; 9] = [
        Weight::UltraLight,
        Weight::Thin,
        Weight::Light,
        Weight::Regular,
        Weight::Medium,
        Weight::Semibold,
        Weight::Bold,
        Weight::Heavy,
        Weight::Black,
    ];

    /// The lowercase weight name used in asset paths, e.g. `"semibold"`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Weight::UltraLight => "ultralight",
            Weight::Thin => "thin",
            Weight::Light => "light",
            Weight::Regular => "regular",
            Weight::Medium => "medium",
            Weight::Semibold => "semibold",
            Weight::Bold => "bold",
            Weight::Heavy => "heavy",
            Weight::Black => "black",
        }
    }

    /// Parse a lowercase weight name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|weight| weight.as_str() == name)
    }
}

impl std::fmt::Display for Weight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Weight {
    type Err = ParseError;

    /// Parse a lowercase weight name, e.g. `"bold"`.
    fn from_str(name: &str) -> Result<Self, ParseError> {
        Self::from_name(name).ok_or(ParseError(()))
    }
}

/// How a symbol's layers are drawn. Both come out in a single color, so the
/// icon still follows the text color.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Rendering {
    /// Every layer fully opaque.
    #[default]
    Monochrome,
    /// Secondary layers at reduced opacity, e.g. the folder behind a badge.
    Hierarchical,
}

/// Returned when parsing an unknown weight name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseError(());

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown SF Symbols weight name")
    }
}

impl std::error::Error for ParseError {}

/// An SF Symbol by name, e.g. `"gearshape"` or `"folder.badge.plus"`, with
/// its weight, rendering mode and variable value.
///
/// Names are the ones shown in Apple's SF Symbols app. Which names exist
/// depends on the macOS version; see [`SfSymbol::exists`].
///
/// `SfSymbol` is `Copy`: names given at run time are interned (each distinct
/// name is kept for the life of the program).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SfSymbol {
    name: &'static str,
    weight: Weight,
    rendering: Rendering,
    /// Percent, for symbols with variable color such as `wifi`.
    variable_value: Option<u8>,
}

impl SfSymbol {
    /// A symbol in the regular weight.
    pub fn new(name: &str) -> Self {
        static NAMES: LazyLock<Mutex<HashSet<&'static str>>> = LazyLock::new(Default::default);

        let mut names = NAMES.lock().unwrap();
        let name = match names.get(name) {
            Some(name) => name,
            None => {
                let name: &'static str = Box::leak(name.into());
                names.insert(name);
                name
            }
        };
        Self::from_static(name)
    }

    /// Like [`SfSymbol::new`], without interning; usable in constants.
    pub const fn from_static(name: &'static str) -> Self {
        Self {
            name,
            weight: Weight::Regular,
            rendering: Rendering::Monochrome,
            variable_value: None,
        }
    }

    /// The symbol's name, e.g. `"gearshape"`.
    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn get_weight(self) -> Weight {
        self.weight
    }

    pub const fn get_rendering(self) -> Rendering {
        self.rendering
    }

    /// The variable value, if one was set, from 0.0 to 1.0.
    pub fn get_variable_value(self) -> Option<f32> {
        self.variable_value.map(|percent| f32::from(percent) / 100.)
    }

    /// This symbol in the given weight.
    pub fn weight(mut self, weight: Weight) -> Self {
        self.weight = weight;
        self
    }

    pub fn ultralight(self) -> Self {
        self.weight(Weight::UltraLight)
    }

    pub fn thin(self) -> Self {
        self.weight(Weight::Thin)
    }

    pub fn light(self) -> Self {
        self.weight(Weight::Light)
    }

    pub fn regular(self) -> Self {
        self.weight(Weight::Regular)
    }

    pub fn medium(self) -> Self {
        self.weight(Weight::Medium)
    }

    pub fn semibold(self) -> Self {
        self.weight(Weight::Semibold)
    }

    pub fn bold(self) -> Self {
        self.weight(Weight::Bold)
    }

    pub fn heavy(self) -> Self {
        self.weight(Weight::Heavy)
    }

    pub fn black(self) -> Self {
        self.weight(Weight::Black)
    }

    /// This symbol with the given rendering mode.
    pub fn rendering(mut self, rendering: Rendering) -> Self {
        self.rendering = rendering;
        self
    }

    /// Draw secondary layers at reduced opacity.
    pub fn hierarchical(self) -> Self {
        self.rendering(Rendering::Hierarchical)
    }

    /// Draw every layer fully opaque (the default).
    pub fn monochrome(self) -> Self {
        self.rendering(Rendering::Monochrome)
    }

    /// For symbols with variable color, such as `wifi` or `speaker.wave.3`:
    /// how much of the symbol is "on", from 0.0 to 1.0 (rounded to whole
    /// percent). Layers past the value are drawn faded. Other symbols ignore it.
    pub fn variable_value(mut self, value: f32) -> Self {
        self.variable_value = Some((value.clamp(0., 1.) * 100.).round() as u8);
        self
    }

    /// The asset path, e.g. `sf-symbols/bold+hierarchical+v50/wifi.svg`.
    pub fn path(self) -> SharedString {
        let mut path = format!("{PATH_PREFIX}{}", self.weight);
        if self.rendering == Rendering::Hierarchical {
            path.push_str("+hierarchical");
        }
        if let Some(percent) = self.variable_value {
            path.push_str(&format!("+v{percent}"));
        }
        path.push('/');
        path.push_str(self.name);
        path.push_str(".svg");
        path.into()
    }

    /// Parse an asset path produced by [`SfSymbol::path`].
    pub fn from_path(path: &str) -> Option<Self> {
        let rest = path.strip_prefix(PATH_PREFIX)?.strip_suffix(".svg")?;
        let (style, name) = rest.split_once('/')?;
        if name.is_empty() || name.contains('/') {
            return None;
        }
        let mut parts = style.split('+');
        let mut symbol = Self::new(name).weight(Weight::from_name(parts.next()?)?);
        for part in parts {
            if part == "hierarchical" && symbol.rendering == Rendering::Monochrome {
                symbol.rendering = Rendering::Hierarchical;
            } else if let Some(percent) = part.strip_prefix('v')
                && symbol.variable_value.is_none()
            {
                symbol.variable_value = Some(percent.parse().ok().filter(|p| *p <= 100)?);
            } else {
                return None;
            }
        }
        Some(symbol)
    }

    /// Whether the system has a symbol with this name.
    pub fn exists(self) -> bool {
        shim::exists(self.name)
    }

    /// The symbol as an SVG document, or `None` if it cannot be rendered (see
    /// the [crate docs](crate#platform-support)).
    ///
    /// Useful with `Icon::default().data(..)` or `svg().data(..)` when no
    /// asset source is wanted. Results are cached.
    pub fn svg(self) -> Option<Arc<[u8]>> {
        type Cache = HashMap<SfSymbol, Option<Arc<[u8]>>>;
        static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Default::default);

        if let Some(cached) = CACHE.lock().unwrap().get(&self) {
            return cached.clone();
        }
        // Not under the lock: rendering may wait on the main thread, which
        // may itself be waiting for the lock.
        let svg = shim::render(self);
        // A symbol that exists but failed to render (the main thread was busy)
        // is retried next time.
        if svg.is_some() || !self.exists() {
            CACHE.lock().unwrap().insert(self, svg.clone());
        }
        svg
    }
}

/// Whether SF Symbols can be rendered here: macOS 13 or later.
pub fn is_available() -> bool {
    shim::available()
}

impl From<&'static str> for SfSymbol {
    fn from(name: &'static str) -> Self {
        Self::from_static(name)
    }
}

impl From<&String> for SfSymbol {
    fn from(name: &String) -> Self {
        Self::new(name)
    }
}

impl From<&SharedString> for SfSymbol {
    fn from(name: &SharedString) -> Self {
        Self::new(name)
    }
}

impl std::fmt::Display for SfSymbol {
    /// The symbol's name, e.g. `gearshape`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

impl IconNamed for SfSymbol {
    fn path(self) -> SharedString {
        SfSymbol::path(self)
    }
}

// Like `gpui_kit_assets::IconName`, a symbol can be used directly as a child,
// sized and colored by the surrounding text style. Wrap it in
// `gpui_kit::component::Icon` for themed sizes and transformations.
//
// Written out instead of `#[derive(IntoElement)]` so the impls don't depend on
// how the derive spells GPUI's crate path; this mirrors what it generates.
impl IntoElement for SfSymbol {
    type Element = ViewElement<Self>;

    #[track_caller]
    fn into_element(self) -> Self::Element {
        ViewElement::new(self)
    }

    #[track_caller]
    fn into_any_element(self) -> AnyElement {
        self.into_element().into_any()
    }
}

impl RenderOnce for SfSymbol {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let text_style = window.text_style();
        svg()
            .path(self.path())
            .flex_shrink_0()
            .size(text_style.font_size.to_pixels(window.rem_size()))
            .text_color(text_style.color)
    }
}

impl From<SfSymbol> for AnyElement {
    fn from(symbol: SfSymbol) -> Self {
        symbol.into_any_element()
    }
}

/// Renders any `sf-symbols/…` path from the system.
///
/// Combine it with GPUI Kit's own `Assets` (or any other source) using
/// [`SfSymbolAssets::with`]:
///
/// ```no_run
/// # use gpui_kit_sf_symbols::SfSymbolAssets;
/// gpui_kit::application().with_assets(SfSymbolAssets.with(gpui_kit::assets::Assets));
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct SfSymbolAssets;

impl SfSymbolAssets {
    /// Serve SF Symbols first and fall back to `fallback` for other paths.
    pub fn with<B: AssetSource>(self, fallback: B) -> Layered<Self, B> {
        Layered(self, fallback)
    }
}

impl AssetSource for SfSymbolAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(SfSymbol::from_path(path)
            .and_then(|symbol| symbol.svg())
            .map(|svg| Cow::Owned(svg.to_vec())))
    }

    /// The system's symbols cannot be enumerated, so this lists nothing.
    fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

/// Two asset sources layered together: `load` tries the first source, then the
/// second; `list` merges both.
#[derive(Clone, Copy, Debug, Default)]
pub struct Layered<A, B>(pub A, pub B);

impl<A, B> Layered<A, B> {
    /// Add another fallback below the existing ones.
    pub fn with<C: AssetSource>(self, fallback: C) -> Layered<Self, C> {
        Layered(self, fallback)
    }
}

impl<A: AssetSource, B: AssetSource> AssetSource for Layered<A, B> {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        match self.0.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => self.1.load(path),
        }
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut paths = self.0.list(path)?;
        paths.extend(self.1.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

/// The Swift shim compiled by `build.rs` (see `swift/shim.swift`).
#[cfg(sf_symbols_shim)]
mod shim {
    use std::{
        ffi::{CString, c_char, c_int, c_void},
        sync::{Arc, mpsc},
        time::Duration,
    };

    use crate::{Rendering, SfSymbol};

    /// How long an off-main-thread render waits for the main thread.
    const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(2);

    unsafe extern "C" {
        /// Main thread only.
        fn gpui_sf_symbol_svg(
            name: *const c_char,
            weight: i32,
            hierarchical: bool,
            variable_value: f64,
            length: *mut usize,
        ) -> *mut u8;
        fn gpui_sf_symbol_free(buffer: *mut u8);
        fn gpui_sf_symbol_exists(name: *const c_char) -> bool;
        fn gpui_sf_symbol_available() -> bool;

        // libSystem: pthreads and Grand Central Dispatch.
        fn pthread_main_np() -> c_int;
        static _dispatch_main_q: c_void;
        fn dispatch_async_f(
            queue: *const c_void,
            context: *mut c_void,
            work: unsafe extern "C" fn(*mut c_void),
        );
    }

    pub fn available() -> bool {
        unsafe { gpui_sf_symbol_available() }
    }

    pub fn exists(name: &str) -> bool {
        let Ok(name) = CString::new(name) else {
            return false;
        };
        unsafe { gpui_sf_symbol_exists(name.as_ptr()) }
    }

    pub fn render(symbol: SfSymbol) -> Option<Arc<[u8]>> {
        if unsafe { pthread_main_np() } != 0 {
            return render_on_main(symbol);
        }

        // Hop to the main queue, which GPUI's run loop drains, and wait. The
        // job owns its context, so giving up early leaves nothing dangling.
        type Job = (SfSymbol, mpsc::Sender<Option<Arc<[u8]>>>);
        unsafe extern "C" fn run(context: *mut c_void) {
            let (symbol, done) = *unsafe { Box::from_raw(context.cast::<Job>()) };
            done.send(render_on_main(symbol)).ok();
        }
        let (done, result) = mpsc::channel();
        let job: Box<Job> = Box::new((symbol, done));
        unsafe {
            dispatch_async_f(&raw const _dispatch_main_q, Box::into_raw(job).cast(), run);
        }
        result.recv_timeout(MAIN_THREAD_TIMEOUT).ok().flatten()
    }

    fn render_on_main(symbol: SfSymbol) -> Option<Arc<[u8]>> {
        let name = CString::new(symbol.name()).ok()?;
        let variable_value = symbol.get_variable_value().map_or(f64::NAN, f64::from);
        let mut length = 0;
        unsafe {
            let buffer = gpui_sf_symbol_svg(
                name.as_ptr(),
                // Same order as the shim's `weights` table.
                symbol.weight as i32,
                symbol.rendering == Rendering::Hierarchical,
                variable_value,
                &mut length,
            );
            if buffer.is_null() {
                return None;
            }
            let svg = Arc::from(std::slice::from_raw_parts(buffer, length));
            gpui_sf_symbol_free(buffer);
            Some(svg)
        }
    }
}

#[cfg(not(sf_symbols_shim))]
mod shim {
    use std::sync::Arc;

    use crate::SfSymbol;

    pub fn available() -> bool {
        false
    }

    pub fn exists(_name: &str) -> bool {
        false
    }

    pub fn render(_symbol: SfSymbol) -> Option<Arc<[u8]>> {
        None
    }
}
