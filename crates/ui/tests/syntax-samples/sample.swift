// Syntax sample: Swift.
import Foundation
import SwiftUI

/// A documented protocol.
protocol Describable {
  var summary: String { get }
  func describe(prefix: String) -> String
}

enum Level: Int, CaseIterable {
  case low = 1
  case high
  indirect case nested(Level)
}

struct User: Identifiable, Codable, Describable {
  let id: Int
  var name: String
  var tags: [String] = []
  private(set) var level: Level = .low

  var summary: String { "\(name) #\(id)" }

  func describe(prefix: String = "Hello") -> String {
    "\(prefix), \(name)!"
  }

  mutating func rename(to newName: String) {
    name = newName
  }
}

final class Store<Element: Hashable>: ObservableObject {
  @Published private(set) var items: Set<Element> = []
  static let shared = Store<Int>()
  weak var delegate: AnyObject?

  init() {}

  func add(_ item: Element) -> Bool {
    items.insert(item).inserted
  }
}

extension Array where Element == User {
  func names() -> [String] { map(\.name) }
}

actor Counter {
  private var value = 0
  func increment() -> Int { value += 1; return value }
}

struct ContentView: View {
  @State private var count = 0
  @Environment(\.colorScheme) var scheme

  var body: some View {
    VStack(spacing: 8) {
      Text("Count: \(count)").font(.headline)
      Button("Add") { count += 1 }
    }
  }
}

let raw = #"raw "string" \#(1 + 1)"#
let multi = """
  multi-line \(raw)
  """
let numbers = [1, 0xFF, 0b101, 3.5e2, 1_000]
let user: User? = User(id: 1, name: "ada")

if let user, user.id > 0, !user.tags.isEmpty {
  print(user.describe())
} else {
  guard let first = numbers.first else { fatalError("empty") }
  switch first {
  case 1...3: print("low")
  case let value where value > 10: print(value)
  default: break
  }
}

Task { await Counter().increment() }
do {
  try JSONEncoder().encode(user)
} catch {
  print(error)
}
