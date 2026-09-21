// lane-shot: a JPEG of the front window, scaled down, for the memory it
// belongs to. Needs Screen Recording; without it macOS hands back an empty
// image and this prints "no-permission". Nothing leaves the machine.
//
// usage: lane-shot <out.jpg> [max width, default 1280]
//        lane-shot --check   (exit 0 when Screen Recording is granted)
//        lane-shot --request (ask macOS for Screen Recording)

import Foundation
import AppKit
import CoreGraphics

let args = CommandLine.arguments
if args.count > 1 && args[1] == "--check" {
    exit(CGPreflightScreenCaptureAccess() ? 0 : 1)
}
if args.count > 1 && args[1] == "--request" {
    exit(CGRequestScreenCaptureAccess() ? 0 : 1)
}
guard args.count > 1 else {
    FileHandle.standardError.write("usage: lane-shot <out.jpg> [max width]\n".data(using: .utf8)!)
    exit(2)
}
let out = args[1]
let maxWidth = args.count > 2 ? (CGFloat(Double(args[2]) ?? 1280) ) : 1280
guard CGPreflightScreenCaptureAccess() else {
    print("no-permission")
    exit(3)
}
// The front app's first on-screen, normal-level window.
guard let front = NSWorkspace.shared.frontmostApplication else { exit(1) }
let pid = front.processIdentifier
guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { exit(1) }
var windowId: CGWindowID? = nil
for w in list {
    guard let owner = w[kCGWindowOwnerPID as String] as? Int32, owner == pid else { continue }
    let layer = w[kCGWindowLayer as String] as? Int ?? 0
    if layer != 0 { continue }
    if let b = w[kCGWindowBounds as String] as? [String: Any], let width = b["Width"] as? Double, let height = b["Height"] as? Double, width < 200 || height < 120 { continue }
    if let n = w[kCGWindowNumber as String] as? Int { windowId = CGWindowID(n); break }
}
guard let wid = windowId else {
    print("no-window")
    exit(1)
}
// screencapture does the capture (CGWindowListCreateImage is gone in the
// macOS 15 SDK); the app that launched us is the responsible process for
// the Screen Recording grant.
let tmp = NSTemporaryDirectory() + "lane-shot-\(getpid()).png"
let sc = Process()
sc.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
sc.arguments = ["-x", "-o", "-l", String(wid), "-t", "png", tmp]
do { try sc.run(); sc.waitUntilExit() } catch { print("no-capture"); exit(1) }
defer { try? FileManager.default.removeItem(atPath: tmp) }
guard sc.terminationStatus == 0, let full = NSImage(contentsOfFile: tmp), let image = full.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    print("no-capture")
    exit(1)
}
let scale = min(1.0, maxWidth / CGFloat(image.width))
let w = Int(CGFloat(image.width) * scale), h = Int(CGFloat(image.height) * scale)
guard let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue) else { exit(1) }
ctx.interpolationQuality = .medium
ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
guard let scaled = ctx.makeImage() else { exit(1) }
let rep = NSBitmapImageRep(cgImage: scaled)
guard let data = rep.representation(using: .jpeg, properties: [.compressionFactor: 0.6]) else { exit(1) }
do {
    try data.write(to: URL(fileURLWithPath: out))
    print("ok \(w)x\(h) \(data.count)")
} catch {
    FileHandle.standardError.write("write failed: \(error)\n".data(using: .utf8)!)
    exit(1)
}
