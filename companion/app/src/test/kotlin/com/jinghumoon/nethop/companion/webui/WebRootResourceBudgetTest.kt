package com.jinghumoon.nethop.companion.webui

import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.junit.Test

class WebRootResourceBudgetTest {
    @Test
    fun boundsConcurrentRequestsAndCumulativeBytes() {
        val budget = WebRootResourceBudget(maxRequests = 3, maxConcurrentStreams = 1, maxTotalBytes = 10)
        assertTrue(budget.acquire(6))
        assertFalse(budget.acquire(1))
        budget.releaseStream()
        assertTrue(budget.acquire(4))
        budget.releaseStream()
        assertFalse(budget.acquire(1))
    }

    @Test
    fun defaultBudgetSupportsLazyRouteDependencyBurst() {
        val burstSize = 32
        val budget = WebRootResourceBudget(
            maxRequests = burstSize,
            maxConcurrentStreams = DEFAULT_MAX_CONCURRENT_WEBROOT_STREAMS,
            maxTotalBytes = burstSize.toLong(),
        )

        repeat(burstSize) {
            assertTrue(budget.acquire(1))
        }
        repeat(burstSize) {
            budget.releaseStream()
        }
    }
}
