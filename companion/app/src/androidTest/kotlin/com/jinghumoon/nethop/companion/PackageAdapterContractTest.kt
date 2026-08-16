package com.jinghumoon.nethop.companion

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.jinghumoon.nethop.companion.packages.AndroidPackageAdapter
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PackageAdapterContractTest {
    @Test
    fun enumeratesAndBatchesWithoutInventingOptionalMetrics() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val adapter = AndroidPackageAdapter(context)
        val packages = adapter.listPackages("all")
        assertEquals(packages.distinct().sorted(), packages)
        assertTrue(packages.size <= 10_000)
        val info = adapter.packageInfo(packages.take(128))
        assertTrue(info.all { it.packageName in packages && it.storageBytes == null && it.lastUsedTimeMs == null })
        assertTrue(adapter.packageInfo(List(129) { "com.example.$it" }).isEmpty())
        assertTrue(adapter.listPackages("invalid").isEmpty())
    }
}
