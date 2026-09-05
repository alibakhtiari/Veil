package studio.cluvex.aether

import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

/**
 * Quick Settings tile: one-tap connect/disconnect from the notification
 * shade (GUI_PLAN.md §4.2).
 *
 * The tile does not talk JNI itself: it fires [VpnTunnelService] start/
 * stop intents (which require user VPN consent via
 * `VpnService.prepare()` exactly once — handled by the MainActivity,
 * not here). State shown is last-known; the service updates the tile
 * via `requestListeningState()` on transitions (TODO with the live
 * event poll in Phase-3 app work).
 *
 * Requires API 24+. ⚠️ Uncompiled here (no Android SDK).
 */
class AetherTileService : TileService() {

    override fun onClick() {
        super.onClick()
        val active = qsTile.state == Tile.STATE_ACTIVE
        val intent = Intent(this, VpnTunnelService::class.java).apply {
            action = if (active) VpnTunnelService.ACTION_STOP else VpnTunnelService.ACTION_START
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            @Suppress("DEPRECATION")
            startService(intent)
        }
        qsTile.state = if (active) Tile.STATE_INACTIVE else Tile.STATE_ACTIVE
        qsTile.updateTile()
    }

    override fun onStartListening() {
        super.onStartListening()
        qsTile.icon = Icon.createWithResource(this, android.R.drawable.ic_lock_lock)
        qsTile.updateTile()
    }
}
