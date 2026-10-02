// Generates the DMG background image (660x400) used by `make release`.
// Usage: swift make-dmg-background.swift <output.png>
import AppKit

let width = 660.0, height = 400.0
let image = NSImage(size: NSSize(width: width, height: height))
image.lockFocus()

let panel = NSBezierPath(roundedRect: NSRect(x: 0, y: 0, width: width, height: height), xRadius: 20, yRadius: 20)
NSColor(deviceRed: 0.09, green: 0.10, blue: 0.13, alpha: 1).setFill()
panel.fill()
NSGradient(starting: NSColor(deviceWhite: 0.16, alpha: 1), ending: NSColor(deviceWhite: 0.07, alpha: 1))?
    .draw(in: panel, angle: -90)

let arrowY = 215.0
let arrowStart = CGFloat(225)
let arrowTip = CGFloat(435)
let shaft = NSBezierPath()
shaft.lineWidth = 6
shaft.lineCapStyle = .round
shaft.move(to: NSPoint(x: arrowStart, y: arrowY))
shaft.line(to: NSPoint(x: arrowTip - 18, y: arrowY))
NSColor(deviceWhite: 0.85, alpha: 1).setStroke()
shaft.stroke()

let head = NSBezierPath()
head.move(to: NSPoint(x: arrowTip - 26, y: arrowY + 16))
head.line(to: NSPoint(x: arrowTip - 6, y: arrowY))
head.line(to: NSPoint(x: arrowTip - 26, y: arrowY - 16))
head.close()
NSColor(deviceWhite: 0.85, alpha: 1).setFill()
head.fill()

let centered = NSMutableParagraphStyle()
centered.alignment = .center
let title: [NSAttributedString.Key: Any] = [
    .font: NSFont.systemFont(ofSize: 26, weight: .semibold),
    .foregroundColor: NSColor(deviceWhite: 0.95, alpha: 1),
    .paragraphStyle: centered,
]
NSAttributedString(string: "Drag IDE into Applications", attributes: title)
    .draw(in: NSRect(x: 0, y: 120, width: width, height: 40))

let caption: [NSAttributedString.Key: Any] = [
    .font: NSFont.systemFont(ofSize: 15),
    .foregroundColor: NSColor(deviceWhite: 0.55, alpha: 1),
    .paragraphStyle: centered,
]
NSAttributedString(string: "License files are included next to the app", attributes: caption)
    .draw(in: NSRect(x: 0, y: 62, width: width, height: 24))

image.unlockFocus()

let tiff = image.tiffRepresentation!
let rep = NSBitmapImageRep(data: tiff)!
let png = rep.representation(using: .png, properties: [:])!
try! png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
