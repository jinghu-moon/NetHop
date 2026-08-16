package com.jinghumoon.nethop.companion.webui

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import com.jinghumoon.nethop.companion.control.RootOperation
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import org.junit.Test

class EventProcessTest {
    @Test
    fun drainsStreamsBeforeEmittingSingleExit() {
        val events = mutableListOf<Triple<String, String?, Int?>>()
        val terminal = CountDownLatch(1)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val child = EventProcess(eventOperation(), scope, emit = { type, data, code ->
            synchronized(events) { events += Triple(type, data, code) }
            if (type == "exit") terminal.countDown()
        }, startProcess = { FakeProcess(stdout = "one\n", stderr = "two\n", exitCode = 7) })

        assertTrue(terminal.await(2, TimeUnit.SECONDS))
        assertTrue(child.isClosed)
        val snapshot = synchronized(events) { events.toList() }
        assertEquals(1, snapshot.count { it.first == "exit" })
        assertEquals(7, snapshot.last().third)
        assertTrue(snapshot.any { it.first == "stdout" && it.second == "one\n" })
        assertTrue(snapshot.any { it.first == "stderr" && it.second == "two\n" })
        scope.cancel()
    }

    @Test
    fun streamFailureEmitsOneStableErrorAndStopsOwnedProcess() {
        val events = mutableListOf<Triple<String, String?, Int?>>()
        val terminal = CountDownLatch(1)
        val process = FakeProcess(stdoutStream = FailingInputStream())
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val child = EventProcess(eventOperation(), scope, emit = { type, data, code ->
            synchronized(events) { events += Triple(type, data, code) }
            if (type == "error") terminal.countDown()
        }, startProcess = { process })

        assertTrue(terminal.await(2, TimeUnit.SECONDS))
        assertTrue(child.isClosed)
        assertTrue(process.destroyed)
        val snapshot = synchronized(events) { events.toList() }
        assertEquals(listOf(Triple("error", "child_stream_failed", null)), snapshot.filter { it.first == "error" })
        assertEquals(1, snapshot.count { it.first == "error" || it.first == "exit" })
        scope.cancel()
    }

    @Test
    fun explicitCloseDoesNotEmitAfterTermination() {
        val events = mutableListOf<Triple<String, String?, Int?>>()
        val process = FakeProcess(blockUntilDestroyed = true)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val child = EventProcess(eventOperation(), scope, emit = { type, data, code ->
            synchronized(events) { events += Triple(type, data, code) }
        }, startProcess = { process })

        child.close()
        assertTrue(child.isClosed)
        assertTrue(process.destroyed)
        Thread.sleep(50)
        assertTrue(synchronized(events) { events.isEmpty() })
        scope.cancel()
    }

    @Test
    fun explicitCloseTerminatesTheOwnedRemoteEventSessionExactlyOnce() {
        val process = FakeProcess(blockUntilDestroyed = true)
        val terminations = mutableListOf<RootOperation>()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val child = EventProcess(
            eventOperation(),
            scope,
            emit = { _, _, _ -> },
            startProcess = { process },
            terminateSession = { terminations += it },
        )

        child.close()
        child.close()

        assertEquals(1, terminations.size)
        assertEquals(
            listOf("webui", "events", "terminate", "evt_11111111111111111111111111111111", "--json"),
            terminations.single().command().args,
        )
        scope.cancel()
    }

    @Test
    fun streamFailureTerminatesTheRemoteEventSession() {
        val terminal = CountDownLatch(1)
        val terminations = mutableListOf<RootOperation>()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        EventProcess(
            eventOperation(),
            scope,
            emit = { type, _, _ -> if (type == "error") terminal.countDown() },
            startProcess = { FakeProcess(stdoutStream = FailingInputStream()) },
            terminateSession = { synchronized(terminations) { terminations += it } },
        )

        assertTrue(terminal.await(2, TimeUnit.SECONDS))
        assertEquals(1, synchronized(terminations) { terminations.size })
        scope.cancel()
    }

    private fun eventOperation() = requireNotNull(BridgeCommandPolicy.operation(
        "events.subscribe",
        listOf(
            "events", "--jsonl", "--kinds", "runtime", "--session-id",
            "evt_11111111111111111111111111111111", "--max-runtime-seconds", "300",
        ),
        spawn = true,
    ))
}

private class FailingInputStream : InputStream() {
    override fun read(): Int = throw IOException("fixture failure")
}

private class FakeProcess(
    stdout: String = "",
    stderr: String = "",
    private val exitCode: Int = 0,
    stdoutStream: InputStream? = null,
    private val blockUntilDestroyed: Boolean = false,
) : Process() {
    private val completed = CountDownLatch(if (blockUntilDestroyed) 1 else 0)
    private val stdoutInput = stdoutStream ?: ByteArrayInputStream(stdout.encodeToByteArray())
    private val stderrInput = ByteArrayInputStream(stderr.encodeToByteArray())
    @Volatile var destroyed = false
        private set

    override fun getOutputStream(): OutputStream = ByteArrayOutputStream()
    override fun getInputStream(): InputStream = stdoutInput
    override fun getErrorStream(): InputStream = stderrInput
    override fun waitFor(): Int {
        completed.await()
        return exitCode
    }
    override fun waitFor(timeout: Long, unit: TimeUnit): Boolean = completed.await(timeout, unit)
    override fun exitValue(): Int {
        check(completed.count == 0L)
        return exitCode
    }
    override fun destroy() {
        destroyed = true
        completed.countDown()
    }
    override fun destroyForcibly(): Process {
        destroy()
        return this
    }
    override fun isAlive(): Boolean = completed.count > 0L
}
