package studio.cluvex.aether

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.net.VpnService
import android.os.Build
import android.os.Bundle
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
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import org.json.JSONObject

/**
 * LAUNCHER activity: minimal Jetpack Compose UI driving [VpnTunnelService]
 * (GUI_PLAN.md §4.2).
 *
 * VPN consent follows the platform flow: [VpnService.prepare] returns a
 * consent intent when this is the first grant, which is fired through
 * [vpnConsentLauncher]; an already-granted consent returns null and the
 * service starts directly. Connect/Disconnect buttons fire
 * [VpnTunnelService.ACTION_START] / [VpnTunnelService.ACTION_STOP] intents
 * (reusing its `EXTRA_*` constants — none duplicated here).
 *
 * Declared with `ACTION_MAIN` + `CATEGORY_LAUNCHER` in
 * `AndroidManifest.xml`, so `getLaunchIntentForPackage()` now resolves and
 * the notification fallback in [VpnTunnelService] opens this activity.
 *
 * `connected` is last-requested state only (no service callback yet).
 *
 * Phase-3 app work (GUI_PLAN.md §4.5):
 * - [statusLine] polls `CoreBridge.eventsPoll(0)` every few seconds and
 *   summarises the buffered core events (count, last type, latest rtt).
 *   Parsing is lenient: any bad JSON (or a missing native lib) falls back
 *   to a raw-character count instead of crashing.
 * - [trafficMode] is a local VPN-vs-Proxy selector persisted in
 *   SharedPreferences. It currently only records the preference (the
 *   service always brings up the TUN path); the proxy-only dial path
 *   consumes it in a follow-up.
 * - The log drawer polls `CoreBridge.logPoll(200, "")` while expanded and
 *   shows the tail of the core log, leniently parsed the same way.
 *
 * ⚠️ Uncompiled here (no Android SDK); first Gradle build must verify.
 */
class MainActivity : ComponentActivity() {

    private var connected by mutableStateOf(false)

    private val vpnConsentLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            startVpnService()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val prefs = getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val initialMode = prefs.getString(KEY_TRAFFIC_MODE, MODE_VPN) ?: MODE_VPN
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    var trafficMode by remember { mutableStateOf(initialMode) }
                    var statusLine by remember { mutableStateOf("idle · no events") }
                    var logsExpanded by remember { mutableStateOf(false) }
                    var logText by remember { mutableStateOf("") }

                    // Status/latency line: poll the core event buffer.
                    LaunchedEffect(Unit) {
                        while (true) {
                            statusLine = summarizeEvents(safePoll { CoreBridge.eventsPoll(0) })
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
                            .padding(24.dp),
                        verticalArrangement = Arrangement.Center,
                        horizontalAlignment = Alignment.CenterHorizontally
                    ) {
                        Text(
                            text = if (connected) "Connected" else "Disconnected",
                            style = MaterialTheme.typography.headlineSmall
                        )
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(
                            text = statusLine,
                            style = MaterialTheme.typography.bodySmall
                        )
                        Spacer(modifier = Modifier.height(16.dp))
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
        private const val STATUS_POLL_MS = 2500L
        private const val LOG_POLL_MS = 3000L
        private const val MAX_LOG_CHARS = 4000

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
