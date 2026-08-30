package com.jinghumoon.nethop.companion.control

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
        assertEquals(listOf("capture", "enable", "--json", "--wait"), RootOperation.CaptureEnable.command().args)
        assertEquals(listOf("capture", "disable", "--json", "--wait"), RootOperation.CaptureDisable.command().args)
        assertEquals(listOf("capture", "status", "--json"), RootOperation.CaptureStatus.command().args)
        for (operation in listOf(RootOperation.StatusGet, RootOperation.ServiceStart, RootOperation.ServiceStop)) {
            assertEquals(NETHOPCTL_PATH, operation.command().executable)
        }
        assertFalse(RootOperation.StatusGet.command().mutating)
        assertTrue(RootOperation.ServiceStart.command().mutating)
        assertTrue(RootOperation.ServiceStop.command().mutating)
    }
}
