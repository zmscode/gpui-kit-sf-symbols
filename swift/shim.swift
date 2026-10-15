// Renders SF Symbols to SVG for gpui-kit-sf-symbols.
//
// SwiftUI's `ImageRenderer` draws symbols as vector paths when given a PDF
// context, unlike AppKit, which rasterizes them. The PDF content stream is
// walked with `CGPDFScanner` and its fills are rewritten as SVG paths, so the
// result can be tinted like any other GPUI icon.
//
// Nothing here is bundled: the artwork comes from the running system.

import AppKit
import Foundation
import SwiftUI

/// Point size the symbol is rendered at. Only affects coordinate precision;
/// the SVG scales freely.
private let pointSize: CGFloat = 100

/// Smallest view box side, in points at `pointSize`. Typical symbols fit
/// inside it, so they come out at the same optical size; larger ones shrink
/// to fit.
private let minSide: CGFloat = 120

private let weights: [Font.Weight] = [
	.ultraLight, .thin, .light, .regular, .medium, .semibold, .bold, .heavy, .black,
]

// MARK: - PDF walking

private struct GraphicsState {
	var ctm = CGAffineTransform.identity
	var alpha: CGFloat = 1
	/// Luminance of the fill colour; only matters inside soft masks.
	var gray: CGFloat = 0
	/// Components taken by `sc`, from the fill colour space set by `cs`.
	var components = 1
	/// Id of the SVG `<mask>` standing in for the current PDF soft mask.
	var mask: Int?
}

private struct Fill {
	var d: String
	var evenOdd: Bool
	var alpha: CGFloat
	var gray: CGFloat
	var mask: Int?
}

private final class Walker {
	var state = GraphicsState()
	var stack: [GraphicsState] = []
	var path = ""
	var pathBounds = CGRect.null
	var current = CGPoint.zero
	var fills: [Fill] = []
	var masks: [[Fill]] = []
	var bounds = CGRect.null
	/// Whether fills are being collected for a soft mask rather than the symbol.
	var inMask = false

	func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
		CGPoint(x: x, y: y).applying(state.ctm)
	}

	func append(_ command: String, _ points: [CGPoint]) {
		path += command
		for p in points {
			pathBounds = pathBounds.union(CGRect(origin: p, size: .zero))
			path += " \(fmt(p.x)) \(fmt(p.y))"
		}
	}

	func discardPath() {
		path = ""
		pathBounds = .null
	}

	func fill(evenOdd: Bool) {
		if !path.isEmpty && state.alpha > 0 {
			fills.append(
				Fill(
					d: path, evenOdd: evenOdd, alpha: state.alpha, gray: state.gray,
					mask: state.mask))
			if !inMask { bounds = bounds.union(pathBounds) }
		}
		discardPath()
	}

	/// Draws a form XObject, as the `Do` operator or a soft mask's group does.
	func drawForm(_ form: CGPDFStreamRef, parent: CGPDFContentStreamRef?) {
		guard let dict = CGPDFStreamGetDictionary(form) else { return }
		var subtype: UnsafePointer<CChar>?
		guard CGPDFDictionaryGetName(dict, "Subtype", &subtype), let subtype,
			String(cString: subtype) == "Form"
		else { return }
		let saved = state
		var matrix: CGPDFArrayRef?
		if CGPDFDictionaryGetArray(dict, "Matrix", &matrix), let matrix,
			CGPDFArrayGetCount(matrix) == 6
		{
			var v = [CGPDFReal](repeating: 0, count: 6)
			for i in 0..<6 { CGPDFArrayGetNumber(matrix, i, &v[i]) }
			state.ctm = CGAffineTransform(a: v[0], b: v[1], c: v[2], d: v[3], tx: v[4], ty: v[5])
				.concatenating(state.ctm)
		}
		var resources: CGPDFDictionaryRef?
		_ = CGPDFDictionaryGetDictionary(dict, "Resources", &resources)
		guard let resources, let parent else {
			state = saved
			return
		}
		let stream = CGPDFContentStreamCreateWithStream(form, resources, parent)
		scan(stream)
		CGPDFContentStreamRelease(stream)
		state = saved
	}

	func scan(_ stream: CGPDFContentStreamRef) {
		let scanner = CGPDFScannerCreate(
			stream, operatorTable, Unmanaged.passUnretained(self).toOpaque())
		CGPDFScannerScan(scanner)
		CGPDFScannerRelease(scanner)
	}

	/// Collects a luminosity soft mask's group as an SVG mask; returns its id.
	func softMask(_ group: CGPDFStreamRef, parent: CGPDFContentStreamRef?) -> Int {
		let (savedFills, savedInMask, savedState, savedStack) = (fills, inMask, state, stack)
		fills = []
		inMask = true
		state = GraphicsState(ctm: state.ctm)
		stack = []
		drawForm(group, parent: parent)
		masks.append(fills)
		(fills, inMask, state, stack) = (savedFills, savedInMask, savedState, savedStack)
		return masks.count - 1
	}
}

