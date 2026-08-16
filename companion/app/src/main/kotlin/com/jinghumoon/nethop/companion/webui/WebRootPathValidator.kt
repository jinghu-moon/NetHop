package com.jinghumoon.nethop.companion.webui

import java.io.ByteArrayOutputStream

object WebRootPathValidator {
    private val plainSegment = Regex("^[A-Za-z0-9._@-]+$")

    fun validate(encodedPath: String, root: String = RootShellSession.WEBROOT_PATH): String? {
        val decoded = percentDecodeOnce(encodedPath) ?: return null
        if (decoded.isEmpty() || decoded.startsWith('/') || '\\' in decoded || '\u0000' in decoded || '%' in decoded) {
            return null
        }
        val segments = decoded.split('/')
        if (segments.any { it.isEmpty() || it == "." || it == ".." || !plainSegment.matches(it) }) {
            return null
        }
        val rootPath = java.nio.file.Paths.get(root).toAbsolutePath().normalize()
        val candidate = rootPath.resolve(decoded).normalize()
        if (candidate == rootPath || !candidate.startsWith(rootPath)) return null
        return decoded
    }

    private fun percentDecodeOnce(value: String): String? {
        val bytes = ByteArrayOutputStream(value.length)
        var index = 0
        while (index < value.length) {
            val character = value[index]
            if (character == '%') {
                if (index + 2 >= value.length) return null
                val high = value[index + 1].digitToIntOrNull(16) ?: return null
                val low = value[index + 2].digitToIntOrNull(16) ?: return null
                bytes.write((high shl 4) or low)
                index += 3
            } else {
                if (character.code > 0x7f) return null
                bytes.write(character.code)
                index += 1
            }
        }
        return runCatching { bytes.toByteArray().decodeToString(throwOnInvalidSequence = true) }.getOrNull()
    }
}
