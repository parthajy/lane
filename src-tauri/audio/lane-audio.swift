// lane-audio: records the Mac's audio output ("others") and the
// microphone ("you") into two 16 kHz mono WAV files, until stopped.
//
// System audio uses a Core Audio process tap (macOS 14.2+), which needs
// only the "System Audio Recording" permission, not Screen Recording.
// The microphone uses AVAudioEngine and the Microphone permission.
//
// usage: lane-audio <out-dir> [--seconds N] [--no-system] [--no-mic]
// Stops on SIGINT/SIGTERM or after --seconds. Prints JSON lines: status.

import AVFoundation
import CoreAudio
import Foundation

let args = CommandLine.arguments
guard args.count >= 2 else {
    FileHandle.standardError.write("usage: lane-audio <out-dir> [--seconds N] [--no-system] [--no-mic]\n".data(using: .utf8)!)
    exit(2)
}
let outDir = URL(fileURLWithPath: args[1])
var seconds: Double = 0
var wantSystem = true
var wantMic = true
var i = 2
while i < args.count {
    switch args[i] {
    case "--seconds": seconds = Double(args[i + 1]) ?? 0; i += 1
    case "--no-system": wantSystem = false
    case "--no-mic": wantMic = false
    default: break
    }
    i += 1
}
try? FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)

func emit(_ obj: [String: Any]) {
    if let d = try? JSONSerialization.data(withJSONObject: obj), let s = String(data: d, encoding: .utf8) {
        print(s)
        fflush(stdout)
    }
}

let targetFormat = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 16_000, channels: 1, interleaved: true)!

/// Writes converted 16 kHz mono PCM to a WAV file.
final class WavSink {
    let file: AVAudioFile
    var converter: AVAudioConverter?
    var frames: Int64 = 0
    /// Loudest sample written since the last tick, 0...1. Lets the app tell
    /// silence from a room that is simply quiet.
    var peak: Float = 0
    let lock = NSLock()

    func takePeak() -> Float {
        lock.lock(); defer { lock.unlock() }
        let p = peak
        peak = 0
        return p
    }

    init(url: URL) throws {
        file = try AVAudioFile(forWriting: url, settings: targetFormat.settings, commonFormat: .pcmFormatInt16, interleaved: true)
    }

    func write(_ buffer: AVAudioPCMBuffer) {
        lock.lock(); defer { lock.unlock() }
        if converter == nil || converter!.inputFormat != buffer.format {
            converter = AVAudioConverter(from: buffer.format, to: targetFormat)
        }
        guard let conv = converter else { return }
        let ratio = targetFormat.sampleRate / buffer.format.sampleRate
        let cap = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 64
        guard let out = AVAudioPCMBuffer(pcmFormat: targetFormat, frameCapacity: cap) else { return }
        var consumed = false
        var err: NSError?
        conv.convert(to: out, error: &err) { _, status in
            if consumed {
                status.pointee = .noDataNow
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return buffer
        }
        if err == nil, out.frameLength > 0 {
            try? file.write(from: out)
            frames += Int64(out.frameLength)
            if let ch = out.int16ChannelData?[0] {
                var loudest: Int16 = 0
                for i in 0..<Int(out.frameLength) {
                    let v = abs(ch[i])
                    if v > loudest { loudest = v }
                }
                let level = Float(loudest) / 32768.0
                if level > peak { peak = level }
            }
        }
    }
}

// ── Microphone ──────────────────────────────────────────────────────────

var engine: AVAudioEngine?
var micSink: WavSink?

func startMic() {
    do {
        let sink = try WavSink(url: outDir.appendingPathComponent("mic.wav"))
        micSink = sink
        let eng = AVAudioEngine()
        let input = eng.inputNode
        let fmt = input.outputFormat(forBus: 0)
        input.installTap(onBus: 0, bufferSize: 4096, format: fmt) { buf, _ in sink.write(buf) }
        try eng.start()
        engine = eng
        emit(["event": "mic", "state": "recording", "sampleRate": fmt.sampleRate])
    } catch {
        emit(["event": "mic", "state": "error", "message": "\(error)"])
    }
}

// ── System audio (process tap) ──────────────────────────────────────────

var tapID = AudioObjectID(kAudioObjectUnknown)
var aggregateID = AudioObjectID(kAudioObjectUnknown)
var ioProcID: AudioDeviceIOProcID?
var systemSink: WavSink?

func defaultOutputUID() -> String? {
    var addr = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultOutputDevice, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    var dev = AudioObjectID(kAudioObjectUnknown)
    var size = UInt32(MemoryLayout<AudioObjectID>.size)
    guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size, &dev) == noErr else { return nil }
    var uidAddr = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyDeviceUID, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    var uid: Unmanaged<CFString>?
    size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
    guard AudioObjectGetPropertyData(dev, &uidAddr, 0, nil, &size, &uid) == noErr, let u = uid else { return nil }
    return u.takeRetainedValue() as String
}

