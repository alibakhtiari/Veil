package studio.cluvex.aether

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.net.Uri
import android.net.VpnService
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import org.json.JSONObject
import java.util.Locale

/**
 * LAUNCHER activity: Material 3 Jetpack Compose UI driving [VpnTunnelService].
 *
 * Provides:
 * - One-tap connect / disconnect with VPN consent check.
 * - Real-time upload/download speed metrics and cumulative transfer counters.
 * - Battery optimization exemption prompt.
 * - Live core event latency summary and tail log viewer.
 * - Traffic mode selector (VPN vs Proxy).
 */
class MainActivity : ComponentActivity() {

    private var connected by mutableStateOf(false)
    private var batteryOptimizationIgnored by mutableStateOf(true)

    private val vpnConsentLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            startVpnService()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        connected = VpnTunnelService.isRunning
        batteryOptimizationIgnored = isBatteryOptimizationIgnored()
        val prefs = getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val initialMode = prefs.getString(KEY_TRAFFIC_MODE, MODE_VPN) ?: MODE_VPN

        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    var trafficMode by remember { mutableStateOf(initialMode) }
                    var statusLine by remember { mutableStateOf("idle · no events") }
                    var logsExpanded by remember { mutableStateOf(false) }
                    var logText by remember { mutableStateOf("") }

                    // Bandwidth metrics
                    var rxBytes by remember { mutableStateOf(0L) }
                    var txBytes by remember { mutableStateOf(0L) }
                    var rxRate by remember { mutableStateOf(0L) }
                    var txRate by remember { mutableStateOf(0L) }

                    // Polling loop for core events and bandwidth stats
                    LaunchedEffect(Unit) {
                        var lastStatsTime = 0L
                        while (true) {
                            connected = VpnTunnelService.isRunning
                            val raw = safePoll { CoreBridge.eventsPoll(0) }
                            if (raw.isNotEmpty()) {
                                statusLine = summarizeEvents(raw)
                                try {
                                    val arr = JSONObject(raw).optJSONArray("events")
                                    if (arr != null) {
                                        val now = System.currentTimeMillis()
                                        val dt = if (lastStatsTime > 0) (now - lastStatsTime) / 1000.0 else 0.0
                                        for (i in 0 until arr.length()) {
                                            val ev = arr.optJSONObject(i) ?: continue
                                            when (ev.optString("type")) {
                                                "stats" -> {
                                                    val newRx = ev.optLong("rx_bytes", rxBytes)
                                                    val newTx = ev.optLong("tx_bytes", txBytes)
                                                    if (dt > 0.3 && lastStatsTime > 0) {
                                                        if (newRx >= rxBytes) rxRate = ((newRx - rxBytes) / dt).toLong()
                                                        if (newTx >= txBytes) txRate = ((newTx - txBytes) / dt).toLong()
                                                    }
                                                    rxBytes = newRx
                                                    txBytes = newTx
                                                    lastStatsTime = now
                                                }
                                                "tunnel_up" -> connected = true
                                                "tunnel_down" -> {
                                                    connected = false
                                                    rxRate = 0L
                                                    txRate = 0L
                                                }
                                            }
                                        }
                                    }
                                } catch (_: Throwable) {
                                }
                            }
                            delay(STATUS_POLL_MS)
                        }
                    }

                    // Log drawer: poll only while expanded.
                    if (logsExpanded) {
                        LaunchedEffect(Unit) {
                            while (true) {
                                logText = summarizeLogs(safePoll { CoreBridge.logPoll(200, "") })
                                delay(LOG_POLL_MS)
                            }
                        }
                    }

                    Column(
                        modifier = Modifier
                            .fillMaxSize()
                            .padding(24.dp)
                            .verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.Center,
                        horizontalAlignment = Alignment.CenterHorizontally
                    ) {
                        // Battery optimization warning banner
                        if (!batteryOptimizationIgnored) {
                            Card(
                                shape = RoundedCornerShape(12.dp),
                                colors = CardDefaults.cardColors(
                                    containerColor = MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.4f)
                                ),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(bottom = 16.dp)
                            ) {
                                Column(modifier = Modifier.padding(16.dp)) {
                                    Text(
                                        text = "Battery Optimization Active",
                                        style = MaterialTheme.typography.titleSmall,
                                        fontWeight = FontWeight.Bold
                                    )
                                    Spacer(modifier = Modifier.height(4.dp))
                                    Text(
                                        text = "System battery saving may terminate the VPN tunnel in the background.",
                                        style = MaterialTheme.typography.bodySmall
                                    )
                                    Spacer(modifier = Modifier.height(10.dp))
                                    OutlinedButton(
                                        onClick = {
                                            requestIgnoreBatteryOptimizations()
                                            batteryOptimizationIgnored = isBatteryOptimizationIgnored()
                                        }
                                    ) {
                                        Text(text = "Disable Optimization")
                                    }
                                }
                            }
                        }

