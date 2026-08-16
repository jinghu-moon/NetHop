package com.jinghumoon.nethop.companion

import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import com.jinghumoon.nethop.companion.control.RootCommandExecutor
import com.jinghumoon.nethop.companion.model.StatusDecoder
import com.jinghumoon.nethop.companion.tile.TileOperationCoordinator
import com.jinghumoon.nethop.companion.tile.TilePresentation
import com.jinghumoon.nethop.companion.tile.TileVisualState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class NetHopTileService : TileService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val coordinator = TileOperationCoordinator(RootCommandExecutor(), StatusDecoder())
    private var refreshJob: Job? = null
    private var clickJob: Job? = null

    override fun onStartListening() {
        super.onStartListening()
        refreshJob?.cancel()
        refreshJob = scope.launch {
            coordinator.refresh(::render)
        }
    }

    override fun onStopListening() {
        refreshJob?.cancel()
        refreshJob = null
        super.onStopListening()
    }

    override fun onClick() {
        super.onClick()
        val action = Runnable { startClickOnce() }
        if (isLocked) {
            @Suppress("DEPRECATION")
            unlockAndRun(action)
        } else {
            action.run()
        }
    }

    private fun startClickOnce() {
        if (clickJob?.isActive == true) return
        refreshJob?.cancel()
        clickJob = scope.launch {
            coordinator.click(::render)
        }
    }

    private fun render(presentation: TilePresentation) {
        val tile = qsTile ?: return
        tile.state = when (presentation.state) {
            TileVisualState.ACTIVE -> Tile.STATE_ACTIVE
            TileVisualState.INACTIVE -> Tile.STATE_INACTIVE
            TileVisualState.UNAVAILABLE -> Tile.STATE_UNAVAILABLE
        }
        tile.label = getString(R.string.app_name)
        tile.subtitle = presentation.subtitle
        tile.contentDescription = "${tile.label}, ${tile.subtitle}"
        tile.updateTile()
    }

    override fun onDestroy() {
        refreshJob?.cancel()
        clickJob?.cancel()
        scope.cancel()
        super.onDestroy()
    }
}
