// Generates the DMG background image (660x400) used by `make release`.
// Usage: swift make-dmg-background.swift <output.png>
import AppKit

let width = 660.0, height = 400.0
let image = NSImage(size: NSSize(width: width, height: height))
image.lockFocus()

let panel = NSBezierPath(roundedRect: NSRect(x: 0, y: 0, width: width, height: height), xRadius: 20, yRadius: 20)
NSColor(deviceRed: 0.09, green: 0.10, blue: 0.13, alpha: 1).setFill()
panel.fill()
NSGradient(starting: NSColor(deviceWhite: 0.15, alpha: 1), ending: NSColor(deviceWhite: 0.07, alpha: 1))?
    .draw(in: panel, angle: -90)

let centered = NSMutableParagraphStyle()
centered.alignment = .center
let caption: [NSAttributedString.Key: Any] = [
    .font: NSFont.systemFont(ofSize: 15),
    .foregroundColor: NSColor(deviceWhite: 0.45, alpha: 1),
    .paragraphStyle: centered,
]
NSAttributedString(string: "Drag IDE into the Applications folder to install", attributes: caption)
    .draw(in: NSRect(x: 0, y: 18, width: width, height: 22))

image.unlockFocus()

let tiff = image.tiffRepresentation!
let rep = NSBitmapImageRep(data: tiff)!
let png = rep.representation(using: .png, properties: [:])!
try! png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
