package com.jinghumoon.nethop.companion.control

import java.io.ByteArrayInputStream
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.junit.Test

class RootOperationTest {
    @Test
    fun mapsOnlyFixedControlCommands() {
        assertEquals(listOf("status", "--json"), RootOperation.StatusGet.command().args)
        assertEquals(listOf("start", "--json", "--wait"), RootOperation.ServiceStart.command().args)
        assertEquals(listOf("stop", "--json", "--wait"), RootOperation.ServiceStop.command().args)
        for (operation in listOf(RootOperation.StatusGet, RootOperation.ServiceStart, RootOperation.ServiceStop)) {
            assertEquals(NETHOPCTL_PATH, operation.command().executable)
        }
        assertFalse(RootOperation.StatusGet.command().mutating)
        assertTrue(RootOperation.ServiceStart.command().mutating)
        assertTrue(RootOperation.ServiceStop.command().mutating)
    }

    @Test
    fun boundedReaderDrainsButNeverStoresBeyondLimit() {
        val result = readBounded(ByteArrayInputStream(ByteArray(32) { it.toByte() }), 8)
        assertTrue(result.exceeded)
        assertEquals(8, result.bytes.size)
        assertFalse(readBounded(ByteArrayInputStream(byteArrayOf(1, 2)), 8).exceeded)
    }
}
