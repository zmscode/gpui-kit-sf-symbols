//! Draws SF Symbols with GPUI's headless Metal renderer and checks the
//! pixels, so a path, asset-source or element regression that leaves an icon
//! blank fails here instead of in an app.
//!
//! ```sh
//! cargo test --test rendering
//! ```
//!
//! Set `SF_SYMBOLS_SNAPSHOT=path.png` to also save a grid of every weight.

// The `gpui-kit` dev-dependency always uses gpui-pre, so under `gpui-fast` its
// `Icon` and this crate's types come from different GPUI builds.
fn main() {
    #[cfg(all(target_os = "macos", not(feature = "gpui-fast")))]
    if gpui_kit_sf_symbols::is_available() {
        macos::run();
    } else {
        println!("rendering: skipped; SF Symbols need macOS 13 or later");
    }
    #[cfg(not(target_os = "macos"))]
    println!("rendering: skipped; SF Symbols are only available on macOS");
    #[cfg(feature = "gpui-fast")]
    println!("rendering: skipped; the gpui-kit dev-dependency does not use gpui-fast");
}

#[cfg(all(target_os = "macos", not(feature = "gpui-fast")))]
mod macos {
    use std::sync::Arc;

    use gpui_kit::{
        AnyElement, AppContext as _, AssetSource, Bounds, Context, HeadlessAppContext, IntoElement,
        ParentElement as _, Render, Styled as _, Window, WindowBounds, WindowOptions,
        assets::Assets,
        component::{ActiveTheme as _, Icon, Sizable as _},
        div, px, size,
    };
    use gpui_kit_sf_symbols::{SfSymbol, SfSymbolAssets, Weight};
    use image::RgbaImage;

    struct View(Box<dyn Fn() -> AnyElement>);

    impl Render for View {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .p_1()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .text_size(px(24.))
                .child((self.0)())
        }
    }

    fn draw(
        assets: impl AssetSource,
        width: f32,
        height: f32,
        content: impl Fn() -> AnyElement + 'static,
    ) -> RgbaImage {
        let mut cx = HeadlessAppContext::with_platform(
            gpui_kit::platform::current_platform(true).text_system(),
            Arc::new(assets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(gpui_kit::init);
        let (handle, _) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: Default::default(),
                            size: size(px(width), px(height)),
                        })),
                        focus: false,
                        show: false,
                        ..Default::default()
                    },
                    cx,
                    |_, cx| cx.new(|_| View(Box::new(content))),
                )
            })
            .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        cx.capture_screenshot(handle)
            .expect("Metal rendering must be available")
    }

    fn icon(assets: impl AssetSource, content: impl Fn() -> AnyElement + 'static) -> RgbaImage {
        draw(assets, 40., 40., content)
    }

    /// Pixels that differ from the background (the top-left corner).
    fn ink(image: &RgbaImage) -> usize {
        let background = *image.get_pixel(0, 0);
        image.pixels().filter(|pixel| **pixel != background).count()
    }

    fn every_weight_renders_and_differs() {
        let mut seen = Vec::new();
        for weight in Weight::ALL {
            let image = icon(SfSymbolAssets, move || {
                Icon::new(SfSymbol::new("heart").weight(weight))
                    .large()
                    .into_any_element()
            });
            assert!(ink(&image) > 20, "{weight} heart drew nothing");
            assert!(
                !seen.contains(&image),
                "{weight} heart looks like another weight"
            );
            seen.push(image);
        }
    }

    fn bare_symbols_follow_text_style() {
        let image = icon(SfSymbolAssets, || {
            SfSymbol::new("gearshape").into_any_element()
        });
        assert!(ink(&image) > 20, "bare SfSymbol drew nothing");
    }

    fn hierarchical_and_masked_symbols_render() {
        let mono = icon(SfSymbolAssets, || {
            Icon::new(SfSymbol::new("folder.badge.plus"))
                .large()
                .into_any_element()
        });
        let layered = icon(SfSymbolAssets, || {
            Icon::new(SfSymbol::new("folder.badge.plus").hierarchical())
                .large()
                .into_any_element()
        });
        assert!(ink(&mono) > 20 && ink(&layered) > 20);
        assert_ne!(mono, layered);
    }

    fn missing_assets_draw_nothing() {
        // GPUI Kit's own source does not know `sf-symbols/…` paths.
        let image = icon(Assets, || {
            Icon::new(SfSymbol::new("heart")).large().into_any_element()
        });
        assert_eq!(ink(&image), 0);
        // Nor does the system know this symbol.
        let image = icon(SfSymbolAssets, || {
            Icon::new(SfSymbol::new("not.a.symbol"))
                .large()
                .into_any_element()
        });
        assert_eq!(ink(&image), 0);
    }

    fn layered_sources_serve_both_icon_sets() {
        let kit = icon(SfSymbolAssets.with(Assets), || {
            Icon::new(gpui_kit::component::IconName::Check)
                .large()
                .into_any_element()
        });
        assert!(ink(&kit) > 20, "fallback GPUI Kit icon drew nothing");
    }

    fn snapshot(path: &str) {
        const NAMES: &[&str] = &[
            "house",
            "gearshape",
            "magnifyingglass",
            "heart.fill",
            "star",
            "bell.badge",
            "bubble.left",
            "folder.badge.plus",
            "trash",
            "bolt.fill",
            "wifi",
            "minus",
        ];
        let image = draw(SfSymbolAssets, 520., 420., || {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .children(Weight::ALL.into_iter().map(|weight| {
                    div().flex().gap_3().children(
                        NAMES
                            .iter()
                            .map(move |name| SfSymbol::new(name).weight(weight)),
                    )
                }))
                .child(
                    div().flex().gap_3().children(
                        NAMES
                            .iter()
                            .map(|name| SfSymbol::new(name).hierarchical().variable_value(0.5)),
                    ),
                )
                .into_any_element()
        });
        image.save(path).expect("write snapshot");
        println!("rendering: wrote {path}");
    }

    pub fn run() {
        let tests: &[(&str, fn())] = &[
            (
                "every_weight_renders_and_differs",
                every_weight_renders_and_differs,
            ),
            (
                "bare_symbols_follow_text_style",
                bare_symbols_follow_text_style,
            ),
            (
                "hierarchical_and_masked_symbols_render",
                hierarchical_and_masked_symbols_render,
            ),
            ("missing_assets_draw_nothing", missing_assets_draw_nothing),
            (
                "layered_sources_serve_both_icon_sets",
                layered_sources_serve_both_icon_sets,
            ),
        ];
        for (name, test) in tests {
            test();
            println!("test {name} ... ok");
        }
        if let Some(path) = std::env::var_os("SF_SYMBOLS_SNAPSHOT") {
            snapshot(path.to_str().expect("UTF-8 snapshot path"));
        }
    }
}
