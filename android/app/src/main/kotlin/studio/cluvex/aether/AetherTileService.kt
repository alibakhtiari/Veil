package studio.cluvex.aether

import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

/**
 * Quick Settings tile: one-tap connect/disconnect from the notification
 * shade.
 *
 * The tile does not talk JNI itself: it fires [VpnTunnelService] start/
 * stop intents (which require user VPN consent via
 * `VpnService.prepare()` exactly once — handled by the MainActivity,
 * not here). State shown is synchronized with [VpnTunnelService.isRunning].
 *
 * Requires API 24+.
 */
class AetherTileService : TileService() {

    override fun onClick() {
        super.onClick()
        val active = VpnTunnelService.isRunning
        val intent = Intent(this, VpnTunnelService::class.java).apply {
            action = if (active) VpnTunnelService.ACTION_STOP else VpnTunnelService.ACTION_START
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            @Suppress("DEPRECATION")
            startService(intent)
        }
        updateTileState(!active)
    }

    override fun onStartListening() {
        super.onStartListening()
        updateTileState(VpnTunnelService.isRunning)
    }

    private fun updateTileState(active: Boolean) {
        val tile = qsTile ?: return
        tile.state = if (active) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.label = "Veil"
        tile.icon = Icon.createWithResource(this, android.R.drawable.ic_lock_lock)
        tile.updateTile()
    }
}