private func fmt(_ value: CGFloat) -> String {
	// Coordinates are in points at `pointSize`; hundredths are plenty.
	var s = String(format: "%.2f", value)
	while s.hasSuffix("0") { s.removeLast() }
	if s.hasSuffix(".") { s.removeLast() }
	return s == "-0" ? "0" : s
}

private func walker(_ info: UnsafeMutableRawPointer?) -> Walker {
	Unmanaged<Walker>.fromOpaque(info!).takeUnretainedValue()
}

private func pop(_ scanner: CGPDFScannerRef, _ count: Int) -> [CGFloat] {
	var values = [CGFloat](repeating: 0, count: count)
	for i in (0..<count).reversed() {
		var v: CGPDFReal = 0
		CGPDFScannerPopNumber(scanner, &v)
		values[i] = v
	}
	return values
}

/// Number of colour components in a colour space operand of `cs`.
private func componentCount(_ scanner: CGPDFScannerRef, _ name: String) -> Int {
	switch name {
	case "DeviceGray", "CalGray": return 1
	case "DeviceRGB", "CalRGB", "Lab": return 3
	case "DeviceCMYK": return 4
	default: break
	}
	// A named resource, typically `[/ICCBased stream]` with `/N` components.
	let stream = CGPDFScannerGetContentStream(scanner)
	var array: CGPDFArrayRef?
	var profile: CGPDFStreamRef?
	var count: CGPDFInteger = 3
	if let object = CGPDFContentStreamGetResource(stream, "ColorSpace", name),
		CGPDFObjectGetValue(object, .array, &array), let array,
		CGPDFArrayGetStream(array, 1, &profile), let profile,
		let dict = CGPDFStreamGetDictionary(profile)
	{
		CGPDFDictionaryGetInteger(dict, "N", &count)
	}
	return Int(count)
}

private func luminance(_ c: [CGFloat]) -> CGFloat {
	switch c.count {
	case 1: return c[0]
	case 3: return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
	case 4:
		return (1 - c[0]) * (1 - c[3]) * 0.3 + (1 - c[1]) * (1 - c[3]) * 0.59 + (1 - c[2])
			* (1 - c[3]) * 0.11
	default: return 0
	}
}