func startSystem() {
    guard #available(macOS 14.2, *) else {
        emit(["event": "system", "state": "error", "message": "needs macOS 14.2 or later"])
        return
    }
    do {
        let sink = try WavSink(url: outDir.appendingPathComponent("system.wav"))
        systemSink = sink

        // Tap everything that plays, mixed to stereo.
        let desc = CATapDescription(stereoGlobalTapButExcludeProcesses: [])
        desc.name = "Lane meeting tap"
        desc.isPrivate = false
        desc.muteBehavior = .unmuted
        var status = AudioHardwareCreateProcessTap(desc, &tapID)
        guard status == noErr else {
            emit(["event": "system", "state": "error", "message": "tap failed (\(status)); is System Audio Recording allowed?"])
            return
        }

        // The tap must live inside an aggregate device to be read; the
        // default output device is its clock source.
        let outputUID = defaultOutputUID() ?? ""
        let aggDesc: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Lane meeting capture",
            kAudioAggregateDeviceUIDKey: "so.lane.app.meeting-capture",
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceMainSubDeviceKey: outputUID,
            kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: outputUID]],
            kAudioAggregateDeviceTapListKey: [[kAudioSubTapUIDKey: desc.uuid.uuidString, kAudioSubTapDriftCompensationKey: true]],
        ]
        status = AudioHardwareCreateAggregateDevice(aggDesc as CFDictionary, &aggregateID)
        guard status == noErr else {
            emit(["event": "system", "state": "error", "message": "aggregate device failed (\(status))"])
            return
        }

        // Read the tap's stream format.
        var fmtAddr = AudioObjectPropertyAddress(mSelector: kAudioTapPropertyFormat, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var asbd = AudioStreamBasicDescription()
        var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        status = AudioObjectGetPropertyData(tapID, &fmtAddr, 0, nil, &size, &asbd)
        guard status == noErr, let inFormat = AVAudioFormat(streamDescription: &asbd) else {
            emit(["event": "system", "state": "error", "message": "tap format failed (\(status))"])
            return
        }

        status = AudioDeviceCreateIOProcIDWithBlock(&ioProcID, aggregateID, nil) { _, inData, _, _, _ in
            guard let buf = AVAudioPCMBuffer(pcmFormat: inFormat, bufferListNoCopy: inData, deallocator: nil) else { return }
            sink.write(buf)
        }
        guard status == noErr, let proc = ioProcID else {
            emit(["event": "system", "state": "error", "message": "io proc failed (\(status))"])
            return
        }
        status = AudioDeviceStart(aggregateID, proc)
        guard status == noErr else {
            emit(["event": "system", "state": "error", "message": "device start failed (\(status))"])
            return
        }
        emit(["event": "system", "state": "recording", "sampleRate": inFormat.sampleRate, "channels": inFormat.channelCount])
    } catch {
        emit(["event": "system", "state": "error", "message": "\(error)"])
    }
}

func stopAll() {
    engine?.inputNode.removeTap(onBus: 0)
    engine?.stop()
    if aggregateID != kAudioObjectUnknown, let proc = ioProcID {
        AudioDeviceStop(aggregateID, proc)
        AudioDeviceDestroyIOProcID(aggregateID, proc)
        AudioHardwareDestroyAggregateDevice(aggregateID)
    }
    if tapID != kAudioObjectUnknown {
        AudioHardwareDestroyProcessTap(tapID)
    }
    emit(["event": "stopped", "micFrames": micSink?.frames ?? 0, "systemFrames": systemSink?.frames ?? 0])
    micSink = nil
    systemSink = nil
    exit(0)
}

signal(SIGINT) { _ in stopAll() }
signal(SIGTERM) { _ in stopAll() }

if wantMic { startMic() }
if wantSystem { startSystem() }
emit(["event": "started", "dir": outDir.path])

if seconds > 0 {
    DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { stopAll() }
}
// Heartbeat with frame counts so the app can show a live level/duration.
Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { _ in
    emit(["event": "tick", "micFrames": micSink?.frames ?? 0, "systemFrames": systemSink?.frames ?? 0,
          "micPeak": micSink?.takePeak() ?? 0, "systemPeak": systemSink?.takePeak() ?? 0])
}
RunLoop.main.run()
