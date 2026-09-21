// lane-calendar: prints the user's calendar events for the next N hours as
// JSON, from the Mac's own Calendar database (EventKit). Needs the
// Calendars permission; nothing is sent anywhere.
//
// usage: lane-calendar [hours]   (default 48)

import EventKit
import Foundation

let hours = Double(CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "48") ?? 48
let store = EKEventStore()
var granted = false
var answered = false

// Never block on a prompt that was already answered: check first.
let status = EKEventStore.authorizationStatus(for: .event)
if #available(macOS 14.0, *) {
    if status == .fullAccess { granted = true; answered = true }
} else if status == .authorized {
    granted = true; answered = true
}
if status == .denied || status == .restricted {
    print("{\"error\":\"calendar access not granted\"}")
    exit(1)
}
if !answered {
    // The completion arrives on the main run loop, so pump it while waiting
    // (a semaphore on the main thread would wait forever).
    if #available(macOS 14.0, *) {
        store.requestFullAccessToEvents { ok, _ in granted = ok; answered = true }
    } else {
        store.requestAccess(to: .event) { ok, _ in granted = ok; answered = true }
    }
    let deadline = Date().addingTimeInterval(120)
    while !answered && Date() < deadline {
        RunLoop.main.run(until: Date().addingTimeInterval(0.1))
    }
}

guard granted else {
    print("{\"error\":\"calendar access not granted\"}")
    exit(1)
}

// A negative span lists events that ended in the past N hours.
let start = hours >= 0 ? Date() : Date().addingTimeInterval(hours * 3600)
let end = hours >= 0 ? start.addingTimeInterval(hours * 3600) : Date()
let predicate = store.predicateForEvents(withStart: start, end: end, calendars: nil)
let events = store.events(matching: predicate).sorted { $0.startDate < $1.startDate }
let iso = ISO8601DateFormatter()

var out: [[String: Any]] = []
for e in events.prefix(50) {
    var attendees: [String] = []
    for a in e.attendees ?? [] {
        if a.isCurrentUser { continue }
        let name = a.name ?? ""
        let url = a.url.absoluteString.replacingOccurrences(of: "mailto:", with: "")
        attendees.append(name.isEmpty ? url : name)
    }
    out.append([
        "id": e.eventIdentifier ?? "",
        "title": e.title ?? "",
        "start": Int(e.startDate.timeIntervalSince1970 * 1000),
        "end": Int(e.endDate.timeIntervalSince1970 * 1000),
        "allDay": e.isAllDay,
        "location": e.location ?? "",
        "notes": String((e.notes ?? "").prefix(500)),
        "calendar": e.calendar.title,
        "attendees": attendees,
        "organizer": e.organizer?.name ?? "",
    ])
}
if let d = try? JSONSerialization.data(withJSONObject: ["events": out, "generatedAt": iso.string(from: Date())]), let s = String(data: d, encoding: .utf8) {
    print(s)
}
