# gpui-kit-sf-symbols

Apple [SF Symbols](https://developer.apple.com/sf-symbols/) for
[GPUI Kit](https://gpui-kit.com) on macOS: any symbol the system has, in all
nine weights, with hierarchical layers and variable values.

`SfSymbol` implements GPUI Kit's `IconNamed` trait, so it works anywhere GPUI
Kit takes an icon: `Icon::new(...)`, buttons, menus, and so on.

Nothing is bundled. Apple's license does not allow redistributing the symbol
artwork, so symbols are rendered as vector SVG paths from the running system
(through SwiftUI) and tint with the text color like every other GPUI icon.

## Usage

```toml
[dependencies]
gpui-kit = "0.7.3"
gpui-kit-sf-symbols = "0.1"
```

Register the SF Symbols asset source along with GPUI Kit's own icons:

```rust,no_run
use gpui_kit::*;
use gpui_kit::assets::Assets;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit_sf_symbols::{SfSymbol, SfSymbolAssets, Weight};

const GEAR: SfSymbol = SfSymbol::from_static("gearshape");

struct Example;

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap_2()
            .child(Icon::new(GEAR))                                                // regular weight
            .child(Icon::new(SfSymbol::new("heart.fill").bold()).large())          // shorthand per weight
            .child(Icon::new(SfSymbol::new("house").weight(Weight::Semibold)))     // or pick at runtime
            .child(Icon::new(SfSymbol::new("folder.badge.plus").hierarchical()))   // layered opacity
            .child(Icon::new(SfSymbol::new("wifi").variable_value(0.5)))           // variable color
            .child(SfSymbol::new("magnifyingglass"))                               // bare: follows the text style
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(SfSymbolAssets.with(Assets)) // SF Symbols first, then GPUI Kit's icons
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Example))
                .expect("failed to open window");
        });
}
```

Names are the ones shown in Apple's SF Symbols app, e.g. `gearshape`,
`folder.badge.plus` or `speaker.wave.3.fill`. Names known at compile time can
be constants via `SfSymbol::from_static`; `SfSymbol::new` takes any `&str`.
`SfSymbol` is `Copy`, like GPUI Kit's own icon names.

| Builder                            | Effect                                                                             |
| ---------------------------------- | ---------------------------------------------------------------------------------- |
| `.ultralight()` … `.black()`       | One of the nine weights; `.weight(Weight::…)` picks one at runtime                 |
| `.hierarchical()`                  | Secondary layers at reduced opacity (still one color, so the tint still applies)   |
| `.variable_value(0.0..=1.0)`       | For variable-color symbols like `wifi`: layers past the value are drawn faded       |

Asset paths look like `sf-symbols/bold+hierarchical+v50/wifi.svg`, so they never
collide with GPUI Kit's `icons/...` paths. If you don't want an asset source at
all, `SfSymbol::new("gearshape").svg()` returns the SVG bytes for
`Icon::default().data(...)`.

## Platform support

Rendering needs **macOS 13** or later. The crate builds on every platform, but
elsewhere (or on older macOS, or for a name the system doesn't know) a symbol
draws nothing, the same as any other missing GPUI asset. Pick a fallback with
`gpui_kit_sf_symbols::is_available()` or `SfSymbol::exists()`; for example,
pair this crate with `gpui-kit-phosphor` on other platforms. Which symbols
exist depends on the macOS version.

Building on macOS compiles a small Swift shim, so the Xcode command line tools
must be installed (`xcode-select --install`). Set `SWIFTC` to use a specific
compiler.

SwiftUI only renders on the main thread. GPUI loads icons there while
painting, so this costs nothing in practice. Called from another thread,
`SfSymbol::svg()` hands the work to the main thread and gives up (returning
`None`) after two seconds if the main thread is busy. Each symbol is rendered
once per style and cached.

## How it works

SwiftUI's `ImageRenderer` draws symbols as vector paths into a PDF context
(AppKit's `NSImage` would rasterize them). The shim walks that PDF with
`CGPDFScanner` and rewrites its fills as SVG paths, turning layer opacity into
`fill-opacity` and the soft masks that knock out badges into SVG masks. The
result is a plain single-color SVG that GPUI renders as a tinted icon.

## Compatibility

| gpui-kit-sf-symbols | gpui-kit | GPUI snapshot     | macOS |
| ------------------- | -------- | ----------------- | ----- |
| 0.1                 | 0.7.3    | `gpui-pre =0.3.9` | 13+   |

The crate depends on the exact GPUI snapshot `gpui-kit-assets` uses so their
types line up. If you enable `gpui-kit`'s `gpui-fast` feature, enable this
crate's `gpui-fast` feature too.

## Gallery

```sh
cargo run --example gallery
```

## Tests

```sh
cargo test
```

`tests/symbols.rs` checks paths and the shim's SVG output; `tests/rendering.rs`
draws symbols with GPUI's headless Metal renderer (no window is shown) and
checks the pixels. Set `SF_SYMBOLS_SNAPSHOT=grid.png` to also save a grid of
every weight. Both run without the default test harness, because SwiftUI needs
the main thread.

## License

Crate code: Apache-2.0 (matching GPUI Kit). SF Symbols themselves are not
included; they are provided by macOS and subject to Apple's
[license terms](https://developer.apple.com/support/terms/), which allow using
them in apps for Apple platforms.
