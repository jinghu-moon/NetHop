package com.jinghumoon.nethop.companion.webui

import android.content.Context
import com.jinghumoon.nethop.companion.control.PersistentRootShell
import com.topjohnwu.superuser.Shell
import com.topjohnwu.superuser.io.SuFile
import com.topjohnwu.superuser.io.SuFileInputStream
import java.io.InputStream
import java.util.concurrent.atomic.AtomicBoolean

class RootShellSession private constructor(
    private val shell: Shell,
    val manifest: WebRootManifestIndex,
) : AutoCloseable {
    private val closed = AtomicBoolean(false)

    fun open(asset: WebRootAsset): InputStream? {
        if (closed.get()) return null
        val relative = WebRootPathValidator.validate(asset.path) ?: return null
        val file = boundFile("$WEBROOT_PATH/$relative")
        if (!file.exists() || !file.isFile || file.isSymlink || file.length() != asset.bytes) return null
        return runCatching { SuFileInputStream.open(file) }.getOrNull()
    }

    private fun boundFile(path: String): SuFile = SuFile(path).also { it.setShell(shell) }

    override fun close() {
        closed.compareAndSet(false, true)
    }

    companion object {
        const val MODULE_PATH = "/data/adb/modules/nethop"
        const val WEBROOT_PATH = "$MODULE_PATH/webroot"
        const val MODULE_MANIFEST_PATH = "$MODULE_PATH/licenses/webui-asset-manifest.json"

        fun open(context: Context, expectedManifestBytes: ByteArray): RootShellSession? {
            val expected = WebRootManifestIndex.parse(expectedManifestBytes) ?: return null
            val shell = PersistentRootShell.acquire(context) ?: return null
            fun fail(): RootShellSession? {
                return null
            }
            if (shell.status != Shell.ROOT_SHELL || !shell.isAlive) return fail()
            val moduleManifest = SuFile(MODULE_MANIFEST_PATH).also { it.setShell(shell) }
            if (!moduleManifest.exists() || !moduleManifest.isFile || moduleManifest.isSymlink || moduleManifest.length() != expectedManifestBytes.size.toLong()) return fail()
            val observedManifest = runCatching {
                SuFileInputStream.open(moduleManifest).use { it.readNBytes(expectedManifestBytes.size + 1) }
            }.getOrNull() ?: return fail()
            if (!observedManifest.contentEquals(expectedManifestBytes)) return fail()
            val indexAsset = expected.asset("index.html") ?: return fail()
            val indexFile = SuFile("$WEBROOT_PATH/index.html").also { it.setShell(shell) }
            if (!indexFile.exists() || !indexFile.isFile || indexFile.isSymlink || indexFile.length() != indexAsset.bytes) return fail()
            val indexBytes = runCatching {
                SuFileInputStream.open(indexFile).use { it.readNBytes(indexAsset.bytes.toInt() + 1) }
            }.getOrNull() ?: return fail()
            if (indexBytes.size.toLong() != indexAsset.bytes || sha256(indexBytes) != indexAsset.sha256) return fail()
            return RootShellSession(shell, expected)
        }
    }
}
