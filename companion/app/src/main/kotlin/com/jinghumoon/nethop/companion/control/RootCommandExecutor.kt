package com.jinghumoon.nethop.companion.control

import java.io.InputStream
import java.io.ByteArrayOutputStream
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
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

internal data class BoundedBytes(val bytes: ByteArray, val exceeded: Boolean)

internal fun readBounded(input: InputStream, limit: Int): BoundedBytes {
    val output = ByteArrayOutputStream(minOf(limit, 8192))
    val buffer = ByteArray(8192)
    var exceeded = false
    while (true) {
        val count = input.read(buffer)
        if (count < 0) break
        val remaining = limit - output.size()
        if (remaining > 0) {
            val accepted = minOf(remaining, count)
            output.write(buffer, 0, accepted)
        }
        if (count > remaining) exceeded = true
    }
    return BoundedBytes(output.toByteArray(), exceeded)
}

class RootCommandExecutor internal constructor(
    private val startProcess: (List<String>) -> Process = ::startProcess,
) : CommandExecutor {
    override suspend fun execute(operation: RootOperation): CommandResult = withContext(Dispatchers.IO) {
        val spec = operation.command()
        if (spec.executable != NETHOPCTL_PATH || spec.args.any(::unsafeArgument)) {
            return@withContext CommandResult.Failure("command_rejected")
        }
        if (spec.mutating) MUTATION_LOCK.withLock { execute(spec) } else execute(spec)
    }

    private suspend fun execute(spec: RootCommandSpec): CommandResult {
        val commandLine = (listOf(spec.executable) + spec.args).joinToString(" ", transform = ::shellQuote)
        val process = runCatching { startProcess(listOf("su", "-c", commandLine)) }
            .getOrElse { return CommandResult.Failure("root_unavailable") }

        try {
            val completed = coroutineScope {
                val stdout = async { readBounded(process.inputStream, spec.stdoutLimitBytes) }
                val stderr = async { readBounded(process.errorStream, spec.stderrLimitBytes) }
                if (!runInterruptible { process.waitFor(spec.timeoutMillis, TimeUnit.MILLISECONDS) }) {
                    stopProcess(process)
                    stdout.await()
                    stderr.await()
                    null
                } else {
                    Triple(process.exitValue(), stdout.await(), stderr.await())
                }
            }
            if (completed == null) return CommandResult.Failure("command_timeout")
            val (exitCode, stdout, stderr) = completed
            if (stdout.exceeded || stderr.exceeded) {
                return CommandResult.Failure("command_output_exceeded", stderr.bytes)
            }
            if (exitCode != 0) {
                return CommandResult.Failure("command_failed", stderr.bytes)
            }
            return CommandResult.Success(stdout.bytes, stderr.bytes)
        } catch (failure: Throwable) {
            stopProcess(process)
            throw failure
        }
    }

    private fun stopProcess(process: Process) {
        runCatching { process.inputStream.close() }
        runCatching { process.errorStream.close() }
        process.destroy()
        if (process.isAlive) process.destroyForcibly()
    }

    private fun unsafeArgument(value: String): Boolean =
        value.isEmpty() || value.any { it == '\u0000' || it == '\r' || it == '\n' }

    private fun shellQuote(value: String): String = "'${value.replace("'", "'\\''")}'"

    companion object {
        private val MUTATION_LOCK = Mutex()

        private fun startProcess(command: List<String>): Process =
            ProcessBuilder(command).redirectErrorStream(false).start()
    }
}
