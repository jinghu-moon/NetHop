package com.jinghumoon.nethop.companion.control

import android.content.Context
import com.topjohnwu.superuser.Shell
import java.util.ArrayList
import java.util.concurrent.ExecutionException
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException

internal data class RootJobResult(
    val exitCode: Int,
    val stdout: ByteArray,
    val stderr: ByteArray,
    val stdoutExceeded: Boolean = false,
    val stderrExceeded: Boolean = false,
)

internal fun interface RootJobRunner {
    fun execute(commandLine: String, timeoutMillis: Long, stdoutLimitBytes: Int, stderrLimitBytes: Int): RootJobResult
}

internal class RootShellUnavailableException : Exception()

internal class RootShellTimeoutException : Exception()

internal object PersistentRootShell {
    private val lock = Any()
    private var shell: Shell? = null

    fun acquire(context: Context): Shell? = synchronized(lock) {
        val current = shell
        if (current != null && current.status == Shell.ROOT_SHELL && current.isAlive) return current
        current?.let { closeQuietly(it) }
        val created = runCatching {
            Shell.Builder.create()
                .setContext(context.applicationContext)
                .setTimeout(5)
                .build("su", "--mount-master")
        }.getOrNull()
        if (created == null || created.status != Shell.ROOT_SHELL || !created.isAlive) {
            created?.let(::closeQuietly)
            shell = null
            null
        } else {
            shell = created
            created
        }
    }

    fun invalidate(candidate: Shell) {
        synchronized(lock) {
            if (shell !== candidate) return@synchronized
            shell = null
            closeQuietly(candidate)
        }
    }

    private fun closeQuietly(candidate: Shell) {
        runCatching { candidate.close() }
    }
}

internal class LibSuRootJobRunner(
    private val context: Context,
) : RootJobRunner {
    override fun execute(commandLine: String, timeoutMillis: Long, stdoutLimitBytes: Int, stderrLimitBytes: Int): RootJobResult {
        val shell = PersistentRootShell.acquire(context) ?: throw RootShellUnavailableException()
        val stdout = BoundedLineCollector(stdoutLimitBytes)
        val stderr = BoundedLineCollector(stderrLimitBytes)
        val future = shell.newJob()
            .add(commandLine)
            .to(stdout, stderr)
            .enqueue()
        val result = try {
            future.get(timeoutMillis, TimeUnit.MILLISECONDS)
        } catch (_: TimeoutException) {
            future.cancel(true)
            PersistentRootShell.invalidate(shell)
            throw RootShellTimeoutException()
        } catch (failure: InterruptedException) {
            future.cancel(true)
            PersistentRootShell.invalidate(shell)
            throw failure
        } catch (failure: ExecutionException) {
            if (!shell.isAlive) PersistentRootShell.invalidate(shell)
            throw failure.cause ?: failure
        }
        if (!shell.isAlive) PersistentRootShell.invalidate(shell)
        return RootJobResult(result.code, stdout.bytes(), stderr.bytes(), stdout.exceeded, stderr.exceeded)
    }

    private class BoundedLineCollector(private val limitBytes: Int) : AbstractMutableList<String>() {
        private val values = ArrayList<String>()
        private var byteCount = 0
        var exceeded = false
            private set

        override val size: Int get() = values.size
        override fun get(index: Int): String = values[index]
        override fun set(index: Int, element: String): String = values.set(index, element)
        override fun removeAt(index: Int): String = values.removeAt(index)
        override fun add(index: Int, element: String) {
            val elementBytes = element.encodeToByteArray().size
            val separatorBytes = if (values.isEmpty()) 0 else 1
            if (byteCount.toLong() + separatorBytes + elementBytes > limitBytes.toLong()) {
                exceeded = true
                return
            }
            values.add(index, element)
            byteCount += separatorBytes + elementBytes
        }

        fun bytes(): ByteArray = values.joinToString("\n").encodeToByteArray()
    }
}
