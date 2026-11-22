// Usage: swift press.swift <pid> <role> <title>
// Performs AXPress on the first element with that AX role (e.g. AXCheckBox) and exact title.
// Presses through Accessibility like VoiceOver does, so it needs no focus and sends no keystrokes.
import ApplicationServices
guard CommandLine.arguments.count == 4, let pid = Int32(CommandLine.arguments[1]) else {
  print("usage: press.swift <pid> <role> <title>"); exit(2)
}
let (role, title) = (CommandLine.arguments[2], CommandLine.arguments[3])
func attr(_ e: AXUIElement, _ a: String) -> AnyObject? {
  var v: AnyObject?
  return AXUIElementCopyAttributeValue(e, a as CFString, &v) == .success ? v : nil
}
func find(_ e: AXUIElement, _ depth: Int) -> AXUIElement? {
  if depth > 30 { return nil }
  if attr(e, kAXRoleAttribute) as? String == role, attr(e, kAXTitleAttribute) as? String == title { return e }
  for c in attr(e, kAXChildrenAttribute) as? [AXUIElement] ?? [] { if let hit = find(c, depth + 1) { return hit } }
  return nil
}
guard AXIsProcessTrusted() else { print("press.swift: this terminal lacks Accessibility permission"); exit(1) }
for w in attr(AXUIElementCreateApplication(pid), kAXWindowsAttribute) as? [AXUIElement] ?? [] {
  if let e = find(w, 0) {
    let r = AXUIElementPerformAction(e, kAXPressAction as CFString)
    if r == .success { exit(0) }
    print("press.swift: AXPress failed (\(r.rawValue))"); exit(1)
  }
}
print("press.swift: no \(role) titled \(title)"); exit(1)
