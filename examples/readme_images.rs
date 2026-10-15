//! Renders the images in README.md with GPUI's headless Metal renderer, in
//! light and dark variants. Nothing is shown on screen.
//!
//! ```sh
//! cargo run --example readme_images
//! ```

fn main() {
    #[cfg(target_os = "macos")]
    macos::run();
    #[cfg(not(target_os = "macos"))]
    eprintln!("readme_images: GPUI only supplies a headless renderer on macOS");
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{path::Path, sync::Arc};

    use gpui_kit::{
        AnyElement, AppContext as _, Bounds, Context, Entity, HeadlessAppContext, Hsla,
        IntoElement, ParentElement as _, Render, Styled as _, Window, WindowBounds, WindowOptions,
        assets::Assets,
        component::{
            ActiveTheme as _, Icon, Sizable as _, Theme, ThemeMode,
            button::{Button, ButtonVariants as _},
            input::{Input, InputState},
            tag::Tag,
        },
        div, px, size,
    };
    use gpui_kit_sf_symbols::{SfSymbol, SfSymbolAssets, Weight};

    const SHOWCASE: &[&str] = &[
        "house",
        "magnifyingglass",
        "bell",
        "bubble.left.and.text.bubble.right",
        "heart",
        "star",
        "folder",
        "camera",
        "bolt",
        "gearshape",
    ];

    type Build = Box<dyn Fn(&mut Window, &mut Context<Scene>) -> AnyElement>;

    struct Scene {
        build: Build,
    }

    impl Render for Scene {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let content = (self.build)(window, cx);
            div()
                .size_full()
                .p_6()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .child(content)
        }
    }

    fn render(
        out: &Path,
        name: &str,
        (width, height): (f32, f32),
        build: impl Fn(&mut Window, &mut Context<Scene>) -> AnyElement + Clone + 'static,
    ) {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let mut cx = HeadlessAppContext::with_platform(
                gpui_kit::platform::current_platform(true).text_system(),
                Arc::new(SfSymbolAssets.with(Assets)),
                gpui_kit::platform::current_headless_renderer,
            );
            cx.update(|cx| {
                gpui_kit::init(cx);
                Theme::change(mode, None, cx);
            });
            let build = build.clone();
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
                        move |_, cx| {
                            cx.new(|_| Scene {
                                build: Box::new(build),
                            })
                        },
                    )
                })
                .unwrap();
            // Two frames: the first one loads the SVGs.
            for _ in 0..2 {
                cx.update_window(handle, |_, window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                })
                .unwrap();
                cx.run_until_parked();
            }
            let suffix = if mode.is_dark() { "dark" } else { "light" };
            let path = out.join(format!("{name}-{suffix}.png"));
            cx.capture_screenshot(handle)
                .expect("Metal rendering must be available")
                .save(&path)
                .expect("write image");
            println!("wrote {}", path.display());
        }
    }

    /// Every weight of a row of icons.
    fn weights(_: &mut Window, cx: &mut Context<Scene>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(Weight::ALL.into_iter().map(move |weight| {
                div()
                    .flex()
                    .items_center()
                    .gap_5()
                    .child(
                        div()
                            .w(px(84.))
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(muted)
                            .child(weight.as_str()),
                    )
                    .children(SHOWCASE.iter().map(move |name| {
                        Icon::new(SfSymbol::new(name).weight(weight)).size(px(28.))
                    }))
            }))
            .into_any_element()
    }

    /// Sizes, colors, hierarchical layers and variable values.
    fn styling(_: &mut Window, cx: &mut Context<Scene>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let colors: [Hsla; 6] = [
            theme.foreground,
            theme.red,
            theme.yellow,
            theme.green,
            theme.blue,
            theme.magenta,
        ];
        let row = |label: &'static str| {
            div().flex().items_center().gap_5().child(
                div()
                    .w(px(84.))
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(muted)
                    .child(label),
            )
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                row("size").children([16., 20., 24., 32., 40., 48.].map(|size| {
                    Icon::new(SfSymbol::new("globe.americas.fill").hierarchical()).size(px(size))
                })),
            )
            .child(row("color").children(colors.map(|color| {
                Icon::new(SfSymbol::new("heart.fill"))
                    .size(px(32.))
                    .text_color(color)
            })))
            .child(row("hierarchical").children(colors.map(|color| {
                Icon::new(SfSymbol::new("folder.badge.plus").hierarchical())
                    .size(px(32.))
                    .text_color(color)
            })))
            .child(row("variable").children(
                [0., 0.25, 0.5, 1.].map(|value| {
                    Icon::new(SfSymbol::new("wifi").variable_value(value)).size(px(32.))
                }),
            ))
            .into_any_element()
    }

    /// SF Symbols inside GPUI Kit components.
    fn components(window: &mut Window, cx: &mut Context<Scene>) -> AnyElement {
        let search: Entity<InputState> = window.use_keyed_state("search", cx, |window, cx| {
            InputState::new(window, cx).placeholder("Search symbols…")
        });
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("save")
                            .primary()
                            .icon(SfSymbol::new("square.and.arrow.down").semibold())
                            .label("Save"),
                    )
                    .child(
                        Button::new("share")
                            .outline()
                            .icon(SfSymbol::new("square.and.arrow.up"))
                            .label("Share"),
                    )
                    .child(
                        Button::new("download")
                            .icon(SfSymbol::new("arrow.down.circle"))
                            .label("Download"),
                    )
                    .child(
                        Button::new("like")
                            .ghost()
                            .icon(SfSymbol::new("heart.fill")),
                    )
                    .child(
                        Button::new("more")
                            .ghost()
                            .icon(SfSymbol::new("ellipsis").bold()),
                    ),
            )
            .child(
                Input::new(&search)
                    .w(px(360.))
                    .prefix(Icon::new(SfSymbol::new("magnifyingglass")).small()),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Tag::success().outline().child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(Icon::new(SfSymbol::new("checkmark.circle.fill")).xsmall())
                                .child("Deployed"),
                        ),
                    )
                    .child(
                        Tag::warning().outline().child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Icon::new(SfSymbol::new("exclamationmark.triangle.fill"))
                                        .xsmall(),
                                )
                                .child("Pending"),
                        ),
                    )
                    .child(
                        Tag::danger().outline().child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(Icon::new(SfSymbol::new("xmark.circle.fill")).xsmall())
                                .child("Failed"),
                        ),
                    )
                    .child(
                        Tag::info().outline().child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Icon::new(SfSymbol::new("arrow.triangle.branch").semibold())
                                        .xsmall(),
                                )
                                .child("main"),
                        ),
                    ),
            )
            .into_any_element()
    }

    pub fn run() {
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/images");
        std::fs::create_dir_all(&out).expect("create docs/images");
        render(&out, "weights", (620., 382.), weights);
        render(&out, "styling", (580., 238.), styling);
        render(&out, "components", (580., 168.), components);
    }
}
