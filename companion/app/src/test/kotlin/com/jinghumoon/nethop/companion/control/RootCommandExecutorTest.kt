package com.jinghumoon.nethop.companion.control

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.test.assertEquals
import kotlin.test.assertContentEquals
import kotlin.test.assertTrue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.junit.Test

class RootCommandExecutorTest {
    @Test
    fun mapsSuccessFailureAndOutputLimit() = runBlocking {
        val success = RootCommandExecutor { FakeProcess("ok", "", 0) }.execute(RootOperation.StatusGet)
        assertTrue(success is CommandResult.Success)
        assertContentEquals("ok".encodeToByteArray(), success.stdout)
        assertContentEquals(byteArrayOf(), success.stderr)

        val failure = RootCommandExecutor { FakeProcess("", "bad", 9) }.execute(RootOperation.StatusGet)
        assertTrue(failure is CommandResult.Failure)
        assertEquals("command_failed", failure.code)
        assertContentEquals("bad".encodeToByteArray(), failure.stderr)

        val oversized = RootCommandExecutor { FakeProcess("x".repeat(256 * 1024 + 1), "", 0) }.execute(RootOperation.StatusGet)
        assertEquals("command_output_exceeded", (oversized as CommandResult.Failure).code)
    }

    @Test
    fun mapsStartupAndTimeoutFailures() = runBlocking {
        val unavailable = RootCommandExecutor { error("no root") }.execute(RootOperation.StatusGet)
        assertTrue(unavailable is CommandResult.Failure)
        assertEquals("root_unavailable", unavailable.code)

        val process = FakeProcess(blockUntilDestroyed = true)
        val timeout = RootCommandExecutor { process }.execute(
            RootOperation.webUi(listOf("status", "--json"), timeoutMillis = 1, mutating = false),
        )
        assertTrue(timeout is CommandResult.Failure)
        assertEquals("command_timeout", timeout.code)
        assertTrue(process.destroyed)
    }

    @Test
    fun cancellationDestroysOwnedProcessAndClosesStreams() = runBlocking {
        val process = FakeProcess(blockUntilDestroyed = true)
        val job = async(Dispatchers.Default) { RootCommandExecutor { process }.execute(RootOperation.StatusGet) }
        assertTrue(process.started.await(2, TimeUnit.SECONDS))
        job.cancel()
        job.join()
        assertTrue(process.destroyed)
        assertTrue(process.stdoutClosed)
        assertTrue(process.stderrClosed)
    }
}

private class FakeProcess(
    stdout: String = "",
    stderr: String = "",
    private val code: Int = 0,
    private val blockUntilDestroyed: Boolean = false,
) : Process() {
    val started = CountDownLatch(1)
    private val completion = CountDownLatch(if (blockUntilDestroyed) 1 else 0)
    private val stdoutStream = TrackingInputStream(stdout.encodeToByteArray())
    private val stderrStream = TrackingInputStream(stderr.encodeToByteArray())
    @Volatile var destroyed = false
        private set
    val stdoutClosed get() = stdoutStream.closed
    val stderrClosed get() = stderrStream.closed

    override fun getOutputStream(): OutputStream = ByteArrayOutputStream()
    override fun getInputStream(): InputStream = stdoutStream
    override fun getErrorStream(): InputStream = stderrStream
    override fun waitFor(): Int { started.countDown(); completion.await(); return code }
    override fun waitFor(timeout: Long, unit: TimeUnit): Boolean {
        started.countDown()
        return completion.await(timeout, unit)
    }
    override fun exitValue(): Int { check(completion.count == 0L); return code }
    override fun destroy() { destroyed = true; completion.countDown() }
    override fun destroyForcibly(): Process { destroy(); return this }
    override fun isAlive(): Boolean = completion.count > 0L
}

private class TrackingInputStream(bytes: ByteArray) : ByteArrayInputStream(bytes) {
    @Volatile var closed = false
    override fun close() { closed = true; super.close() }
}
