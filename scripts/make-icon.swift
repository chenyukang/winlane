#!/usr/bin/env swift
import AppKit
import Foundation

// Winlane draws its identity from code so both marks stay reproducible: the
// app tile is a pack of glossy tool chips, and the menu-bar mark is the same
// pack reduced to a two-by-two grid. `resources/AppIcon.png` is written as the
// master artwork for reference; the ICNS, the menu-bar PDF and the preview
// sheet come from the same drawing code.

let files = FileManager.default
let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let resources = root.appendingPathComponent("resources", isDirectory: true)

// MARK: - palette

let amber = NSColor(srgbRed: 1.00, green: 0.73, blue: 0.20, alpha: 1)
let orange = NSColor(srgbRed: 1.00, green: 0.52, blue: 0.24, alpha: 1)
let coral = NSColor(srgbRed: 1.00, green: 0.36, blue: 0.44, alpha: 1)
let violet = NSColor(srgbRed: 0.58, green: 0.36, blue: 1.00, alpha: 1)
let azure = NSColor(srgbRed: 0.22, green: 0.55, blue: 1.00, alpha: 1)
let cyan = NSColor(srgbRed: 0.20, green: 0.78, blue: 0.96, alpha: 1)
let tileTop = NSColor(srgbRed: 0.32, green: 0.21, blue: 0.84, alpha: 1)
let tileBottom = NSColor(srgbRed: 0.06, green: 0.05, blue: 0.26, alpha: 1)

// MARK: - drawing primitives

/// The macOS app tile shape: a superellipse rather than a plain rounded rect.
func squircle(_ rect: NSRect, _ n: CGFloat = 5.0) -> NSBezierPath {
    let path = NSBezierPath()
    let a = rect.size.width / 2, b = rect.size.height / 2
    let cx = rect.midX, cy = rect.midY
    for step in 0...240 {
        let angle = CGFloat(step) / 240 * 2 * .pi
        let cosAngle = cos(angle), sinAngle = sin(angle)
        let x = cx + a * (cosAngle < 0 ? -1 : 1) * pow(abs(cosAngle), 2 / n)
        let y = cy + b * (sinAngle < 0 ? -1 : 1) * pow(abs(sinAngle), 2 / n)
        if step == 0 { path.move(to: NSPoint(x: x, y: y)) } else { path.line(to: NSPoint(x: x, y: y)) }
    }
    path.close()
    return path
}

func fourPointStar(_ center: NSPoint, _ radius: CGFloat, _ waist: CGFloat, _ color: NSColor) {
    let path = NSBezierPath()
    path.move(to: NSPoint(x: center.x, y: center.y + radius))
    for sign in [1.0, -1.0] as [CGFloat] {
        path.curve(to: NSPoint(x: center.x + sign * radius, y: center.y),
                   controlPoint1: NSPoint(x: center.x + sign * waist, y: center.y + waist),
                   controlPoint2: NSPoint(x: center.x + sign * waist, y: center.y + waist))
        path.curve(to: NSPoint(x: center.x, y: center.y - sign * radius),
                   controlPoint1: NSPoint(x: center.x + sign * waist, y: center.y - sign * waist),
                   controlPoint2: NSPoint(x: center.x + sign * waist, y: center.y - sign * waist))
    }
    path.close()
    color.setFill()
    path.fill()
}

