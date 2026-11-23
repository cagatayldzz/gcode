// Syntax sample: Kotlin.
package sample

data class User(val id: Int, val name: String?)

fun greet(user: User): String {
  if (user.name == null) return "Hello, guest!"
  return "Hello, ${user.name}!"
}

fun main() {
  val users = listOf(User(1, "ada"), User(2, null))
  users.forEach { println(greet(it)) }
}
