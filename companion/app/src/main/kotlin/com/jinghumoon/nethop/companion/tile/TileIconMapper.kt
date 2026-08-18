package com.jinghumoon.nethop.companion.tile

import com.jinghumoon.nethop.companion.R

internal object TileIconMapper {
    fun resourceFor(state: TileVisualState): Int = when (state) {
        TileVisualState.ACTIVE -> R.drawable.ic_nethop_tile_on
        TileVisualState.INACTIVE, TileVisualState.UNAVAILABLE -> R.drawable.ic_nethop_tile
    }
}
