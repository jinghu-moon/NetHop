package com.jinghumoon.nethop.companion.packages

import android.content.Context
import android.content.pm.ApplicationInfo
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import java.util.concurrent.ConcurrentHashMap

/** Activity-scoped PackageManager snapshot shared by list and icon consumers. */
class AndroidPackageRepository(context: Context) {
    private val packageManager = context.applicationContext.packageManager
    private val snapshot = ConcurrentHashMap<String, PackageInfo>()

    init {
        runCatching {
            packageManager.getInstalledPackages(PackageManager.PackageInfoFlags.of(0)).forEach { info ->
                if (info.packageName.matches(PACKAGE_NAME)) snapshot[info.packageName] = info
            }
        }
    }

    fun listPackages(type: String): List<String> {
        if (type !in setOf("user", "system", "all")) return emptyList()
        return snapshot.values.asSequence().filter { info ->
            val application = info.applicationInfo ?: return@filter false
            val system = application.flags and ApplicationInfo.FLAG_SYSTEM != 0
            type == "all" || (type == "system") == system
        }.map(PackageInfo::packageName).distinct().sorted().take(MAX_PACKAGES).toList()
    }

    fun packageInfo(packages: List<String>): List<PackageRecord> {
        if (packages.size > MAX_BATCH || packages.any { !PACKAGE_NAME.matches(it) }) return emptyList()
        return packages.distinct().mapNotNull { name ->
            val info = snapshot[name] ?: return@mapNotNull null
            val application = info.applicationInfo ?: return@mapNotNull null
            PackageRecord(name, info.versionName.orEmpty().take(128), info.longVersionCode,
                packageManager.getApplicationLabel(application).toString().take(256),
                application.flags and ApplicationInfo.FLAG_SYSTEM != 0, application.uid,
                info.lastUpdateTime.takeIf { it >= 0 })
        }
    }

    fun applicationInfo(packageName: String, lastUpdateTimeMs: Long): ApplicationInfo? =
        snapshot[packageName]?.takeIf { it.lastUpdateTime == lastUpdateTimeMs }?.applicationInfo

    fun packageManager(): PackageManager = packageManager

    companion object {
        private const val MAX_PACKAGES = 10_000
        private const val MAX_BATCH = 128
        private val PACKAGE_NAME = Regex("^[A-Za-z0-9_.-]{1,256}$")
    }
}
