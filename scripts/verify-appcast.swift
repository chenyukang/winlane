import CryptoKit
import Foundation

// Verify the published archive against the public key embedded in Winlane, not
// merely against the private key available to the release job.
guard CommandLine.arguments.count == 6 else {
    fatalError("Usage: verify-appcast.swift FEED ZIP PUBLIC_KEY VERSION ARCH")
}
let args = CommandLine.arguments
let xml = try XMLDocument(contentsOf: URL(fileURLWithPath: args[1]), options: [])
let items = try xml.nodes(forXPath: "/rss/channel/item")
guard items.count == 1, let item = items.first as? XMLElement else {
    fatalError("Expected exactly one update in each architecture feed")
}
func value(_ name: String) -> String? { item.elements(forName: name).first?.stringValue }
let archiveURL = URL(fileURLWithPath: args[2])
let expectedURL = "https://github.com/chenyukang/winlane/releases/download/v\(args[4])/\(archiveURL.lastPathComponent)"
guard value("sparkle:version") == args[4],
      value("sparkle:minimumSystemVersion") == "14.0",
      value("sparkle:hardwareRequirements") == (args[5] == "arm64" ? "arm64" : nil),
      let enclosure = item.elements(forName: "enclosure").first,
      enclosure.attribute(forName: "url")?.stringValue == expectedURL,
      archiveURL.lastPathComponent == "Winlane-\(args[4])-macos-\(args[5]).zip",
      let signatureText = enclosure.attribute(forName: "sparkle:edSignature")?.stringValue,
      let signature = Data(base64Encoded: signatureText) else {
    fatalError("Invalid update version, URL, architecture, OS requirement, or signature")
}
let archive = try Data(contentsOf: archiveURL, options: .mappedIfSafe)
guard enclosure.attribute(forName: "length")?.stringValue == String(archive.count) else {
    fatalError("Update length differs from the archive")
}
let keyText = try String(contentsOfFile: args[3], encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines)
guard let keyData = Data(base64Encoded: keyText) else { fatalError("Invalid public key") }
let key = try Curve25519.Signing.PublicKey(rawRepresentation: keyData)
guard key.isValidSignature(signature, for: archive) else {
    fatalError("Update signature does not match Winlane's embedded public key")
}
print("Verified \(archiveURL.lastPathComponent) against the embedded public key")
