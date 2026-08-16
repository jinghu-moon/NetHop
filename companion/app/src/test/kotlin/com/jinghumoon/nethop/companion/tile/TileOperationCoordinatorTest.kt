package com.jinghumoon.nethop.companion.tile

import com.jinghumoon.nethop.companion.control.CommandExecutor
import com.jinghumoon.nethop.companion.control.CommandResult
import com.jinghumoon.nethop.companion.control.RootOperation
import com.jinghumoon.nethop.companion.model.StatusDecoder
import com.jinghumoon.nethop.companion.model.StatusDecoderTest
import kotlin.test.assertEquals
import kotlinx.coroutines.test.runTest
import org.junit.Test

class TileOperationCoordinatorTest {
    @Test
    fun clickReadsIntentMutatesExplicitlyAndReadsFinalFact() = runTest {
        val operations = mutableListOf<RootOperation>()
        var enabled = false
        val executor = CommandExecutor { operation ->
            operations += operation
            when (operation) {
                RootOperation.StatusGet -> CommandResult.Success(
                    StatusDecoderTest.statusEnvelope(
                        state = if (enabled) "running_tproxy" else "fail_open_direct",
                        configured = enabled,
                        effective = enabled,
                        diagnostic = "null",
                    ).encodeToByteArray(),
                    byteArrayOf(),
                )
                RootOperation.ServiceStart -> {
                    enabled = true
                    CommandResult.Success("{}".encodeToByteArray(), byteArrayOf())
                }
                RootOperation.ServiceStop -> error("unexpected stop")
                is RootOperation.WebUi -> error("unexpected WebUI operation")
            }
        }
        val published = mutableListOf<TilePresentation>()
        TileOperationCoordinator(executor, StatusDecoder()).click(published::add)
        assertEquals(listOf(RootOperation.StatusGet, RootOperation.ServiceStart, RootOperation.StatusGet), operations)
        assertEquals(TileVisualState.ACTIVE, published.last().state)
    }

    @Test
    fun malformedStatusNeverCausesMutation() = runTest {
        val operations = mutableListOf<RootOperation>()
        val executor = CommandExecutor { operation ->
            operations += operation
            CommandResult.Success("not-json".encodeToByteArray(), byteArrayOf())
        }
        val published = mutableListOf<TilePresentation>()
        TileOperationCoordinator(executor, StatusDecoder()).click(published::add)
        assertEquals(listOf<RootOperation>(RootOperation.StatusGet), operations)
        assertEquals(TileVisualState.UNAVAILABLE, published.last().state)
    }
}
