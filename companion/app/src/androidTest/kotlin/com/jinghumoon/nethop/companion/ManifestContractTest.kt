package com.jinghumoon.nethop.companion

import android.Manifest
import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import android.service.quicksettings.TileService
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ManifestContractTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val packageManager = context.packageManager

    @Test
    fun exposesOnlyTheTileAndFixedPreferencesActivity() {
        val packageInfo = packageManager.getPackageInfo(
            context.packageName,
            PackageManager.PackageInfoFlags.of(
                (PackageManager.GET_PERMISSIONS or PackageManager.GET_SERVICES or PackageManager.GET_ACTIVITIES).toLong(),
            ),
        )
        val permissions = packageInfo.requestedPermissions?.toSet().orEmpty()
        assertEquals(setOf(Manifest.permission.QUERY_ALL_PACKAGES), permissions)
        assertFalse(Manifest.permission.INTERNET in permissions)
        assertFalse(Manifest.permission.REQUEST_INSTALL_PACKAGES in permissions)

        val service = packageInfo.services?.single { it.name == NetHopTileService::class.java.name }
        assertNotNull(service)
        val requiredService = requireNotNull(service)
        assertTrue(requiredService.exported)
        assertEquals(Manifest.permission.BIND_QUICK_SETTINGS_TILE, requiredService.permission)
        val serviceInfo = packageManager.getServiceInfo(
            ComponentName(context, NetHopTileService::class.java),
            PackageManager.ComponentInfoFlags.of(PackageManager.GET_META_DATA.toLong()),
        )
        assertTrue(serviceInfo.metaData.getBoolean(TileService.META_DATA_TOGGLEABLE_TILE))
        assertFalse(serviceInfo.metaData.containsKey(TileService.META_DATA_ACTIVE_TILE))

        val activity = packageInfo.activities?.single { it.name == WebUiEntryActivity::class.java.name }
        assertNotNull(activity)
        assertTrue(requireNotNull(activity).exported)
    }
}
