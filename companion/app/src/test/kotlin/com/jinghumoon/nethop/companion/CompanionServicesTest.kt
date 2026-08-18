package com.jinghumoon.nethop.companion

import com.jinghumoon.nethop.companion.control.CommandExecutor
import com.jinghumoon.nethop.companion.control.CommandResult
import com.jinghumoon.nethop.companion.model.StatusDecoder
import com.jinghumoon.nethop.companion.tile.TileVisualState
import kotlinx.coroutines.test.runTest
import kotlin.test.assertEquals
import kotlin.test.assertSame
import org.junit.Test

class CompanionServicesTest {
    @Test
    fun compositionRootSharesTheExecutorAndBuildsTheTileFeature() = runTest {
        val executor = CommandExecutor {
            CommandResult.Success(
                stdout = """{"version":6,"request_id":"tile","ok":true,"result":{"schema_version":2,"state":"running_tproxy","generation":1,"last_update":"succeeded","service":{"configured_enabled":true,"effective_enabled":true,"override":null},"diagnostic_code":null,"watcher_health":{},"runtime":{},"subscription":{},"core_update":{},"rule_set":{},"dns_split":{},"capture":{},"operational":{}}}""".encodeToByteArray(),
                stderr = byteArrayOf(),
            )
        }
        val services = CompanionServices(executor, StatusDecoder()) {
            error("package repository is activity-scoped")
        }
        var state: TileVisualState? = null

        services.createTileCoordinator().refresh { state = it.state }

        assertSame(executor, services.commandExecutor)
        assertEquals(TileVisualState.ACTIVE, state)
    }
}
