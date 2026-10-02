// Generates the DMG background image (660x480) used by `make release`.
// Full-bleed: no rounded corners, no text — Finder provides the window chrome.
// Usage: swift make-dmg-background.swift <output.png>
import AppKit

let width = 660.0, height = 480.0
let image = NSImage(size: NSSize(width: width, height: height))
image.lockFocus()

let rect = NSRect(x: 0, y: 0, width: width, height: height)
NSColor(deviceRed: 0.09, green: 0.10, blue: 0.13, alpha: 1).setFill()
rect.fill()
NSGradient(starting: NSColor(deviceWhite: 0.16, alpha: 1), ending: NSColor(deviceWhite: 0.06, alpha: 1))?
    .draw(in: rect, angle: -90)

image.unlockFocus()

let tiff = image.tiffRepresentation!
let rep = NSBitmapImageRep(data: tiff)!
let png = rep.representation(using: .png, properties: [:])!
try! png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
