package com.jinghumoon.nethop.companion.control

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.runBlocking
import org.junit.Test

class RootCommandExecutorTest {
    @Test
    fun mapsSuccessFailureAndOutputLimit() = runBlocking {
        val success = executor(RootJobResult(0, "ok".encodeToByteArray(), byteArrayOf()))
            .execute(RootOperation.StatusGet)
        assertTrue(success is CommandResult.Success)
        assertContentEquals("ok".encodeToByteArray(), success.stdout)
        assertContentEquals(byteArrayOf(), success.stderr)

        val failure = executor(RootJobResult(9, byteArrayOf(), "bad".encodeToByteArray()))
            .execute(RootOperation.StatusGet)
        assertTrue(failure is CommandResult.Failure)
        assertEquals("command_failed", failure.code)
        assertContentEquals("bad".encodeToByteArray(), failure.stderr)

        val oversized = executor(RootJobResult(0, ByteArray(256 * 1024 + 1), byteArrayOf()))
            .execute(RootOperation.StatusGet)
        assertEquals("command_output_exceeded", (oversized as CommandResult.Failure).code)
    }

    @Test
    fun mapsUnavailableTimeoutAndUnexpectedFailures() = runBlocking {
        val unavailable = RootCommandExecutor(RootJobRunner { _, _, _, _ -> throw RootShellUnavailableException() })
            .execute(RootOperation.StatusGet)
        assertEquals("root_unavailable", (unavailable as CommandResult.Failure).code)

        val timeout = RootCommandExecutor(RootJobRunner { _, _, _, _ -> throw RootShellTimeoutException() })
            .execute(RootOperation.StatusGet)
        assertEquals("command_timeout", (timeout as CommandResult.Failure).code)

        val failed = RootCommandExecutor(RootJobRunner { _, _, _, _ -> error("broken shell") })
            .execute(RootOperation.StatusGet)
        assertEquals("command_failed", (failed as CommandResult.Failure).code)
    }

    @Test
    fun reusesOneRunnerAndSerializesConcurrentRootJobs() = runBlocking {
        val calls = AtomicInteger(0)
        val active = AtomicInteger(0)
        val maxActive = AtomicInteger(0)
        val runner = RootJobRunner { command, _, _, _ ->
            assertTrue(command.startsWith("'/data/adb/modules/nethop/bin/nethopctl'"))
            calls.incrementAndGet()
            val nowActive = active.incrementAndGet()
            maxActive.accumulateAndGet(nowActive, ::maxOf)
            Thread.sleep(20)
            active.decrementAndGet()
            RootJobResult(0, "{}".encodeToByteArray(), byteArrayOf())
        }
        val executor = RootCommandExecutor(runner)

        List(4) { async(Dispatchers.Default) { executor.execute(RootOperation.StatusGet) } }.awaitAll()

        assertEquals(4, calls.get())
        assertEquals(1, maxActive.get())
    }

    @Test
    fun cancellationInterruptsTheActivePersistentShellJob() = runBlocking {
        val started = CountDownLatch(1)
        val interrupted = CountDownLatch(1)
        val executor = RootCommandExecutor(RootJobRunner { _, _, _, _ ->
            started.countDown()
            try {
                Thread.sleep(TimeUnit.MINUTES.toMillis(1))
            } catch (failure: InterruptedException) {
                interrupted.countDown()
                throw failure
            }
            RootJobResult(0, byteArrayOf(), byteArrayOf())
        })
        val job = async(Dispatchers.Default) { executor.execute(RootOperation.StatusGet) }
        assertTrue(started.await(2, TimeUnit.SECONDS))

        job.cancelAndJoin()

        assertTrue(interrupted.await(2, TimeUnit.SECONDS))
    }

    private fun executor(result: RootJobResult) = RootCommandExecutor(RootJobRunner { _, _, _, _ -> result })
}
