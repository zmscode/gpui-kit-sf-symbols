//! A small gallery of SF Symbols in every weight.
//!
//! ```sh
//! cargo run --example gallery
//! ```

use gpui_kit::assets::Assets;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, StyledExt as _};
use gpui_kit::*;
use gpui_kit_sf_symbols::{SfSymbol, SfSymbolAssets, Weight};

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
];

struct Gallery;

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .size_full()
            .p_6()
            .gap_4()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .children(Weight::ALL.into_iter().map(|weight| {
                div()
                    .h_flex()
                    .gap_4()
                    .items_center()
                    .child(div().w_20().text_sm().child(weight.as_str()))
                    .children(
                        NAMES
                            .iter()
                            .map(move |name| Icon::new(SfSymbol::new(name).weight(weight)).large()),
                    )
            }))
            .child(
                div()
                    .h_flex()
                    .gap_4()
                    .items_center()
                    .child(div().w_20().text_sm().child("layered"))
                    .children(
                        NAMES
                            .iter()
                            .map(|name| Icon::new(SfSymbol::new(name).hierarchical()).large()),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .gap_4()
                    .items_center()
                    .child(div().w_20().text_sm().child("variable"))
                    .children((0..=4).map(|step| {
                        Icon::new(
                            SfSymbol::new("speaker.wave.3.fill").variable_value(step as f32 / 4.),
                        )
                        .large()
                    })),
            )
            .child(
                // Bare symbols render at the current text size and color.
                div()
                    .h_flex()
                    .gap_2()
                    .items_center()
                    .text_color(cx.theme().primary)
                    .child(SfSymbol::new("sparkles").semibold())
                    .child("Bare symbols follow the text style"),
            )
    }
}

fn main() {
    if !gpui_kit_sf_symbols::is_available() {
        eprintln!("SF Symbols need macOS 13 or later; the gallery will be empty.");
    }
    gpui_kit::application()
        .with_assets(SfSymbolAssets.with(Assets))
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Gallery))
                .expect("failed to open window");
        });
}
