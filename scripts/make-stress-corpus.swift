// SPDX-License-Identifier: GPL-3.0-or-later
// Writes the 2,000-file stress corpus for `examples/stress_bench.rs` (docs/benchmarks.md).
// Usage: swift scripts/make-stress-corpus.swift <out-dir>. macOS only: CoreText, CoreGraphics, textutil.
// Text is shuffled sentences from the public license and policy texts in eval/corpus, so the
// corpus holds no private content. Deterministic: the same files on every run.
import CoreGraphics
import CoreText
import Foundation

let args = CommandLine.arguments
guard args.count == 2 else {
    FileHandle.standardError.write("usage: make-stress-corpus.swift <out-dir>\n".data(using: .utf8)!)
    exit(2)
}
let out = URL(fileURLWithPath: args[1])
let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let fm = FileManager.default
try? fm.removeItem(at: out)
try fm.createDirectory(at: out, withIntermediateDirectories: true)

// The mix. 2,000 files in all; scanned PDFs are 10%.
let scanned = 200, bigPages = [520, 610], encrypted = 10, empty = 10, brokenDocx = 10
let docx = 300, textPdf = 400, md = 368
let txt = 2000 - scanned - bigPages.count - encrypted - empty - brokenDocx - docx - textPdf - md

var sentences: [String] = []
for f in try fm.contentsOfDirectory(at: root.appendingPathComponent("eval/corpus"), includingPropertiesForKeys: nil)
    .sorted(by: { $0.path < $1.path }) {
    let text = try String(contentsOf: f, encoding: .utf8)
    for s in text.components(separatedBy: ". ") {
        let s = s.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        if s.count > 40 { sentences.append(s) }
    }
}
var x: UInt64 = 0x9E37_79B9_7F4A_7C15
func body(_ bytes: Int) -> String {
    var b = ""
    while b.utf8.count < bytes {
        x = x &* 6364136223846793005 &+ 1442695040888963407
        b += sentences[Int(x >> 33) % sentences.count] + ". "
    }
    return b
}

let a4 = CGRect(x: 0, y: 0, width: 595, height: 842)
let font = CTFontCreateWithName("Helvetica" as CFString, 11, nil)

/// Lays `text` out over as many A4 pages as it needs, or exactly `pages` pages when given.
func textPdf(_ url: URL, _ text: String, pages: Int? = nil, password: String? = nil) {
    var info: [CFString: Any] = [:]
    if let p = password {
        info[kCGPDFContextUserPassword] = p
        info[kCGPDFContextOwnerPassword] = p
    }
    var box = a4
    let ctx = CGContext(url as CFURL, mediaBox: &box, info as CFDictionary)!
    let attr = NSAttributedString(string: text, attributes: [kCTFontAttributeName as NSAttributedString.Key: font])
    let setter = CTFramesetterCreateWithAttributedString(attr)
    var at = 0
    var n = 0
    while at < attr.length && (pages.map { n < $0 } ?? true) {
        ctx.beginPDFPage(nil)
        let frame = CTFramesetterCreateFrame(setter, CFRange(location: at, length: 0),
                                             CGPath(rect: a4.insetBy(dx: 50, dy: 50), transform: nil), nil)
        CTFrameDraw(frame, ctx)
        at += CTFrameGetVisibleStringRange(frame).length
        ctx.endPDFPage()
        n += 1
    }
    ctx.closePDF()
}

/// A PDF whose pages are only bitmaps of text, as a scanner makes: no text layer.
func scannedPdf(_ url: URL, _ text: String, pages: Int) {
    var box = a4
    let ctx = CGContext(url as CFURL, mediaBox: &box, nil)!
    let scale: CGFloat = 150.0 / 72.0
    let (w, h) = (Int(a4.width * scale), Int(a4.height * scale))
    let attr = NSAttributedString(string: text, attributes: [kCTFontAttributeName as NSAttributedString.Key: font])
    let setter = CTFramesetterCreateWithAttributedString(attr)
    var at = 0
    for _ in 0..<pages {
        let bmp = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
                            space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
        bmp.setFillColor(gray: 1, alpha: 1)
        bmp.fill(CGRect(x: 0, y: 0, width: w, height: h))
        bmp.scaleBy(x: scale, y: scale)
        bmp.setFillColor(gray: 0, alpha: 1)
        let frame = CTFramesetterCreateFrame(setter, CFRange(location: at, length: 0),
                                             CGPath(rect: a4.insetBy(dx: 50, dy: 50), transform: nil), nil)
        CTFrameDraw(frame, bmp)
        at += CTFrameGetVisibleStringRange(frame).length
        ctx.beginPDFPage(nil)
        ctx.draw(bmp.makeImage()!, in: a4)
        ctx.endPDFPage()
    }
    ctx.closePDF()
}

func write(_ name: String, _ s: String) throws {
    try s.write(to: out.appendingPathComponent(name), atomically: false, encoding: .utf8)
}

for i in 0..<txt { try write(String(format: "note%04d.txt", i), body(3000)) }
for i in 0..<md { try write(String(format: "page%04d.md", i), "# Section \(i)\n\n" + body(3000)) }
for i in 0..<textPdf { textPdf(out.appendingPathComponent(String(format: "report%04d.pdf", i)), body(3000 * (1 + i % 3))) }
for i in 0..<scanned { scannedPdf(out.appendingPathComponent(String(format: "scan%04d.pdf", i)), body(6000), pages: 1 + i % 2) }
for (i, p) in bigPages.enumerated() {
    textPdf(out.appendingPathComponent("book\(i).pdf"), body(p * 8000), pages: p)
}
for i in 0..<encrypted {
    textPdf(out.appendingPathComponent(String(format: "locked%02d.pdf", i)), body(3000), password: "stress")
}
for (i, ext) in ["txt", "md", "pdf", "docx", "png"].enumerated() {
    for j in 0..<(empty / 5) { fm.createFile(atPath: out.appendingPathComponent("empty\(i)\(j).\(ext)").path, contents: Data()) }
}

// DOCX through textutil, from a staging folder so only the .docx files land in `out`.
let stage = out.appendingPathComponent(".stage")
try fm.createDirectory(at: stage, withIntermediateDirectories: true)
var staged: [String] = []
for i in 0..<(docx + brokenDocx) {
    let p = stage.appendingPathComponent(String(format: "memo%04d.txt", i)).path
    try body(3000).write(toFile: p, atomically: false, encoding: .utf8)
    staged.append(p)
}
let tu = Process()
tu.executableURL = URL(fileURLWithPath: "/usr/bin/textutil")
tu.arguments = ["-convert", "docx"] + staged
try tu.run()
tu.waitUntilExit()
guard tu.terminationStatus == 0 else { fatalError("textutil failed") }
for p in staged {
    let name = URL(fileURLWithPath: p).deletingPathExtension().lastPathComponent + ".docx"
    try fm.moveItem(at: stage.appendingPathComponent(name), to: out.appendingPathComponent(name))
}
try fm.removeItem(at: stage)
// Broken DOCX: half are truncated zips, half are text with a .docx name.
for i in docx..<(docx + brokenDocx) {
    let u = out.appendingPathComponent(String(format: "memo%04d.docx", i))
    let d = try Data(contentsOf: u)
    let broken = (i % 2 == 0) ? d.prefix(d.count / 2) : Data(body(500).utf8)
    try broken.write(to: u)
}

let files = try fm.contentsOfDirectory(atPath: out.path).filter { !$0.hasPrefix(".") }
print("make-stress-corpus: \(files.count) files in \(out.path)")
guard files.count == 2000 else { fatalError("expected 2000 files") }
