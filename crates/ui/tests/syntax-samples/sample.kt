// Syntax sample: Kotlin.
package sample.app

import kotlin.math.max
import kotlinx.coroutines.*

/** A documented class. */
@Suppress("unused")
data class User(val id: Int, var name: String = "guest", val tags: List<String> = emptyList())

sealed interface Result<out T> {
    data class Ok<T>(val value: T) : Result<T>
    data class Failure(val error: Throwable) : Result<Nothing>
}

enum class Level(val weight: Int) { LOW(1), HIGH(10) }

object Registry {
    private val users = mutableMapOf<Int, User>()
    const val LIMIT = 16

    fun add(user: User): Boolean = users.putIfAbsent(user.id, user) == null
}

abstract class Base<T : Comparable<T>> {
    abstract fun compare(a: T, b: T): Int
    open fun describe(): String = "base"
}

fun User.greet(prefix: String = "Hello"): String = "$prefix, ${name.uppercase()}! ($id)"

inline fun <reified T> List<*>.firstOf(): T? = this.firstOrNull { it is T } as? T

suspend fun load(id: Int): Result<User> = withContext(Dispatchers.IO) {
    try {
        delay(100L)
        Result.Ok(User(id, "loaded"))
    } catch (error: Exception) {
        Result.Failure(error)
    }
}

fun main() = runBlocking {
    val raw = """
        raw "string" with $LIMIT
    """.trimIndent()
    val char = 'x'
    val number = 0xFF + 1_000L + 3.5f
    val users = listOf(User(1, "ada"), User(2, "bob", listOf("admin")))
    for ((index, user) in users.withIndex()) {
        when {
            user.id > 1 && user.tags.isNotEmpty() -> println(user.greet())
            index == 0 -> continue
            else -> break
        }
    }
    val result = load(max(1, 2))
    if (result is Result.Ok) println(result.value.name ?: "none") else println(raw + char + number)
    users.map { it.name }.filter { it.length > 2 }.forEach(::println)
}
