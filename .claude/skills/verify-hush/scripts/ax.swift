// Usage: swift ax.swift <pid>
// Dumps the accessibility tree of every window: "<depth-indent><role>[/<subrole>] desc=<..> value=<..>".
// Web content (ARIA labels, input values, heading text) shows up here, so grep it for proof.
import ApplicationServices
guard CommandLine.arguments.count == 2, let pid = Int32(CommandLine.arguments[1]) else {
  print("usage: ax.swift <pid>"); exit(2)
}
func attr(_ e: AXUIElement, _ a: String) -> AnyObject? {
  var v: AnyObject?
  return AXUIElementCopyAttributeValue(e, a as CFString, &v) == .success ? v : nil
}
func dump(_ e: AXUIElement, _ depth: Int) {
  guard depth < 30 else { return }
  let role = attr(e, kAXRoleAttribute) as? String ?? "?"
  let sub = (attr(e, kAXSubroleAttribute) as? String).map { "/\($0)" } ?? ""
  var line = String(repeating: "  ", count: depth) + role + sub
  for (k, a) in [("title", kAXTitleAttribute), ("desc", kAXDescriptionAttribute), ("value", kAXValueAttribute)] {
    if let s = attr(e, a) as? String, !s.isEmpty { line += " \(k)=\(s)" }
  }
  if role == "AXWindow", let f = attr(e, "AXFrame") {
    var r = CGRect.zero
    AXValueGetValue(f as! AXValue, .cgRect, &r)
    line += " size=\(Int(r.width))x\(Int(r.height))"
  }
  print(line)
  for c in attr(e, kAXChildrenAttribute) as? [AXUIElement] ?? [] { dump(c, depth + 1) }
}
guard AXIsProcessTrusted() else { print("ax.swift: this terminal lacks Accessibility permission"); exit(1) }
for w in attr(AXUIElementCreateApplication(pid), kAXWindowsAttribute) as? [AXUIElement] ?? [] { dump(w, 0) }
