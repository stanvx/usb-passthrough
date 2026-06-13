package com.anyplug.tv

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Bundle
import android.os.IBinder
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.anyplug.AnyPlugService
import com.anyplug.bridge.RustBridge

/**
 * Android TV Main Activity — Leanback-style UI for D-pad navigation.
 *
 * Designed for the 10-foot experience:
 *  - Large text and touch targets
 *  - Horizontal card scrolling
 *  - D-pad navigation (no touch required)
 *  - Simplified: connect-only mode (TV typically acts as client)
 */
class TvMainActivity : ComponentActivity() {

    private var service: AnyPlugService? = null
    private var serviceBound = false

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, binder: IBinder?) {
            service = (binder as AnyPlugService.LocalBinder).getService()
            serviceBound = true
        }
        override fun onServiceDisconnected(name: ComponentName?) {
            service = null
            serviceBound = false
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        RustBridge.init()

        bindService(
            Intent(this, AnyPlugService::class.java),
            serviceConnection,
            Context.BIND_AUTO_CREATE
        )

        setContent {
            MaterialTheme {
                TvScreen()
            }
        }
    }

    override fun onDestroy() {
        if (serviceBound) unbindService(serviceConnection)
        super.onDestroy()
    }
}

@Composable
fun TvScreen() {
    var connected by remember { mutableStateOf(false) }
    var deviceName by remember { mutableStateOf("") }
    val focusRequester = remember { FocusRequester() }

    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background
    ) {
        Column(
            modifier = Modifier.padding(48.dp),
            verticalArrangement = Arrangement.Center
        ) {
            Text(
                text = "AnyPlug",
                style = MaterialTheme.typography.headlineLarge,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.padding(bottom = 32.dp)
            )

            if (!connected) {
                Text(
                    text = "Discovering USB/IP servers on your network...",
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(bottom = 24.dp)
                )

                // D-pad navigable server list
                listOf("Living Room PC", "Office PC", "Raspberry Pi").forEach { server ->
                    Card(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 8.dp)
                            .focusRequester(focusRequester)
                            .focusable()
                    ) {
                        Row(
                            modifier = Modifier.padding(24.dp),
                            horizontalArrangement = Arrangement.SpaceBetween
                        ) {
                            Column {
                                Text(
                                    text = server,
                                    style = MaterialTheme.typography.titleMedium
                                )
                                Text(
                                    text = "Devices: Logitech G920, Webcam",
                                    style = MaterialTheme.typography.bodySmall
                                )
                            }
                        }
                    }
                }
            } else {
                Text(
                    text = "Connected: $deviceName",
                    style = MaterialTheme.typography.titleLarge,
                    color = MaterialTheme.colorScheme.primary
                )
            }
        }
    }
}
