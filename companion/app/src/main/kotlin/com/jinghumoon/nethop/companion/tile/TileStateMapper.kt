package com.jinghumoon.nethop.companion.tile

import com.jinghumoon.nethop.companion.model.RuntimeState
import com.jinghumoon.nethop.companion.model.ServiceOverride
import com.jinghumoon.nethop.companion.model.StatusDocument

enum class TileVisualState { ACTIVE, INACTIVE, UNAVAILABLE }

enum class TileAction { START, STOP, NONE }

data class TilePresentation(
    val state: TileVisualState,
    val subtitle: String,
    val action: TileAction,
)

object TileStateMapper {
    fun map(status: StatusDocument): TilePresentation {
        if (status.diagnosticCode?.name == "CONFIG_UNAVAILABLE") return unavailable("不可用")
        if (!status.service.configuredEnabled) {
            return TilePresentation(TileVisualState.INACTIVE, "已关闭", TileAction.START)
        }
        if (status.service.override == ServiceOverride.WIFI_SCENE && !status.service.effectiveEnabled) {
            return TilePresentation(TileVisualState.ACTIVE, "场景暂停", TileAction.STOP)
        }
        return when (status.state) {
            RuntimeState.RUNNING_TPROXY -> TilePresentation(TileVisualState.ACTIVE, "TPROXY", TileAction.STOP)
            RuntimeState.RUNNING_TUN -> TilePresentation(TileVisualState.ACTIVE, "TUN", TileAction.STOP)
            RuntimeState.PROBING, RuntimeState.STARTING_CORE, RuntimeState.STARTING_TUN -> unavailable("启动中")
            RuntimeState.STOPPING -> unavailable("停止中")
            RuntimeState.DEGRADED, RuntimeState.BACKOFF, RuntimeState.CIRCUIT_OPEN,
            RuntimeState.FAIL_OPEN_DIRECT -> unavailable("异常")
            RuntimeState.INIT -> unavailable("不可用")
        }
    }

    fun processing() = TilePresentation(TileVisualState.UNAVAILABLE, "处理中", TileAction.NONE)

    fun unavailable(subtitle: String = "不可用") =
        TilePresentation(TileVisualState.UNAVAILABLE, subtitle, TileAction.NONE)
}
