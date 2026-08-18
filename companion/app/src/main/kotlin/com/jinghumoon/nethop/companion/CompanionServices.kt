package com.jinghumoon.nethop.companion

import android.app.Application
import android.content.Context
import com.jinghumoon.nethop.companion.control.CommandExecutor
import com.jinghumoon.nethop.companion.control.RootCommandExecutor
import com.jinghumoon.nethop.companion.model.StatusDecoder
import com.jinghumoon.nethop.companion.packages.AndroidPackageRepository
import com.jinghumoon.nethop.companion.tile.TileOperationCoordinator

internal class CompanionServices(
    val commandExecutor: CommandExecutor,
    private val statusDecoder: StatusDecoder,
    private val packageRepositoryFactory: (Context) -> AndroidPackageRepository,
) {
    constructor(context: Context) : this(
        RootCommandExecutor(context.applicationContext),
        StatusDecoder(),
        ::AndroidPackageRepository,
    )

    fun createTileCoordinator(): TileOperationCoordinator =
        TileOperationCoordinator(commandExecutor, statusDecoder)

    fun createPackageRepository(context: Context): AndroidPackageRepository =
        packageRepositoryFactory(context)
}

class NetHopCompanionApplication : Application() {
    internal val services: CompanionServices by lazy(LazyThreadSafetyMode.SYNCHRONIZED) {
        CompanionServices(this)
    }
}

internal val Context.companionServices: CompanionServices
    get() = (applicationContext as NetHopCompanionApplication).services
