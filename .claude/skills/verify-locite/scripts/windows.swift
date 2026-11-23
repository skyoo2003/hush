// SPDX-License-Identifier: GPL-3.0-or-later
// Usage: swift windows.swift <pid>
// Prints "<label> <onscreen 0|1> <CGWindowID> <width>x<height>" for Locite's two app windows.
// Labels come from tauri.conf.json: quick-search is the always-on-top one (layer > 0).
import CoreGraphics
guard CommandLine.arguments.count == 2, let pid = Int32(CommandLine.arguments[1]) else {
  print("usage: windows.swift <pid>"); exit(2)
}
let all = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
for w in all where (w[kCGWindowOwnerPID as String] as? Int32) == pid {
  // Skip the untitled helper windows WebKit/AppKit create (menu bar strips etc.).
  guard let name = w[kCGWindowName as String] as? String, !name.isEmpty,
        let b = w[kCGWindowBounds as String] as? [String: Double] else { continue }
  let layer = w[kCGWindowLayer as String] as? Int ?? 0
  let on = (w[kCGWindowIsOnscreen as String] as? Bool ?? false) ? 1 : 0
  print("\(layer > 0 ? "quick-search" : "main") \(on) \(w[kCGWindowNumber as String]!) \(Int(b["Width"]!))x\(Int(b["Height"]!))")
}
