// Syntax sample: Swift.
import Foundation

struct User {
  let id: Int
  var name: String

  func greet() -> String {
    "Hello, \(name)!"
  }
}

let users = [User(id: 1, name: "ada"), User(id: 2, name: "bob")]
for user in users where user.id > 1 {
  print(user.greet())
}
