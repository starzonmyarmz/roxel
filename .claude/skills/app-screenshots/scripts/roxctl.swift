// Synthetic input + window lookup for driving Roxel during screenshots.
// Build: swiftc -O -o <bin> roxctl.swift
//
//   roxctl win                          -> "<id> <x> <y> <w> <h>" of Roxel's main window (points)
//   roxctl dialog                       exit 0 if a save/open panel is open
//   roxctl move <x> <y>
//   roxctl click <x> <y>
//   roxctl scroll <x> <y> <dy> [steps]  pixel-unit wheel; dy<0 zooms out
//   roxctl orbit <x> <y> <dx> <dy>      right-drag
//   roxctl pan <x> <y> <dx> <dy>        Space + left-drag
//   roxctl zclick <x> <y> [alt]         Z + left-click (zoom 2x toward point)
//   roxctl key <keycode> [cmd] [shift]  one key press
//   roxctl type <text>                  type text into the focused field
// Every command but `win` exits 3 without sending input unless Roxel is frontmost.
import AppKit
import CoreGraphics
import Foundation

let a = CommandLine.arguments

if a[1] != "win", a[1] != "dialog", NSWorkspace.shared.frontmostApplication?.localizedName != "Roxel" {
    let front = NSWorkspace.shared.frontmostApplication?.localizedName ?? "unknown"
    FileHandle.standardError.write("Roxel is not frontmost (\(front) has focus); refusing to send input.\n".data(using: .utf8)!)
    exit(3)
}
func d(_ i: Int) -> Double { Double(a[i])! }
func pt(_ i: Int) -> CGPoint { CGPoint(x: d(i), y: d(i + 1)) }
func post(_ e: CGEvent?) { e?.post(tap: .cghidEventTap) }
func mouse(_ t: CGEventType, _ p: CGPoint, _ b: CGMouseButton = .left, flags: CGEventFlags = []) {
    let e = CGEvent(mouseEventSource: nil, mouseType: t, mouseCursorPosition: p, mouseButton: b)
    e?.flags = flags
    post(e)
}
func key(_ code: CGKeyCode, _ down: Bool, flags: CGEventFlags = []) {
    let e = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down)
    e?.flags = flags
    post(e)
}
func drag(_ from: CGPoint, _ dx: Double, _ dy: Double, button: CGMouseButton) {
    let (downT, dragT, upT): (CGEventType, CGEventType, CGEventType) = button == .right
        ? (.rightMouseDown, .rightMouseDragged, .rightMouseUp)
        : (.leftMouseDown, .leftMouseDragged, .leftMouseUp)
    mouse(.mouseMoved, from); usleep(150_000)
    mouse(downT, from, button); usleep(80_000)
    let steps = max(10, Int(max(abs(dx), abs(dy)) / 6))
    var last = from
    for i in 1...steps {
        let f = Double(i) / Double(steps)
        let p = CGPoint(x: (from.x + dx * f).rounded(), y: (from.y + dy * f).rounded())
        // Bevy's camera reads raw motion deltas, not positions.
        let e = CGEvent(mouseEventSource: nil, mouseType: dragT, mouseCursorPosition: p, mouseButton: button)
        e?.setIntegerValueField(.mouseEventDeltaX, value: Int64(p.x - last.x))
        e?.setIntegerValueField(.mouseEventDeltaY, value: Int64(p.y - last.y))
        post(e)
        last = p
        usleep(16_000)
    }
    usleep(120_000)
    mouse(upT, CGPoint(x: from.x + dx, y: from.y + dy), button)
}

switch a[1] {
case "win", "dialog":
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
    let wins = list.compactMap { w -> (Int, [String: Double])? in
        guard (w[kCGWindowOwnerName as String] as? String) == "Roxel",
              (w[kCGWindowLayer as String] as? Int) == 0,
              let id = w[kCGWindowNumber as String] as? Int,
              let b = w[kCGWindowBounds as String] as? [String: Double] else { return nil }
        return (id, b)
    }.sorted { $0.1["Width"]! * $0.1["Height"]! > $1.1["Width"]! * $1.1["Height"]! }
    if a[1] == "dialog" { exit(wins.count > 1 ? 0 : 1) }  // a save/open panel is open
    guard let (id, b) = wins.first else { exit(1) }
    print(id, Int(b["X"]!), Int(b["Y"]!), Int(b["Width"]!), Int(b["Height"]!))
case "move":
    mouse(.mouseMoved, pt(2))
case "click":
    mouse(.mouseMoved, pt(2)); usleep(150_000)
    mouse(.leftMouseDown, pt(2)); usleep(80_000)
    mouse(.leftMouseUp, pt(2))
case "scroll":
    let p = pt(2)
    mouse(.mouseMoved, p); usleep(150_000)
    for _ in 0..<(a.count > 5 ? Int(a[5])! : 1) {
        let e = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1, wheel1: Int32(a[4])!, wheel2: 0, wheel3: 0)
        e?.location = p
        post(e)
        usleep(60_000)
    }
case "orbit":
    drag(pt(2), d(4), d(5), button: .right)
case "pan":
    mouse(.mouseMoved, pt(2)); usleep(100_000)
    key(49, true); usleep(150_000)
    drag(pt(2), d(4), d(5), button: .left)
    usleep(100_000); key(49, false)
case "zclick":
    let alt = a.count > 4
    mouse(.mouseMoved, pt(2)); usleep(100_000)
    if alt { key(58, true, flags: .maskAlternate) }
    key(6, true, flags: alt ? .maskAlternate : []); usleep(150_000)
    mouse(.leftMouseDown, pt(2), flags: alt ? .maskAlternate : []); usleep(80_000)
    mouse(.leftMouseUp, pt(2), flags: alt ? .maskAlternate : []); usleep(150_000)
    key(6, false); if alt { key(58, false) }
case "type":
    for ch in a[2].utf16 {
        var c = ch
        for down in [true, false] {
            let e = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: down)
            e?.keyboardSetUnicodeString(stringLength: 1, unicodeString: &c)
            post(e)
        }
        usleep(25_000)
    }
case "key":
    var f: CGEventFlags = []
    if a.contains("cmd") { f.insert(.maskCommand) }
    if a.contains("shift") { f.insert(.maskShift) }
    key(CGKeyCode(a[2])!, true, flags: f); usleep(40_000)
    key(CGKeyCode(a[2])!, false, flags: f)
default:
    FileHandle.standardError.write("unknown command\n".data(using: .utf8)!)
    exit(2)
}
