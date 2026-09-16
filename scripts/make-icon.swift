#!/usr/bin/env swift
import AppKit
import Foundation

func color(_ hex: UInt32, alpha: CGFloat = 1) -> NSColor {
    NSColor(srgbRed: CGFloat((hex >> 16) & 255) / 255,
            green: CGFloat((hex >> 8) & 255) / 255,
            blue: CGFloat(hex & 255) / 255, alpha: alpha)
}

func rounded(_ x: CGFloat, _ y: CGFloat, _ width: CGFloat,
             _ height: CGFloat, _ radius: CGFloat) -> NSBezierPath {
    NSBezierPath(roundedRect: NSRect(x: x, y: y, width: width, height: height),
                 xRadius: radius, yRadius: radius)
}

func fill(_ path: NSBezierPath, _ value: NSColor) {
    value.setFill()
    path.fill()
}

func windowGlyph(_ x: CGFloat, _ y: CGFloat, _ width: CGFloat,
                 _ height: CGFloat, _ ink: NSColor, compact: Bool) {
    let outline = rounded(x, y, width, height, width * 0.17)
    ink.setStroke()
    outline.lineWidth = compact ? 11 : 6
    outline.stroke()
    if !compact {
        let separator = NSBezierPath()
        separator.move(to: NSPoint(x: x, y: y + height * 0.69))
        separator.line(to: NSPoint(x: x + width, y: y + height * 0.69))
        separator.lineWidth = 5
        separator.stroke()
    }
}

func png(size: Int) -> Data {
    let bitmap = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true,
        isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    let context = NSGraphicsContext(bitmapImageRep: bitmap)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = context
    context.cgContext.clear(CGRect(x: 0, y: 0, width: size, height: size))
    context.shouldAntialias = true
    let transform = NSAffineTransform()
    transform.scale(by: CGFloat(size) / 1024)
    transform.concat()
    let compact = size <= 64

    let tile = rounded(64, 64, 896, 896, 204)
    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = color(0x000814, alpha: 0.32)
    shadow.shadowBlurRadius = 28
    shadow.shadowOffset = NSSize(width: 0, height: -18)
    shadow.set()
    fill(tile, color(0x10273F))
    NSGraphicsContext.restoreGraphicsState()
    NSGradient(starting: color(0x091929), ending: color(0x193B59))!
        .draw(in: tile, angle: 90)
    color(0xB2D9E9, alpha: 0.17).setStroke()
    tile.lineWidth = 2
    tile.stroke()

    for y: CGFloat in [257, 655] {
        let row = rounded(214, y, 596, 114, 33)
        fill(row, color(0xB4D0E3, alpha: 0.095))
        color(0xB4D0E3, alpha: 0.1).setStroke()
        row.lineWidth = 2
        row.stroke()
        windowGlyph(257, y + 33, 56, 49, color(0x9BB6CC, alpha: 0.8), compact: compact)
        fill(rounded(345, y + 63, 227, compact ? 20 : 14, 7), color(0xBBD1E1, alpha: 0.72))
        if !compact {
            fill(rounded(345, y + 34, 147, 11, 6), color(0x8DAAC2, alpha: 0.48))
        }
    }

    let selected = rounded(152, 424, 720, 176, 47)
    NSGraphicsContext.saveGraphicsState()
    let rowShadow = NSShadow()
    rowShadow.shadowColor = color(0x000E18, alpha: 0.4)
    rowShadow.shadowBlurRadius = 24
    rowShadow.shadowOffset = NSSize(width: 0, height: -12)
    rowShadow.set()
    fill(selected, color(0x78EDCC))
    NSGraphicsContext.restoreGraphicsState()
    NSGradient(starting: color(0x60DCBA), ending: color(0x91F2D6))!
        .draw(in: selected, angle: 90)
    color(0xD4FFF0, alpha: 0.4).setStroke()
    selected.lineWidth = 2
    selected.stroke()

    let ink = color(0x0D3C3D)
    windowGlyph(215, 475, 84, 74, ink, compact: compact)
    fill(rounded(336, 526, 227, compact ? 25 : 19, 9), ink)
    if !compact {
        fill(rounded(336, 482, 156, 14, 7), color(0x245D58, alpha: 0.64))
    }
    let arrow = NSBezierPath()
    arrow.move(to: NSPoint(x: 707, y: 512))
    arrow.line(to: NSPoint(x: 789, y: 512))
    arrow.move(to: NSPoint(x: 758, y: 544))
    arrow.line(to: NSPoint(x: 790, y: 512))
    arrow.line(to: NSPoint(x: 758, y: 480))
    arrow.lineWidth = compact ? 25 : 16
    arrow.lineCapStyle = .round
    arrow.lineJoinStyle = .round
    ink.setStroke()
    arrow.stroke()

    NSGraphicsContext.restoreGraphicsState()
    return bitmap.representation(using: .png, properties: [:])!
}

let files = FileManager.default
let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let resources = root.appendingPathComponent("resources", isDirectory: true)
try files.createDirectory(at: resources, withIntermediateDirectories: true)
let iconset = files.temporaryDirectory.appendingPathComponent("winlane-\(UUID().uuidString).iconset")
try files.createDirectory(at: iconset, withIntermediateDirectories: true)
for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let suffix = scale == 2 ? "@2x" : ""
        let destination = iconset.appendingPathComponent("icon_\(points)x\(points)\(suffix).png")
        try png(size: points * scale).write(to: destination)
    }
}
let preview = resources.appendingPathComponent("AppIcon.png")
try png(size: 1024).write(to: preview)
let output = resources.appendingPathComponent("AppIcon.icns")
let converter = Process()
converter.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
converter.arguments = ["-c", "icns", "-o", output.path, iconset.path]
try converter.run()
converter.waitUntilExit()
guard converter.terminationStatus == 0 else {
    fputs("iconutil failed; generated PNGs are available at \(iconset.path)\n", stderr)
    exit(1)
}
try files.removeItem(at: iconset)
print(output.path)
print(preview.path)
