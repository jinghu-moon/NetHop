package com.jinghumoon.nethop.companion.tile

import com.jinghumoon.nethop.companion.model.StatusDecodeResult
import com.jinghumoon.nethop.companion.model.StatusDecoder
import com.jinghumoon.nethop.companion.model.StatusDecoderTest
import kotlin.test.assertEquals
import kotlin.test.assertIs
import org.junit.Test

class TileStateMapperTest {
    @Test
    fun mapsDisabledRunningAndScenePause() {
        assertEquals(
            TilePresentation(TileVisualState.INACTIVE, "已关闭", TileAction.START),
            map(state = "fail_open_direct", configured = false, effective = false),
        )
        assertEquals(
            TilePresentation(TileVisualState.ACTIVE, "TPROXY", TileAction.STOP),
            map(state = "running_tproxy"),
        )
        assertEquals(
            TilePresentation(TileVisualState.ACTIVE, "TUN", TileAction.STOP),
            map(state = "running_tun"),
        )
        assertEquals(
            TilePresentation(TileVisualState.ACTIVE, "场景暂停", TileAction.STOP),
            map(state = "fail_open_direct", effective = false, override = "\"wifi_scene\"", diagnostic = "\"fail_open_direct\""),
        )
    }

    @Test
    fun transitionalAndFailedStatesRejectMutation() {
        for (state in listOf("init", "probing", "starting_core", "starting_tun", "stopping", "degraded", "backoff", "circuit_open", "fail_open_direct")) {
            assertEquals(TileAction.NONE, map(state = state, diagnostic = if (state == "fail_open_direct") "\"fail_open_direct\"" else "null").action)
        }
    }

    private fun map(
        state: String,
        configured: Boolean = true,
        effective: Boolean = true,
        override: String = "null",
        diagnostic: String = "null",
    ): TilePresentation {
        val decoded = StatusDecoder().decode(StatusDecoderTest.statusEnvelope(state, configured, effective, override, diagnostic).encodeToByteArray())
        return TileStateMapper.map(assertIs<StatusDecodeResult.Success>(decoded).status)
    }
}
