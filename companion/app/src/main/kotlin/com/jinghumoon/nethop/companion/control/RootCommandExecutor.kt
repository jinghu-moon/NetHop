package com.jinghumoon.nethop.companion.control

import android.content.Context
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runInterruptible
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

sealed interface CommandResult {
    data class Success(val stdout: ByteArray, val stderr: ByteArray) : CommandResult
    data class Failure(val code: String, val stderr: ByteArray = byteArrayOf()) : CommandResult
}

fun interface CommandExecutor {
    suspend fun execute(operation: RootOperation): CommandResult
}

class RootCommandExecutor internal constructor(
    private val runner: RootJobRunner,
) : CommandExecutor {
    constructor(context: Context) : this(LibSuRootJobRunner(context.applicationContext))

    override suspend fun execute(operation: RootOperation): CommandResult = withContext(Dispatchers.IO) {
        val spec = operation.command()
        if (spec.executable != NETHOPCTL_PATH || spec.args.any(::unsafeArgument)) {
            return@withContext CommandResult.Failure("command_rejected")
        }
        val commandLine = (listOf(spec.executable) + spec.args).joinToString(" ", transform = ::shellQuote)
        val run: suspend () -> CommandResult = {
            try {
                val result = runInterruptible {
                    runner.execute(commandLine, spec.timeoutMillis, spec.stdoutLimitBytes, spec.stderrLimitBytes)
                }
                if (result.stdoutExceeded || result.stderrExceeded || result.stdout.size > spec.stdoutLimitBytes || result.stderr.size > spec.stderrLimitBytes) {
                    CommandResult.Failure("command_output_exceeded", result.stderr)
                } else if (result.exitCode != 0) {
                    CommandResult.Failure("command_failed", result.stderr)
                } else {
                    CommandResult.Success(result.stdout, result.stderr)
                }
            } catch (failure: CancellationException) {
                throw failure
            } catch (_: RootShellUnavailableException) {
                CommandResult.Failure("root_unavailable")
            } catch (_: RootShellTimeoutException) {
                CommandResult.Failure("command_timeout")
            } catch (_: InterruptedException) {
                CommandResult.Failure("command_cancelled")
            } catch (_: Throwable) {
                CommandResult.Failure("command_failed")
            }
        }
        ROOT_COMMAND_LOCK.withLock { run() }
    }

    private fun unsafeArgument(value: String): Boolean =
        value.isEmpty() || value.any { it == '\u0000' || it == '\r' || it == '\n' }

    private fun shellQuote(value: String): String = "'${value.replace("'", "'\\''")}'"

    companion object {
        private val ROOT_COMMAND_LOCK = Mutex()
    }
}
