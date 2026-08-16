package com.jinghumoon.nethop.companion.packages

import android.content.Context
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import kotlinx.serialization.Serializable

@Serializable
data class PackageRecord(
    val packageName: String,
    val versionName: String,
    val versionCode: Long,
    val appLabel: String,
    val isSystem: Boolean,
    val uid: Int,
    val lastUpdateTimeMs: Long? = null,
    val storageBytes: Long? = null,
    val lastUsedTimeMs: Long? = null,
)

class AndroidPackageAdapter(context: Context) {
    private val packageManager = context.applicationContext.packageManager

    fun listPackages(type: String): List<String> {
        if (type !in setOf("user", "system", "all")) return emptyList()
        return runCatching {
            packageManager.getInstalledApplications(PackageManager.ApplicationInfoFlags.of(0))
                .asSequence()
                .filter { application ->
                    val system = application.flags and ApplicationInfo.FLAG_SYSTEM != 0
                    type == "all" || (type == "system") == system
                }
                .map(ApplicationInfo::packageName)
                .distinct()
                .sorted()
                .take(MAX_PACKAGES)
                .toList()
        }.getOrElse { emptyList() }
    }

    fun packageInfo(packages: List<String>): List<PackageRecord> {
        if (packages.size > MAX_BATCH || packages.any { !PACKAGE_NAME.matches(it) }) return emptyList()
        return packages.distinct().mapNotNull { name ->
            runCatching {
                val info = packageManager.getPackageInfo(name, PackageManager.PackageInfoFlags.of(0))
                val application = info.applicationInfo ?: return@runCatching null
                PackageRecord(
                    packageName = name,
                    versionName = info.versionName.orEmpty().take(128),
                    versionCode = info.longVersionCode,
                    appLabel = packageManager.getApplicationLabel(application).toString().take(256),
                    isSystem = application.flags and ApplicationInfo.FLAG_SYSTEM != 0,
                    uid = application.uid,
                    lastUpdateTimeMs = info.lastUpdateTime.takeIf { it >= 0 },
                )
            }.getOrNull()
        }
    }

    companion object {
        private const val MAX_PACKAGES = 10_000
        private const val MAX_BATCH = 128
        private val PACKAGE_NAME = Regex("^[A-Za-z0-9_.-]{1,256}$")
    }
}