private func makeTable() -> CGPDFOperatorTableRef {
	let table = CGPDFOperatorTableCreate()!
	func on(_ op: String, _ callback: CGPDFOperatorCallback) {
		CGPDFOperatorTableSetCallback(table, op, callback)
	}
	on("q") { _, info in
		let w = walker(info)
		w.stack.append(w.state)
	}
	on("Q") { _, info in
		let w = walker(info)
		if let s = w.stack.popLast() { w.state = s }
	}
	on("cm") { scanner, info in
		let v = pop(scanner, 6)
		let w = walker(info)
		w.state.ctm = CGAffineTransform(a: v[0], b: v[1], c: v[2], d: v[3], tx: v[4], ty: v[5])
			.concatenating(w.state.ctm)
	}
	on("cs") { scanner, info in
		var name: UnsafePointer<CChar>?
		guard CGPDFScannerPopName(scanner, &name), let name else { return }
		let w = walker(info)
		w.state.components = componentCount(scanner, String(cString: name))
		w.state.gray = 0
	}
	let color: CGPDFOperatorCallback = { scanner, info in
		let w = walker(info)
		w.state.gray = luminance(pop(scanner, w.state.components))
	}
	on("sc", color)
	on("scn", color)
	on("g") { scanner, info in
		let w = walker(info)
		w.state.components = 1
		w.state.gray = luminance(pop(scanner, 1))
	}
	on("rg") { scanner, info in
		let w = walker(info)
		w.state.components = 3
		w.state.gray = luminance(pop(scanner, 3))
	}
	on("k") { scanner, info in
		let w = walker(info)
		w.state.components = 4
		w.state.gray = luminance(pop(scanner, 4))
	}
	on("gs") { scanner, info in
		var name: UnsafePointer<CChar>?
		guard CGPDFScannerPopName(scanner, &name), let name else { return }
		let stream = CGPDFScannerGetContentStream(scanner)
		guard let object = CGPDFContentStreamGetResource(stream, "ExtGState", name) else { return }
		var dict: CGPDFDictionaryRef?
		guard CGPDFObjectGetValue(object, .dictionary, &dict), let dict else { return }
		let w = walker(info)
		var alpha: CGPDFReal = 1
		if CGPDFDictionaryGetNumber(dict, "ca", &alpha) {
			w.state.alpha = alpha
		}
		var maskName: UnsafePointer<CChar>?
		var maskDict: CGPDFDictionaryRef?
		var group: CGPDFStreamRef?
		if CGPDFDictionaryGetName(dict, "SMask", &maskName) {
			w.state.mask = nil  // `/SMask /None`
		} else if CGPDFDictionaryGetDictionary(dict, "SMask", &maskDict), let maskDict,
			CGPDFDictionaryGetStream(maskDict, "G", &group), let group
		{
			w.state.mask = w.softMask(group, parent: stream)
		}
	}
	on("Do") { scanner, info in
		var name: UnsafePointer<CChar>?
		guard CGPDFScannerPopName(scanner, &name), let name else { return }
		let stream = CGPDFScannerGetContentStream(scanner)
		guard let object = CGPDFContentStreamGetResource(stream, "XObject", name) else { return }
		var form: CGPDFStreamRef?
		guard CGPDFObjectGetValue(object, .stream, &form), let form else { return }
		walker(info).drawForm(form, parent: stream)
	}
	on("m") { scanner, info in
		let v = pop(scanner, 2)
		let w = walker(info)
		w.current = CGPoint(x: v[0], y: v[1])
		w.append("M", [w.point(v[0], v[1])])
	}
	on("l") { scanner, info in
		let v = pop(scanner, 2)
		let w = walker(info)
		w.current = CGPoint(x: v[0], y: v[1])
		w.append("L", [w.point(v[0], v[1])])
	}
	on("c") { scanner, info in
		let v = pop(scanner, 6)
		let w = walker(info)
		w.current = CGPoint(x: v[4], y: v[5])
		w.append("C", [w.point(v[0], v[1]), w.point(v[2], v[3]), w.point(v[4], v[5])])
	}
	on("v") { scanner, info in
		let v = pop(scanner, 4)
		let w = walker(info)
		let c = w.current
		w.current = CGPoint(x: v[2], y: v[3])
		w.append("C", [w.point(c.x, c.y), w.point(v[0], v[1]), w.point(v[2], v[3])])
	}
	on("y") { scanner, info in
		let v = pop(scanner, 4)
		let w = walker(info)
		w.current = CGPoint(x: v[2], y: v[3])
		w.append("C", [w.point(v[0], v[1]), w.point(v[2], v[3]), w.point(v[2], v[3])])
	}
	on("h") { _, info in walker(info).path += "Z" }
	on("re") { scanner, info in
		let v = pop(scanner, 4)
		let w = walker(info)
		let (x, y, width, height) = (v[0], v[1], v[2], v[3])
		w.append("M", [w.point(x, y)])
		w.append(
			"L", [w.point(x + width, y), w.point(x + width, y + height), w.point(x, y + height)])
		w.path += "Z"
	}
	on("f") { _, info in walker(info).fill(evenOdd: false) }
	on("F") { _, info in walker(info).fill(evenOdd: false) }
	on("f*") { _, info in walker(info).fill(evenOdd: true) }
	// Clipping paths and anything stroked are not part of the symbol's shape.
	on("n") { _, info in walker(info).discardPath() }
	on("S") { _, info in walker(info).discardPath() }
	return table
}

private let operatorTable = makeTable()

private func pathElement(_ fill: Fill, colored: Bool) -> String {
	var svg = "<path d=\"\(fill.d)\""
	if fill.evenOdd { svg += " fill-rule=\"evenodd\"" }
	if colored {
		let level = Int((fill.gray * 255).rounded())
		svg += " fill=\"rgb(\(level),\(level),\(level))\""
	}
	if fill.alpha < 1 { svg += " fill-opacity=\"\(fmt(fill.alpha))\"" }
	if let mask = fill.mask { svg += " mask=\"url(#m\(mask))\"" }
	return svg + "/>"
}

// MARK: - Rendering

