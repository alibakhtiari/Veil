package studio.cluvex.aether

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ComponentName
import android.content.Intent
import android.net.VpnService
import android.os.Build
import android.os.ParcelFileDescriptor
import android.service.quicksettings.TileService

/**
 * TUN front end for the Veil core.
 *
 * Packet path (SOCKS-forward mode — the core stays a user-space SOCKS
 * proxy and never touches TUN fds itself):
 *
 *   apps → TUN fd (here) → hev-socks5-tunnel worker → 127.0.0.1:1819
 *   (libaether.so via [CoreBridge]) → Cloudflare edge
 *
 * Per-app routing uses [VpnService.Builder.addAllowedApplication] /
 * [addDisallowedApplication] from the [EXTRA_ALLOWED_APPS] /
 * [EXTRA_DISALLOWED_APPS] intent extras.
 */
class VpnTunnelService : VpnService() {

    companion object {
        const val ACTION_START = "studio.cluvex.aether.START"
        const val ACTION_STOP = "studio.cluvex.aether.STOP"
        const val EXTRA_SOCKS_PORT = "socks_port"
        const val EXTRA_ALLOWED_APPS = "allowed_apps"
        const val EXTRA_DISALLOWED_APPS = "disallowed_apps"
        const val DEFAULT_SOCKS_PORT = 1819
        private const val CHANNEL_ID = "aether_tunnel"
        private const val NOTIF_ID = 1

        @Volatile
        var isRunning: Boolean = false
            private set
    }

    private var tun: ParcelFileDescriptor? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val action = intent?.action
        return when (action) {
            ACTION_STOP -> {
                teardown()
                stopSelf()
                START_NOT_STICKY
            }
            ACTION_START, VpnService.SERVICE_INTERFACE, null -> {
                // Handled for explicit starts, system boot, and Always-On VPN invocations
                val socksPort = intent?.getIntExtra(EXTRA_SOCKS_PORT, DEFAULT_SOCKS_PORT)
                    ?: DEFAULT_SOCKS_PORT
                try {
                    establishTun(intent, socksPort)
                } catch (e: Exception) {
                    notifyError(e.message ?: "VPN setup failed")
                    stopSelf()
                }
                START_STICKY
            }
            else -> {
                val socksPort = intent.getIntExtra(EXTRA_SOCKS_PORT, DEFAULT_SOCKS_PORT)
                try {
                    establishTun(intent, socksPort)
                } catch (e: Exception) {
                    notifyError(e.message ?: "VPN setup failed")
                    stopSelf()
                }
                START_STICKY
            }
        }
    }

    private fun establishTun(intent: Intent?, socksPort: Int) {
        val builder = Builder()
            .setSession("Veil")
            .addAddress("10.0.0.2", 24)
            .addRoute("0.0.0.0", 0)
            .addDnsServer("1.1.1.1")
            .addAddress("fd00::2", 120)
            .addRoute("::", 0)
            .addDnsServer("2606:4700:4700::1111")
            .setBlocking(false)
            .setMtu(1500)

        intent?.getStringArrayExtra(EXTRA_ALLOWED_APPS)?.forEach { pkg ->
            if (pkg.isNotBlank()) builder.addAllowedApplication(pkg)
        }
        intent?.getStringArrayExtra(EXTRA_DISALLOWED_APPS)?.forEach { pkg ->
            if (pkg.isNotBlank()) builder.addDisallowedApplication(pkg)
        }

        // Our own traffic must bypass the tunnel or the SOCKS dial loops back.
        builder.addDisallowedApplication(packageName)

        tun = builder.establish()
            ?: throw IllegalStateException("VpnService.Builder.establish() returned null")

        startForeground(NOTIF_ID, buildNotification("Connecting on port $socksPort…"))
        val fd = tun?.fd ?: throw IllegalStateException("TUN fd unavailable")
        HevSocks5Tunnel.start(fd, "127.0.0.1", socksPort)
        isRunning = true
        notifyTileUpdate()
    }

    private fun teardown() {
        HevSocks5Tunnel.stop()
        try {
            tun?.close()
        } catch (_: Exception) {
        }
        tun = null
        isRunning = false
        notifyTileUpdate()
        stopForeground(STOP_FOREGROUND_REMOVE)
    }

    private fun notifyTileUpdate() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            try {
                TileService.requestListeningState(
                    this,
                    ComponentName(this, AetherTileService::class.java)
                )
            } catch (_: Exception) {
            }
        }
    }

    override fun onRevoke() {
        teardown()
        super.onRevoke()
    }

    override fun onDestroy() {
        teardown()
        super.onDestroy()
    }

    private fun buildNotification(text: String): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Veil Tunnel", NotificationManager.IMPORTANCE_LOW)
            )
        }
        // No launcher activity exists yet (Phase-3 app work), so
        // getLaunchIntentForPackage() returns null — passing it raw
        // would NPE inside getActivity. Fall back to a no-op explicit
        // intent until MainActivity replaces it.
        val launchIntent = packageManager.getLaunchIntentForPackage(packageName) ?: Intent()
        val openIntent = PendingIntent.getActivity(
            this, 0,
            launchIntent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
        return Notification.Builder(this, CHANNEL_ID)
            .setContentTitle("Veil")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.ic_lock_lock)
            .setContentIntent(openIntent)
            .setOngoing(true)
            .build()
    }

    private fun notifyError(message: String) {
        val manager = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Veil Tunnel", NotificationManager.IMPORTANCE_LOW)
            )
        }
        manager.notify(
            NOTIF_ID + 1,
            Notification.Builder(this, CHANNEL_ID)
                .setContentTitle("Veil failed to start")
                .setContentText(message)
                .setSmallIcon(android.R.drawable.ic_dialog_alert)
                .build()
        )
    }
}
