package com.jinghumoon.nethop.companion.control

const val NETHOPCTL_PATH = "/data/adb/modules/nethop/bin/nethopctl"

class RootCommandSpec internal constructor(
    val executable: String,
    val args: List<String>,
    val timeoutMillis: Long,
    val stdoutLimitBytes: Int,
    val stderrLimitBytes: Int,
    val mutating: Boolean,
)

sealed interface RootOperation {
    fun command(): RootCommandSpec

    data object StatusGet : RootOperation {
        override fun command() = spec(listOf("status", "--json"), 3_000, 256 * 1024, 16 * 1024, mutating = false)
    }

    data object ServiceStart : RootOperation {
        override fun command() = spec(listOf("start", "--json", "--wait"), 20_000, 256 * 1024, 32 * 1024, mutating = true)
    }

    data object ServiceStop : RootOperation {
        override fun command() = spec(listOf("stop", "--json", "--wait"), 20_000, 256 * 1024, 32 * 1024, mutating = true)
    }

    class WebUi internal constructor(private val spec: RootCommandSpec) : RootOperation {
        override fun command(): RootCommandSpec = spec
    }

    companion object {
        internal fun webUi(args: List<String>, timeoutMillis: Long, mutating: Boolean): RootOperation = WebUi(
            spec(args, timeoutMillis, 1024 * 1024, 32 * 1024, mutating),
        )

        private fun spec(
            args: List<String>,
            timeoutMillis: Long,
            stdoutLimitBytes: Int,
            stderrLimitBytes: Int,
            mutating: Boolean,
        ) = RootCommandSpec(
            executable = NETHOPCTL_PATH,
            args = args,
            timeoutMillis = timeoutMillis,
            stdoutLimitBytes = stdoutLimitBytes,
            stderrLimitBytes = stderrLimitBytes,
            mutating = mutating,
        )
    }
}