/// One tool chip: a rounded square with a soft shadow, a single smooth gloss
/// and a crisp inner rim, so it reads as a solid object at small sizes.
func chip(_ rect: NSRect, _ color: NSColor, radiusFactor: CGFloat = 0.27, lift: CGFloat = 0) {
    let path = NSBezierPath(roundedRect: rect, xRadius: rect.width * radiusFactor, yRadius: rect.width * radiusFactor)
    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = NSColor(white: 0, alpha: 0.30 + lift * 0.10)
    shadow.shadowBlurRadius = rect.width * (0.16 + lift * 0.30)
    shadow.shadowOffset = NSSize(width: 0, height: -rect.width * (0.045 + lift * 0.09))
    shadow.set()
    color.setFill()
    path.fill()
    NSGraphicsContext.restoreGraphicsState()

    NSGraphicsContext.saveGraphicsState()
    path.addClip()
    let gloss = NSGradient(colorsAndLocations:
        (NSColor(white: 1, alpha: 0.34), 0.0),
        (NSColor(white: 1, alpha: 0.06), 0.42),
        (NSColor(white: 0, alpha: 0.0), 0.62),
        (NSColor(white: 0, alpha: 0.20), 1.0))!
    gloss.draw(in: rect, angle: 90)
    NSGraphicsContext.restoreGraphicsState()

    let rim = NSBezierPath(roundedRect: rect.insetBy(dx: rect.width * 0.026, dy: rect.width * 0.026),
                           xRadius: rect.width * (radiusFactor - 0.018),
                           yRadius: rect.width * (radiusFactor - 0.018))
    NSColor(white: 1, alpha: 0.26).setStroke()
    rim.lineWidth = rect.width * 0.026
    rim.stroke()
}

// MARK: - app tile

/// The tile: a deep violet enamel slab holding six tools, with the last one
/// lifted and starred so the pack looks alive rather than like a plain grid.
func appTile(in rect: NSRect) {
    let tile = squircle(rect)
    NSGradient(starting: tileBottom, ending: tileTop)!.draw(in: tile, angle: 58)
    NSGraphicsContext.saveGraphicsState()
    tile.addClip()
    let glow = NSBezierPath(ovalIn: rect.insetBy(dx: -rect.width * 0.12, dy: -rect.height * 0.12))
    NSGradient(starting: NSColor(white: 1, alpha: 0.15), ending: NSColor(white: 1, alpha: 0))!
        .draw(in: glow, angle: 90)
    let rim = squircle(rect.insetBy(dx: rect.width * 0.010, dy: rect.width * 0.010))
    NSColor(white: 1, alpha: 0.28).setStroke()
    rim.lineWidth = rect.width * 0.013
    rim.stroke()
    NSGraphicsContext.restoreGraphicsState()

    let width = rect.width
    let margin = width * 0.135, gap = width * 0.058
    let side = (width - margin * 2 - gap * 2) / 3
    let lift = width * 0.030, liftUp = width * 0.040
    // The lifted chip eats into the top margin, so the pack sits slightly low to
    // keep the whole arrangement optically centred in the tile.
    let originX = rect.minX + margin
    let originY = rect.minY + margin - liftUp / 2
    let colors = [violet, azure, cyan, amber, orange, coral]
    for row in 0..<2 {
        for column in 0..<3 {
            let lifted = row == 1 && column == 2
            var frame = NSRect(x: originX + CGFloat(column) * (side + gap),
                               y: originY + CGFloat(row) * (side + gap),
                               width: side, height: side)
            if lifted { frame = frame.offsetBy(dx: lift, dy: liftUp) }
            chip(frame, colors[row * 3 + column], lift: lifted ? 1 : 0)
            if lifted {
                fourPointStar(NSPoint(x: frame.maxX - width * 0.040, y: frame.maxY - width * 0.040),
                              width * 0.050, width * 0.012, NSColor(white: 1, alpha: 0.95))
            }
        }
    }
}

// MARK: - menu-bar mark

/// The pack reduced to a 18 x 18 point template: a two-by-two grid of chips
/// with the current tool filled in, which stays legible at true menu-bar size.
func menuMark(_ ink: NSColor) {
    let side: CGFloat = 6.6, gap: CGFloat = 2.2
    for row in 0..<2 {
        for column in 0..<2 {
            let rect = NSRect(x: 1.4 + CGFloat(column) * (side + gap),
                              y: 1.4 + CGFloat(row) * (side + gap),
                              width: side, height: side)
            let path = NSBezierPath(roundedRect: rect, xRadius: side * 0.30, yRadius: side * 0.30)
            if row == 1 && column == 1 {
                ink.setFill()
                path.fill()
            } else {
                path.lineWidth = 1.5
                ink.setStroke()
                path.stroke()
            }
        }
    }
}

// MARK: - outputs

