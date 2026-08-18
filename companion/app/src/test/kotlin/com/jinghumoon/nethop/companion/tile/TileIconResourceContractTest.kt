package com.jinghumoon.nethop.companion.tile

import java.nio.file.Path
import javax.xml.parsers.DocumentBuilderFactory
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import org.junit.Test
import org.w3c.dom.Element

class TileIconResourceContractTest {
    @Test
    fun onAndOffIconsShareArcsButUseSolidAndHollowNodes() {
        val off = paths("ic_nethop_tile.xml")
        val on = paths("ic_nethop_tile_on.xml")

        assertEquals(2, off.size)
        assertEquals(2, on.size)
        assertEquals(off[0].pathData, on[0].pathData)
        assertEquals("2", off[0].strokeWidth)
        assertEquals("2", on[0].strokeWidth)

        assertNotEquals(off[1].pathData, on[1].pathData)
        assertEquals(6, Regex("A1\\.6,1\\.6").findAll(off[1].pathData).count())
        assertEquals("1.8", off[1].strokeWidth)
        assertEquals("#00000000", off[1].fillColor)
        assertEquals(6, Regex("A2\\.5,2\\.5").findAll(on[1].pathData).count())
        assertEquals("#FFFFFFFF", on[1].fillColor)
    }

    private fun paths(name: String): List<VectorPath> {
        val document = DocumentBuilderFactory.newInstance().apply {
            isNamespaceAware = true
        }.newDocumentBuilder().parse(Path.of("src", "main", "res", "drawable", name).toFile())
        return (0 until document.getElementsByTagName("path").length).map { index ->
            val element = document.getElementsByTagName("path").item(index) as Element
            VectorPath(
                pathData = element.getAttributeNS(ANDROID_NAMESPACE, "pathData"),
                fillColor = element.getAttributeNS(ANDROID_NAMESPACE, "fillColor"),
                strokeWidth = element.getAttributeNS(ANDROID_NAMESPACE, "strokeWidth"),
            )
        }
    }

    private data class VectorPath(
        val pathData: String,
        val fillColor: String,
        val strokeWidth: String,
    )

    private companion object {
        const val ANDROID_NAMESPACE = "http://schemas.android.com/apk/res/android"
    }
}
