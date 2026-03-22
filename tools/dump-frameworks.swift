#!/usr/bin/env swift
//
// dump-frameworks.swift
//
// Enumerates all classes, methods, and properties from macOS frameworks.
// Run on two macOS versions and diff the output to find new APIs.
//
// Usage:
//   swift dump-frameworks.swift > macos26-apis.json
//   # On another machine:
//   swift dump-frameworks.swift > macos25-apis.json
//   # Then diff:
//   diff <(jq -S . macos25-apis.json) <(jq -S . macos26-apis.json)
//
// Or target specific frameworks:
//   swift dump-frameworks.swift LoggingSupport ktrace kperf
//

import Foundation
import ObjectiveC

struct FrameworkDump: Codable {
    let macosVersion: String
    let timestamp: String
    let frameworks: [FrameworkInfo]
}

struct FrameworkInfo: Codable {
    let path: String
    let name: String
    let classes: [ClassInfo]
}

struct ClassInfo: Codable {
    let name: String
    let superclass: String?
    let methods: [String]
    let properties: [PropertyInfo]
}

struct PropertyInfo: Codable {
    let name: String
    let attributes: String
}

func dumpClass(_ cls: AnyClass) -> ClassInfo {
    let name = NSStringFromClass(cls)
    let superclass = class_getSuperclass(cls).map { NSStringFromClass($0) }

    var methodCount: UInt32 = 0
    var methods: [String] = []
    if let methodList = class_copyMethodList(cls, &methodCount) {
        for i in 0..<Int(methodCount) {
            methods.append(NSStringFromSelector(method_getName(methodList[i])))
        }
        free(methodList)
    }

    var propCount: UInt32 = 0
    var properties: [PropertyInfo] = []
    if let propList = class_copyPropertyList(cls, &propCount) {
        for i in 0..<Int(propCount) {
            let propName = String(cString: property_getName(propList[i]))
            let attrs = property_getAttributes(propList[i]).map { String(cString: $0) } ?? ""
            properties.append(PropertyInfo(name: propName, attributes: attrs))
        }
        free(propList)
    }

    return ClassInfo(
        name: name,
        superclass: superclass,
        methods: methods.sorted(),
        properties: properties.sorted { $0.name < $1.name }
    )
}

func loadAndDumpFramework(path: String) -> FrameworkInfo? {
    let name = (path as NSString).lastPathComponent.replacingOccurrences(of: ".framework", with: "")
    let binaryPath = "\(path)/\(name)"
    let versionsPath = "\(path)/Versions/A/\(name)"

    // Try loading
    guard dlopen(binaryPath, RTLD_LAZY) != nil || dlopen(versionsPath, RTLD_LAZY) != nil else {
        return nil
    }

    // Get all classes that belong to this framework's image
    var classCount: UInt32 = 0
    guard let allClasses = objc_copyClassList(&classCount) else { return nil }
    defer { free(UnsafeMutableRawPointer(allClasses)) }

    // Filter to classes from this framework by checking the image name
    var frameworkClasses: [ClassInfo] = []
    for i in 0..<Int(classCount) {
        let cls = allClasses[i]
        guard let imageName = class_getImageName(cls) else { continue }
        let imageStr = String(cString: imageName)
        if imageStr.contains(name) {
            frameworkClasses.append(dumpClass(cls))
        }
    }

    guard !frameworkClasses.isEmpty else { return nil }

    return FrameworkInfo(
        path: path,
        name: name,
        classes: frameworkClasses.sorted { $0.name < $1.name }
    )
}

// Determine which frameworks to scan
let args = CommandLine.arguments.dropFirst()
var frameworkPaths: [String] = []

if args.isEmpty {
    // Scan all private + public frameworks
    let searchPaths = [
        "/System/Library/PrivateFrameworks",
        "/System/Library/Frameworks",
    ]

    // Also add Xcode's frameworks if available
    let xcodeShared = "/Applications/Xcode.app/Contents/SharedFrameworks"
    if FileManager.default.fileExists(atPath: xcodeShared) {
        frameworkPaths += (try? FileManager.default.contentsOfDirectory(atPath: xcodeShared))?
            .filter { $0.hasSuffix(".framework") }
            .map { "\(xcodeShared)/\($0)" } ?? []
    }

    for searchPath in searchPaths {
        if let contents = try? FileManager.default.contentsOfDirectory(atPath: searchPath) {
            frameworkPaths += contents
                .filter { $0.hasSuffix(".framework") }
                .map { "\(searchPath)/\($0)" }
        }
    }
} else {
    // Scan specific frameworks by name
    for name in args {
        let privatePath = "/System/Library/PrivateFrameworks/\(name).framework"
        let publicPath = "/System/Library/Frameworks/\(name).framework"
        let xcodePath = "/Applications/Xcode.app/Contents/SharedFrameworks/\(name).framework"

        if FileManager.default.fileExists(atPath: privatePath) {
            frameworkPaths.append(privatePath)
        } else if FileManager.default.fileExists(atPath: publicPath) {
            frameworkPaths.append(publicPath)
        } else if FileManager.default.fileExists(atPath: xcodePath) {
            frameworkPaths.append(xcodePath)
        } else {
            fputs("Warning: framework '\(name)' not found\n", stderr)
        }
    }
}

fputs("Scanning \(frameworkPaths.count) frameworks...\n", stderr)

var frameworks: [FrameworkInfo] = []
for (i, path) in frameworkPaths.enumerated() {
    let name = (path as NSString).lastPathComponent
    if i % 50 == 0 {
        fputs("  [\(i)/\(frameworkPaths.count)] \(name)\n", stderr)
    }
    if let info = loadAndDumpFramework(path: path) {
        frameworks.append(info)
    }
}

fputs("Done. \(frameworks.count) frameworks with classes.\n", stderr)

// Get macOS version
let processInfo = ProcessInfo.processInfo
let osVersion = processInfo.operatingSystemVersion
let versionString = "\(osVersion.majorVersion).\(osVersion.minorVersion).\(osVersion.patchVersion)"

let dump = FrameworkDump(
    macosVersion: versionString,
    timestamp: ISO8601DateFormatter().string(from: Date()),
    frameworks: frameworks.sorted { $0.name < $1.name }
)

let encoder = JSONEncoder()
encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
let data = try! encoder.encode(dump)
print(String(data: data, encoding: .utf8)!)
