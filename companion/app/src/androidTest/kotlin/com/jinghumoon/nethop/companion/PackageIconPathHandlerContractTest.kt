package com.jinghumoon.nethop.companion

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.jinghumoon.nethop.companion.packages.PackageIconPathHandler
import com.jinghumoon.nethop.companion.packages.AndroidPackageRepository
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PackageIconPathHandlerContractTest {
    @Test
    fun returnsBoundedPngOnlyForInstalledPackageNames() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val repository = AndroidPackageRepository(context)
        val handler = PackageIconPathHandler(repository)
        val revision = repository.packageInfo(listOf(context.packageName)).single().lastUpdateTimeMs
        requireNotNull(revision)
        val response = handler.handle("$revision/${context.packageName}")
        val bytes = response.data.use { it.readBytes() }

        assertEquals(200, response.statusCode)
        assertEquals("image/png", response.mimeType)
        assertTrue(bytes.size in 9..(256 * 1024))
        assertArrayEquals(byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10), bytes.copyOfRange(0, 8))
        assertEquals(404, handler.handle("../../data").statusCode)
        assertEquals(404, handler.handle("$revision/com.example.missing.package").statusCode)
        handler.close()
        assertEquals(404, handler.handle("$revision/${context.packageName}").statusCode)
    }
}
