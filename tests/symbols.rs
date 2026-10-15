//! Paths, parsing, and rendering through the Swift shim.
//!
//! Runs without the default test harness: SwiftUI renders on the main thread,
//! which the harness keeps busy waiting for its worker threads.

#[cfg(not(feature = "gpui-fast"))]
use gpui::AssetSource;
#[cfg(feature = "gpui-fast")]
use gpui_fast::AssetSource;
use gpui_kit_assets::IconNamed;
use gpui_kit_sf_symbols::{Rendering, SfSymbol, SfSymbolAssets, Weight, is_available};

fn paths_round_trip() {
    assert_eq!(
        IconNamed::path(SfSymbol::new("gearshape")).to_string(),
        "sf-symbols/regular/gearshape.svg"
    );
    let symbol = SfSymbol::from_static("wifi")
        .bold()
        .hierarchical()
        .variable_value(0.5);
    assert_eq!(
        symbol.path().to_string(),
        "sf-symbols/bold+hierarchical+v50/wifi.svg"
    );
    assert_eq!(SfSymbol::from_path(&symbol.path()), Some(symbol));
    for weight in Weight::ALL {
        for rendering in [Rendering::Monochrome, Rendering::Hierarchical] {
            let symbol = SfSymbol::new("folder.badge.plus")
                .weight(weight)
                .rendering(rendering);
            assert_eq!(SfSymbol::from_path(&symbol.path()), Some(symbol));
        }
        assert_eq!(weight.as_str().parse(), Ok(weight));
    }
    for bad in [
        "icons/check.svg",
        "sf-symbols/gearshape.svg",
        "sf-symbols/bolder/gearshape.svg",
        "sf-symbols/bold+v101/wifi.svg",
        "sf-symbols/bold+v50+v50/wifi.svg",
        "sf-symbols/bold+fancy/wifi.svg",
        "sf-symbols/bold/a/b.svg",
        "sf-symbols/bold/.svg",
    ] {
        assert_eq!(SfSymbol::from_path(bad), None, "{bad}");
    }
    assert_eq!(
        SfSymbol::new("wifi")
            .variable_value(7.)
            .get_variable_value(),
        Some(1.)
    );
}

fn symbols_render_as_vectors() {
    let svg = SfSymbol::new("gearshape").svg().expect("gearshape renders");
    let svg = std::str::from_utf8(&svg).unwrap();
    assert!(svg.starts_with("<svg ") && svg.ends_with("</svg>"), "{svg}");
    assert!(svg.contains("<path d=\"M"), "no paths: {svg}");

    // Weights change the outline.
    let light = SfSymbol::new("gearshape").light().svg().unwrap();
    let black = SfSymbol::new("gearshape").black().svg().unwrap();
    assert_ne!(light, black);

    // Hierarchical layers are translucent; knocked-out badges become masks.
    let svg = SfSymbol::new("folder.badge.plus")
        .hierarchical()
        .svg()
        .unwrap();
    let svg = std::str::from_utf8(&svg).unwrap();
    assert!(svg.contains("fill-opacity="), "{svg}");
    assert!(svg.contains("<mask "), "{svg}");

    // Variable values fade the layers past the value.
    let half = SfSymbol::new("wifi").variable_value(0.4).svg().unwrap();
    let full = SfSymbol::new("wifi").svg().unwrap();
    assert!(
        std::str::from_utf8(&half)
            .unwrap()
            .contains("fill-opacity=")
    );
    assert_ne!(half, full);
}

fn unknown_symbols_render_nothing() {
    let missing = SfSymbol::new("definitely.not.a.symbol");
    assert!(!missing.exists());
    assert_eq!(missing.svg(), None);
    assert!(SfSymbol::new("gearshape").exists());
    assert!(SfSymbol::new("bad\0name").svg().is_none());
}

fn asset_source_serves_symbol_paths_only() {
    let path = SfSymbol::new("star.fill").bold().path();
    let bytes = SfSymbolAssets.load(&path).unwrap().expect("star renders");
    assert!(bytes.starts_with(b"<svg "));
    assert!(SfSymbolAssets.load("icons/check.svg").unwrap().is_none());
    assert!(SfSymbolAssets.list("").unwrap().is_empty());
}

fn off_main_thread_gives_up_instead_of_hanging() {
    // The main thread is blocked in `join`, so the render cannot run.
    let symbol = SfSymbol::new("trash.circle");
    let started = std::time::Instant::now();
    let svg = std::thread::spawn(move || symbol.svg()).join().unwrap();
    assert_eq!(svg, None);
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    // ...and the failure is not cached.
    assert!(SfSymbol::new("trash.circle").svg().is_some());
}

fn main() {
    paths_round_trip();
    println!("test paths_round_trip ... ok");
    if !is_available() {
        println!("symbols: rendering skipped; SF Symbols need macOS 13 or later");
        return;
    }
    let tests: &[(&str, fn())] = &[
        ("symbols_render_as_vectors", symbols_render_as_vectors),
        (
            "unknown_symbols_render_nothing",
            unknown_symbols_render_nothing,
        ),
        (
            "asset_source_serves_symbol_paths_only",
            asset_source_serves_symbol_paths_only,
        ),
        (
            "off_main_thread_gives_up_instead_of_hanging",
            off_main_thread_gives_up_instead_of_hanging,
        ),
    ];
    for (name, test) in tests {
        test();
        println!("test {name} ... ok");
    }
}
