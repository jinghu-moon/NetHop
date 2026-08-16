package com.jinghumoon.nethop.companion.webui

import com.jinghumoon.nethop.companion.control.NETHOPCTL_PATH
import com.jinghumoon.nethop.companion.control.RootOperation
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.joinAll
import kotlinx.coroutines.launch

class EventProcess(
    operation: RootOperation,
    scope: CoroutineScope,
    private val emit: (String, String?, Int?) -> Unit,
    startProcess: (List<String>) -> Process = ::startProcess,
    private val terminateSession: (RootOperation) -> Unit = {},
) : AutoCloseable {
    private val closed = AtomicBoolean(false)
    private val process: Process
    private val streamJobs: List<Job>
    private val waitJob: Job
    private val terminationOperation: RootOperation
    private val remoteTerminationRequested = AtomicBoolean(false)

    val isClosed: Boolean
        get() = closed.get()

    init {
        val spec = operation.command()
        require(spec.executable == NETHOPCTL_PATH && spec.timeoutMillis == 0L)
        val sessionId = spec.args.getOrNull(5) ?: error("event session id missing")
        terminationOperation = requireNotNull(BridgeCommandPolicy.operation(
            "events.terminate",
            listOf("webui", "events", "terminate", sessionId, "--json"),
            spawn = false,
        ))
        val command = (listOf(spec.executable) + spec.args).joinToString(" ") { "'${it.replace("'", "'\\''")}'" }
        process = startProcess(listOf("su", "-c", command))
        streamJobs = listOf(
            scope.launch(Dispatchers.IO) { stream(process.inputStream, "stdout") },
            scope.launch(Dispatchers.IO) { stream(process.errorStream, "stderr") },
        )
        waitJob = scope.launch(Dispatchers.IO) {
            val code = process.waitFor()
            streamJobs.joinAll()
            signalTerminal("exit", null, code)
        }
    }

    private fun stream(input: java.io.InputStream, kind: String) {
        runCatching {
            input.bufferedReader().use { reader ->
                val buffer = CharArray(4096)
                while (!closed.get()) {
                    val count = reader.read(buffer)
                    if (count < 0) break
                    if (!closed.get()) emit(kind, String(buffer, 0, count), null)
                }
            }
        }.onFailure {
            requestRemoteTermination()
            signalTerminal("error", "child_stream_failed", null)
            stopProcess()
        }
    }

    private fun signalTerminal(type: String, data: String?, code: Int?) {
        if (closed.compareAndSet(false, true)) emit(type, data, code)
    }

    private fun stopProcess() {
        runCatching { process.inputStream.close() }
        runCatching { process.errorStream.close() }
        process.destroy()
        if (process.isAlive) process.destroyForcibly()
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        requestRemoteTermination()
        streamJobs.forEach(Job::cancel)
        stopProcess()
        if (!process.isAlive) waitJob.cancel()
    }

    private fun requestRemoteTermination() {
        if (remoteTerminationRequested.compareAndSet(false, true)) runCatching { terminateSession(terminationOperation) }
    }

    private companion object {
        fun startProcess(command: List<String>): Process =
            ProcessBuilder(command).redirectErrorStream(false).start()
    }
}
