package com.jinghumoon.nethop.companion.packages

import android.content.Context
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
    private val repository = AndroidPackageRepository(context)

    fun listPackages(type: String): List<String> {
        if (type !in setOf("user", "system", "all")) return emptyList()
        return runCatching { repository.listPackages(type) }.getOrElse { emptyList() }
    }

    fun packageInfo(packages: List<String>): List<PackageRecord> {
        if (packages.size > MAX_BATCH || packages.any { !PACKAGE_NAME.matches(it) }) return emptyList()
        return runCatching { repository.packageInfo(packages) }.getOrElse { emptyList() }
    }

    companion object {
        private const val MAX_BATCH = 128
        private val PACKAGE_NAME = Regex("^[A-Za-z0-9_.-]{1,256}$")
    }

}
