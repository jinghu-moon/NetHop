package com.jinghumoon.nethop.companion.tile

import com.jinghumoon.nethop.companion.control.CommandExecutor
import com.jinghumoon.nethop.companion.control.CommandResult
import com.jinghumoon.nethop.companion.control.RootOperation
import com.jinghumoon.nethop.companion.model.StatusDecodeResult
import com.jinghumoon.nethop.companion.model.StatusDecoder
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

class TileOperationCoordinator(
    private val executor: CommandExecutor,
    private val decoder: StatusDecoder,
) {
    private val operationLock = Mutex()
    private val sequence = AtomicLong(0)
    private val publishedSequence = AtomicLong(0)

    suspend fun refresh(publish: (TilePresentation) -> Unit) {
        val current = sequence.incrementAndGet()
        operationLock.withLock {
            publishIfCurrent(current, readPresentation(), publish)
        }
    }

    suspend fun click(publish: (TilePresentation) -> Unit) {
        if (!operationLock.tryLock()) return
        val current = sequence.incrementAndGet()
        try {
            publishIfCurrent(current, TileStateMapper.processing(), publish)
            val initial = readStatus()
            if (initial == null) {
                publishIfCurrent(current, TileStateMapper.unavailable(), publish)
                return
            }
            val operation = if (initial.service.configuredEnabled) {
                RootOperation.ServiceStop
            } else {
                RootOperation.ServiceStart
            }
            executor.execute(operation)
            publishIfCurrent(current, readPresentation(), publish)
        } finally {
            operationLock.unlock()
        }
    }

    private suspend fun readPresentation(): TilePresentation =
        readStatus()?.let(TileStateMapper::map) ?: TileStateMapper.unavailable()

    private suspend fun readStatus() = when (val result = executor.execute(RootOperation.StatusGet)) {
        is CommandResult.Success -> when (val decoded = decoder.decode(result.stdout)) {
            is StatusDecodeResult.Success -> decoded.status
            is StatusDecodeResult.Failure -> null
        }
        is CommandResult.Failure -> null
    }

    private fun publishIfCurrent(
        current: Long,
        presentation: TilePresentation,
        publish: (TilePresentation) -> Unit,
    ) {
        while (true) {
            val observed = publishedSequence.get()
            if (current < observed) return
            if (publishedSequence.compareAndSet(observed, current)) {
                publish(presentation)
                return
            }
        }
    }
}