                        Text(
                            text = if (connected) "Connected" else "Disconnected",
                            style = MaterialTheme.typography.headlineSmall,
                            fontWeight = FontWeight.Bold
                        )
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(
                            text = statusLine,
                            style = MaterialTheme.typography.bodySmall
                        )
                        Spacer(modifier = Modifier.height(20.dp))

                        // Real-time speed & bandwidth metrics cards
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(12.dp)
                        ) {
                            Card(
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp),
                                colors = CardDefaults.cardColors(
                                    containerColor = MaterialTheme.colorScheme.surfaceVariant
                                )
                            ) {
                                Column(modifier = Modifier.padding(14.dp)) {
                                    Text(
                                        text = "DOWNLOAD",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.outline
                                    )
                                    Spacer(modifier = Modifier.height(4.dp))
                                    Text(
                                        text = formatSpeed(rxRate),
                                        style = MaterialTheme.typography.titleMedium,
                                        fontWeight = FontWeight.Bold
                                    )
                                    Spacer(modifier = Modifier.height(2.dp))
                                    Text(
                                        text = "Total: " + formatBytes(rxBytes),
                                        style = MaterialTheme.typography.labelSmall
                                    )
                                }
                            }
                            Card(
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp),
                                colors = CardDefaults.cardColors(
                                    containerColor = MaterialTheme.colorScheme.surfaceVariant
                                )
                            ) {
                                Column(modifier = Modifier.padding(14.dp)) {
                                    Text(
                                        text = "UPLOAD",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = MaterialTheme.colorScheme.outline
                                    )
                                    Spacer(modifier = Modifier.height(4.dp))
                                    Text(
                                        text = formatSpeed(txRate),
                                        style = MaterialTheme.typography.titleMedium,
                                        fontWeight = FontWeight.Bold
                                    )
                                    Spacer(modifier = Modifier.height(2.dp))
                                    Text(
                                        text = "Total: " + formatBytes(txBytes),
                                        style = MaterialTheme.typography.labelSmall
                                    )
                                }
                            }
                        }

                        Spacer(modifier = Modifier.height(20.dp))
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.Center
                        ) {
                            Button(
                                onClick = { selectMode(prefs, { trafficMode = it }, MODE_VPN) },
                                enabled = trafficMode != MODE_VPN
                            ) {
                                Text(text = "VPN")
                            }
                            Spacer(modifier = Modifier.width(8.dp))
                            Button(
                                onClick = { selectMode(prefs, { trafficMode = it }, MODE_PROXY) },
                                enabled = trafficMode != MODE_PROXY
                            ) {
                                Text(text = "Proxy")
                            }
                        }
                        Spacer(modifier = Modifier.height(16.dp))
                        Button(
                            modifier = Modifier.fillMaxWidth(0.6f),
                            onClick = {
                                if (connected) stopVpnService() else checkAndLaunchVpn()
                            }
                        ) {
                            Text(text = if (connected) "Disconnect" else "Connect")
                        }
                        Spacer(modifier = Modifier.height(8.dp))
                        Button(onClick = { logsExpanded = !logsExpanded }) {
                            Text(text = if (logsExpanded) "Hide logs" else "Show logs")
                        }
                        if (logsExpanded) {
                            Spacer(modifier = Modifier.height(8.dp))
                            Text(
                                text = logText,
                                style = MaterialTheme.typography.labelSmall,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .heightIn(max = 220.dp)
                                    .verticalScroll(rememberScrollState())
                            )
                        }
                    }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        connected = VpnTunnelService.isRunning
        batteryOptimizationIgnored = isBatteryOptimizationIgnored()
    }

    private fun isBatteryOptimizationIgnored(): Boolean {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            val powerManager = getSystemService(Context.POWER_SERVICE) as? PowerManager
            return powerManager?.isIgnoringBatteryOptimizations(packageName) ?: true
        }
        return true
    }

    private fun requestIgnoreBatteryOptimizations() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            try {
                val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS).apply {
                    data = Uri.parse("package:$packageName")
                }
                startActivity(intent)
            } catch (_: Exception) {
                try {
                    startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
                } catch (_: Exception) {
                }
            }
        }
    }

    private fun selectMode(
        prefs: SharedPreferences,
        update: (String) -> Unit,
        mode: String
    ) {
        prefs.edit().putString(KEY_TRAFFIC_MODE, mode).apply()
        update(mode)
    }

    private fun checkAndLaunchVpn() {
        val intent = VpnService.prepare(this)
        if (intent != null) {
            vpnConsentLauncher.launch(intent)
        } else {
            startVpnService()
        }
    }

    private fun startVpnService() {
        val intent = Intent(this, VpnTunnelService::class.java).apply {
            action = VpnTunnelService.ACTION_START
            putExtra(VpnTunnelService.EXTRA_SOCKS_PORT, VpnTunnelService.DEFAULT_SOCKS_PORT)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            @Suppress("DEPRECATION")
            startService(intent)
        }
        connected = true
    }

    private fun stopVpnService() {
        val intent = Intent(this, VpnTunnelService::class.java).apply {
            action = VpnTunnelService.ACTION_STOP
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            @Suppress("DEPRECATION")
            startService(intent)
        }
        connected = false
    }

    companion object {
        private const val PREFS_NAME = "aether_prefs"
        private const val KEY_TRAFFIC_MODE = "traffic_mode"
        private const val MODE_VPN = "VPN"
        private const val MODE_PROXY = "Proxy"
        private const val STATUS_POLL_MS = 2000L
        private const val LOG_POLL_MS = 3000L
        private const val MAX_LOG_CHARS = 4000

        /** Format raw bytes into human-readable notation (e.g. 14.2 MB). */
        fun formatBytes(bytes: Long): String {
            if (bytes <= 0) return "0 B"
            val units = arrayOf("B", "KB", "MB", "GB", "TB")
            val digitGroups = (Math.log10(bytes.toDouble()) / Math.log10(1024.0)).toInt().coerceIn(0, units.size - 1)
            val value = bytes / Math.pow(1024.0, digitGroups.toDouble())
            return String.format(Locale.US, "%.1f %s", value, units[digitGroups])
        }

        /** Format raw bytes/s into human-readable transfer rate (e.g. 2.4 MB/s). */
        fun formatSpeed(bytesPerSec: Long): String {
            if (bytesPerSec <= 0) return "0 B/s"
            val units = arrayOf("B/s", "KB/s", "MB/s", "GB/s")
            val digitGroups = (Math.log10(bytesPerSec.toDouble()) / Math.log10(1024.0)).toInt().coerceIn(0, units.size - 1)
            val value = bytesPerSec / Math.pow(1024.0, digitGroups.toDouble())
            return String.format(Locale.US, "%.1f %s", value, units[digitGroups])
        }

        /** Runs [poll], returning "" on ANY failure (bad JSON, missing .so). */
        private fun safePoll(poll: () -> String): String {
            return try {
                poll()
            } catch (_: Throwable) {
                ""
            }
        }

        /** Lenient `{"events":[...]}` summary; falls back to a raw count. */
        private fun summarizeEvents(raw: String): String {
            if (raw.isEmpty()) return "events: unavailable"
            return try {
                val arr = JSONObject(raw).optJSONArray("events")
                    ?: return "events: raw ${raw.length} chars"
                if (arr.length() == 0) return "idle · no events"
                var last = ""
                var rtt = -1L
                for (i in 0 until arr.length()) {
                    val o = arr.optJSONObject(i) ?: continue
                    last = o.optString("type", last)
                    if (o.has("rtt_ms")) rtt = o.optLong("rtt_ms", rtt)
                }
                buildString {
                    append("events: ${arr.length()}")
                    if (last.isNotEmpty()) append(" · last: $last")
                    if (rtt >= 0) append(" · rtt ${rtt}ms")
                }
            } catch (_: Throwable) {
                "events: raw ${raw.length} chars"
            }
        }

        /** Lenient `{"logs":[...]}` tail; falls back to a raw count. */
        private fun summarizeLogs(raw: String): String {
            if (raw.isEmpty()) return "logs unavailable"
            return try {
                val arr = JSONObject(raw).optJSONArray("logs")
                    ?: return "logs: raw ${raw.length} chars"
                if (arr.length() == 0) return "log buffer empty"
                val start = maxOf(0, arr.length() - 30)
                val tail = buildString {
                    for (i in start until arr.length()) {
                        append(arr.opt(i)?.toString() ?: "?")
                        append('\n')
                    }
                }
                if (tail.length > MAX_LOG_CHARS) tail.takeLast(MAX_LOG_CHARS) else tail
            } catch (_: Throwable) {
                "logs: raw ${raw.length} chars"
            }
        }
    }
}
