// lane-contacts: prints the user's contacts (names, nicknames, organisations,
// emails) as JSON, from the Mac's own Contacts database. Needs the Contacts
// permission; used only to recognise people by first name or nickname.

import Contacts
import Foundation

let store = CNContactStore()
var granted = false
var answered = false
let status = CNContactStore.authorizationStatus(for: .contacts)
if status == .authorized { granted = true; answered = true }
if status == .denied || status == .restricted {
    print("{\"error\":\"contacts access not granted\"}")
    exit(1)
}
if !answered {
    store.requestAccess(for: .contacts) { ok, _ in granted = ok; answered = true }
    let deadline = Date().addingTimeInterval(120)
    while !answered && Date() < deadline {
        RunLoop.main.run(until: Date().addingTimeInterval(0.1))
    }
}
guard granted else {
    print("{\"error\":\"contacts access not granted\"}")
    exit(1)
}

let keys: [CNKeyDescriptor] = [CNContactGivenNameKey as CNKeyDescriptor, CNContactFamilyNameKey as CNKeyDescriptor, CNContactNicknameKey as CNKeyDescriptor, CNContactOrganizationNameKey as CNKeyDescriptor, CNContactEmailAddressesKey as CNKeyDescriptor]
let request = CNContactFetchRequest(keysToFetch: keys)
var out: [[String: Any]] = []
do {
    try store.enumerateContacts(with: request) { c, _ in
        let given = c.givenName.trimmingCharacters(in: .whitespaces)
        let family = c.familyName.trimmingCharacters(in: .whitespaces)
        if given.isEmpty && family.isEmpty { return }
        out.append([
            "given": given,
            "family": family,
            "nickname": c.nickname,
            "organization": c.organizationName,
            "emails": c.emailAddresses.map { String($0.value) },
        ])
    }
} catch {
    print("{\"error\":\"could not read contacts\"}")
    exit(1)
}
if let d = try? JSONSerialization.data(withJSONObject: ["contacts": out]), let s = String(data: d, encoding: .utf8) {
    print(s)
}