@available(macOS 13.0, *)
@MainActor
private func renderSVG(name: String, weight: Int, hierarchical: Bool, variableValue: Double?)
	-> String?
{
	// SwiftUI silently draws nothing for unknown names.
	guard NSImage(systemSymbolName: name, accessibilityDescription: nil) != nil else { return nil }

	let image = Image(systemName: name, variableValue: variableValue)
	let content =
		image
		.font(.system(size: pointSize, weight: weights[min(max(weight, 0), weights.count - 1)]))
		.symbolRenderingMode(hierarchical ? .hierarchical : .monochrome)
		.foregroundStyle(Color.black)
	let renderer = ImageRenderer(content: content)

	let data = NSMutableData()
	var frame = CGRect.zero
	renderer.render { size, draw in
		frame = CGRect(origin: .zero, size: size)
		var box = frame
		guard let consumer = CGDataConsumer(data: data as CFMutableData),
			let context = CGContext(consumer: consumer, mediaBox: &box, nil)
		else { return }
		context.beginPDFPage(nil)
		draw(context)
		context.endPDFPage()
		context.closePDF()
	}
	guard frame.width > 0, frame.height > 0,
		let provider = CGDataProvider(data: data as CFData),
		let document = CGPDFDocument(provider),
		let page = document.page(at: 1)
	else { return nil }

	let walker = Walker()
	// PDF space is y-up; SVG is y-down.
	walker.state.ctm = CGAffineTransform(a: 1, b: 0, c: 0, d: -1, tx: 0, ty: frame.height)
	let stream = CGPDFContentStreamCreateWithPage(page)
	walker.scan(stream)
	CGPDFContentStreamRelease(stream)

	// A square view box centred on the symbol's frame, at least `minSide`
	// across so that symbols share an optical size.
	let extent = frame.union(walker.bounds.isNull ? frame : walker.bounds)
	let side = max(extent.width, extent.height, minSide)
	let box = CGRect(
		x: extent.midX - side / 2, y: extent.midY - side / 2, width: side, height: side)

	var svg =
		"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"\(fmt(box.minX)) \(fmt(box.minY)) \(fmt(side)) \(fmt(side))\" width=\"\(fmt(side))\" height=\"\(fmt(side))\">"
	if !walker.masks.isEmpty {
		// PDF luminosity soft masks with a white backdrop.
		let region =
			"x=\"\(fmt(box.minX))\" y=\"\(fmt(box.minY))\" width=\"\(fmt(side))\" height=\"\(fmt(side))\""
		svg += "<defs>"
		for (id, fills) in walker.masks.enumerated() {
			svg +=
				"<mask id=\"m\(id)\" maskUnits=\"userSpaceOnUse\" \(region)><rect \(region) fill=\"#fff\"/>"
			for fill in fills { svg += pathElement(fill, colored: true) }
			svg += "</mask>"
		}
		svg += "</defs>"
	}
	for fill in walker.fills { svg += pathElement(fill, colored: false) }
	svg += "</svg>"
	return svg
}

// MARK: - C ABI

/// Renders `name` as an SVG document. Returns a `malloc`ed, NUL-free buffer
/// of `*length` bytes (free it with `gpui_sf_symbol_free`), or null when the
/// system has no such symbol.
///
/// Main thread only, as SwiftUI requires; the Rust side hops there. That hop
/// deliberately avoids Swift concurrency (`MainActor.assumeIsolated` and
/// friends): Rust links for macOS 11, where the concurrency runtime is an
/// `@rpath` library that a library crate cannot add a search path for, so
/// its symbols would resolve to null.
@MainActor
@_cdecl("gpui_sf_symbol_svg")
public func gpui_sf_symbol_svg(
	_ name: UnsafePointer<CChar>,
	_ weight: Int32,
	_ hierarchical: Bool,
	_ variableValue: Double,
	_ length: UnsafeMutablePointer<Int>
) -> UnsafeMutablePointer<UInt8>? {
	// `ImageRenderer` arrived in macOS 13.
	guard #available(macOS 13.0, *) else { return nil }
	guard
		let svg = renderSVG(
			name: String(cString: name), weight: Int(weight), hierarchical: hierarchical,
			variableValue: variableValue.isNaN ? nil : variableValue)
	else { return nil }
	let bytes = Array(svg.utf8)
	let buffer = malloc(max(bytes.count, 1))!.assumingMemoryBound(to: UInt8.self)
	bytes.withUnsafeBufferPointer { buffer.update(from: $0.baseAddress!, count: bytes.count) }
	length.pointee = bytes.count
	return buffer
}

@_cdecl("gpui_sf_symbol_free")
public func gpui_sf_symbol_free(_ buffer: UnsafeMutablePointer<UInt8>?) {
	free(buffer)
}

/// Whether the system knows a symbol called `name`.
@_cdecl("gpui_sf_symbol_exists")
public func gpui_sf_symbol_exists(_ name: UnsafePointer<CChar>) -> Bool {
	NSImage(systemSymbolName: String(cString: name), accessibilityDescription: nil) != nil
}

/// Whether symbols can be rendered on this system (macOS 13 or later).
@_cdecl("gpui_sf_symbol_available")
public func gpui_sf_symbol_available() -> Bool {
	if #available(macOS 13.0, *) { return true }
	return false
}
