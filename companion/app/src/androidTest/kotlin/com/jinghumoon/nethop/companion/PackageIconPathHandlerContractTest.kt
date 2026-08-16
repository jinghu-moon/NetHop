package com.jinghumoon.nethop.companion

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.jinghumoon.nethop.companion.packages.PackageIconPathHandler
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
        val handler = PackageIconPathHandler(context)
        val response = handler.handle(context.packageName)
        val bytes = response.data.use { it.readBytes() }

        assertEquals(200, response.statusCode)
        assertEquals("image/png", response.mimeType)
        assertTrue(bytes.size in 9..(256 * 1024))
        assertArrayEquals(byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10), bytes.copyOfRange(0, 8))
        assertEquals(404, handler.handle("../../data").statusCode)
        assertEquals(404, handler.handle("com.example.missing.package").statusCode)
        handler.close()
        assertEquals(404, handler.handle(context.packageName).statusCode)
    }
}