func bitmap(width: Int, height: Int, draw: () -> Void) -> Data {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true,
        isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = context
    context.cgContext.clear(CGRect(x: 0, y: 0, width: width, height: height))
    context.imageInterpolation = .high
    context.shouldAntialias = true
    draw()
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

let iconset = files.temporaryDirectory.appendingPathComponent("winlane-\(UUID().uuidString).iconset")
try files.createDirectory(at: iconset, withIntermediateDirectories: true)
defer { try? files.removeItem(at: iconset) }
for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let size = points * scale
        let suffix = scale == 2 ? "@2x" : ""
        let destination = iconset.appendingPathComponent("icon_\(points)x\(points)\(suffix).png")
        try bitmap(width: size, height: size) {
            appTile(in: NSRect(x: 0, y: 0, width: size, height: size))
        }.write(to: destination)
    }
}
let output = resources.appendingPathComponent("AppIcon.icns")
let converter = Process()
converter.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
converter.arguments = ["-c", "icns", "-o", output.path, iconset.path]
try converter.run()
converter.waitUntilExit()
guard converter.terminationStatus == 0 else { fatalError("iconutil failed") }

// Master artwork, kept for reference and for anything that wants a PNG.
try bitmap(width: 1024, height: 1024) {
    appTile(in: NSRect(x: 0, y: 0, width: 1024, height: 1024))
}.write(to: resources.appendingPathComponent("AppIcon.png"))

let menuOutput = resources.appendingPathComponent("MenuBarIconTemplate.pdf")
var page = CGRect(x: 0, y: 0, width: 18, height: 18)
guard let pdf = CGContext(menuOutput as CFURL, mediaBox: &page, nil) else {
    fatalError("Cannot create menu-bar PDF")
}
pdf.beginPDFPage(nil)
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(cgContext: pdf, flipped: false)
menuMark(.black)
NSGraphicsContext.restoreGraphicsState()
pdf.endPDFPage()
pdf.closePDF()

// This sheet shows the vector at Retina menu-bar size beside the app artwork.
let preview = root.appendingPathComponent("docs/images/icon-preview.png")
try files.createDirectory(at: preview.deletingLastPathComponent(), withIntermediateDirectories: true)
try bitmap(width: 1120, height: 480) {
    NSColor(srgbRed: 0.95, green: 0.96, blue: 0.98, alpha: 1).setFill()
    NSRect(x: 0, y: 0, width: 1120, height: 480).fill()
    appTile(in: NSRect(x: 44, y: 72, width: 336, height: 336))

    func label(_ text: String, _ x: CGFloat, _ y: CGFloat, _ size: CGFloat, _ ink: NSColor) {
        (text as NSString).draw(at: NSPoint(x: x, y: y), withAttributes: [
            .font: NSFont.systemFont(ofSize: size, weight: .medium), .foregroundColor: ink
        ])
    }
    label("Winlane", 456, 366, 36, .black)
    label("One shortcut. Every tool.", 456, 326, 20, .darkGray)

    for (y, dark) in [(CGFloat(220), false), (CGFloat(126), true)] {
        let frame = NSRect(x: 456, y: y, width: 612, height: 66)
        let fill = dark ? NSColor(srgbRed: 0.10, green: 0.12, blue: 0.17, alpha: 1) : .white
        fill.setFill()
        NSBezierPath(roundedRect: frame, xRadius: 16, yRadius: 16).fill()
        let ink: NSColor = dark ? .white : .black
        label(dark ? "Dark menu bar" : "Light menu bar", 480, y + 22, 16, ink)
        NSGraphicsContext.saveGraphicsState()
        let transform = NSAffineTransform()
        transform.translateX(by: 834, yBy: y + 15)
        transform.scale(by: 2)
        transform.concat()
        menuMark(ink)
        NSGraphicsContext.restoreGraphicsState()
        label("09:41", 964, y + 20, 18, ink)
    }
    label("Native template icon · Retina-ready", 456, 78, 15, .gray)
}.write(to: preview)
print(output.path)
print(menuOutput.path)
print(preview.path)
