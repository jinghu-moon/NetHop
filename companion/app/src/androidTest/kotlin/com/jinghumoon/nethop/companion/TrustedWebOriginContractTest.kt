package com.jinghumoon.nethop.companion

import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.jinghumoon.nethop.companion.webui.TrustedWebOrigin
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class TrustedWebOriginContractTest {
    @Test
    fun acceptsOnlyTheExactHttpsMainFrameOrigin() {
        assertTrue(TrustedWebOrigin.accepts(Uri.parse(TrustedWebOrigin.ORIGIN), isMainFrame = true))
        assertFalse(TrustedWebOrigin.accepts(Uri.parse("http://appassets.androidplatform.net"), isMainFrame = true))
        assertFalse(TrustedWebOrigin.accepts(Uri.parse("https://sub.appassets.androidplatform.net"), isMainFrame = true))
        assertFalse(TrustedWebOrigin.accepts(Uri.parse("https://appassets.androidplatform.net:443"), isMainFrame = true))
        assertFalse(TrustedWebOrigin.accepts(Uri.parse(TrustedWebOrigin.ORIGIN), isMainFrame = false))
        assertTrue(TrustedWebOrigin.isPackageIcon(Uri.parse("${TrustedWebOrigin.ORIGIN}/package-icons/com.example.app")))
        assertTrue(TrustedWebOrigin.isTrustedResource(Uri.parse("${TrustedWebOrigin.ORIGIN}/package-icons/com.example.app")))
        assertFalse(TrustedWebOrigin.isPackageIcon(Uri.parse("${TrustedWebOrigin.ORIGIN}/package-icons/com.example.app?x=1")))
        assertFalse(TrustedWebOrigin.isPackageIcon(Uri.parse("https://example.com/package-icons/com.example.app")))
    }
}
