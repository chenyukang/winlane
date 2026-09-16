#!/usr/bin/env swift
import AppKit
import Foundation

let files = FileManager.default
let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let resources = root.appendingPathComponent("resources", isDirectory: true)
guard let artwork = NSImage(contentsOf: resources.appendingPathComponent("AppIcon.png")) else {
    fatalError("Missing resources/AppIcon.png master artwork")
}

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

// An open rear outline keeps the overlap legible without painting an opaque
// cutout, so the same vector works as a template on every menu-bar background.
func menuMark(_ ink: NSColor) {
    let rear = NSBezierPath()
    rear.move(to: NSPoint(x: 4, y: 5.5))
    rear.line(to: NSPoint(x: 3.5, y: 5.5))
    rear.curve(to: NSPoint(x: 1.5, y: 7.5),
               controlPoint1: NSPoint(x: 2.4, y: 5.5), controlPoint2: NSPoint(x: 1.5, y: 6.4))
    rear.line(to: NSPoint(x: 1.5, y: 14.5))
    rear.curve(to: NSPoint(x: 3.5, y: 16.5),
               controlPoint1: NSPoint(x: 1.5, y: 15.6), controlPoint2: NSPoint(x: 2.4, y: 16.5))
    rear.line(to: NSPoint(x: 10.5, y: 16.5))
    rear.curve(to: NSPoint(x: 12.5, y: 14.5),
               controlPoint1: NSPoint(x: 11.6, y: 16.5), controlPoint2: NSPoint(x: 12.5, y: 15.6))
    rear.line(to: NSPoint(x: 12.5, y: 14))

    let front = NSBezierPath(roundedRect: NSRect(x: 5.5, y: 1.5, width: 11, height: 11),
                             xRadius: 2, yRadius: 2)
    let titlebar = NSBezierPath()
    titlebar.move(to: NSPoint(x: 6.25, y: 9.25))
    titlebar.line(to: NSPoint(x: 15.75, y: 9.25))
    ink.setStroke()
    for path in [rear, front, titlebar] {
        path.lineWidth = 1.5
        path.lineCapStyle = .round
        path.lineJoinStyle = .round
        path.stroke()
    }
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
            artwork.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
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
    artwork.draw(in: NSRect(x: 44, y: 72, width: 336, height: 336))

    func label(_ text: String, _ x: CGFloat, _ y: CGFloat, _ size: CGFloat, _ ink: NSColor) {
        (text as NSString).draw(at: NSPoint(x: x, y: y), withAttributes: [
            .font: NSFont.systemFont(ofSize: size, weight: .medium), .foregroundColor: ink
        ])
    }
    label("Winlane", 456, 366, 36, .black)
    label("Two windows. One quick switch.", 456, 326, 20, .darkGray)

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
