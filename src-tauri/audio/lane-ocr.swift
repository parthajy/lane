// lane-ocr: prints the text in an image (screenshot, photo, scan) using the
// Mac's own Vision framework. Nothing leaves the machine.
//
// usage: lane-ocr <image path>

import Foundation
import Vision
import AppKit

guard CommandLine.arguments.count > 1 else {
    FileHandle.standardError.write("usage: lane-ocr <image>\n".data(using: .utf8)!)
    exit(2)
}
let path = CommandLine.arguments[1]
guard let image = NSImage(contentsOfFile: path), let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    FileHandle.standardError.write("cannot read image\n".data(using: .utf8)!)
    exit(1)
}

let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
request.usesLanguageCorrection = true
if #available(macOS 13.0, *) {
    request.automaticallyDetectsLanguage = true
}
let handler = VNImageRequestHandler(cgImage: cg, options: [:])
do {
    try handler.perform([request])
} catch {
    FileHandle.standardError.write("ocr failed: \(error)\n".data(using: .utf8)!)
    exit(1)
}
// Lines top to bottom, left to right, joined by newlines; low-confidence
// fragments are dropped.
let observations = (request.results ?? []).filter { ($0.topCandidates(1).first?.confidence ?? 0) >= 0.3 }
let sorted = observations.sorted { a, b in
    let ay = a.boundingBox.midY, by = b.boundingBox.midY
    if abs(ay - by) > 0.01 { return ay > by }
    return a.boundingBox.minX < b.boundingBox.minX
}
var out: [String] = []
for o in sorted {
    if let t = o.topCandidates(1).first?.string, !t.trimmingCharacters(in: .whitespaces).isEmpty {
        out.append(t)
    }
}
print(out.joined(separator: "\n"))
