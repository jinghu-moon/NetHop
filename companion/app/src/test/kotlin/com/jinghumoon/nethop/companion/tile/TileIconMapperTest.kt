package com.jinghumoon.nethop.companion.tile

import com.jinghumoon.nethop.companion.R
import kotlin.test.assertEquals
import org.junit.Test

class TileIconMapperTest {
    @Test
    fun mapsActiveToSolidIconAndOtherStatesToHollowIcon() {
        assertEquals(R.drawable.ic_nethop_tile_on, TileIconMapper.resourceFor(TileVisualState.ACTIVE))
        assertEquals(R.drawable.ic_nethop_tile, TileIconMapper.resourceFor(TileVisualState.INACTIVE))
        assertEquals(R.drawable.ic_nethop_tile, TileIconMapper.resourceFor(TileVisualState.UNAVAILABLE))
    }
}
