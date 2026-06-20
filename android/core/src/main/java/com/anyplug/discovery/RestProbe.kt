package com.anyplug.discovery

import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * Probe an AnyPlug server's REST API (`GET /api/status`) and parse
 * the response into a [ServerEndpoint].
 *
 * Returns null when the host is unreachable, doesn't speak our API,
 * or times out. The 500 ms timeout is short on purpose — subnet scans
 * hit hundreds of hosts.
 */
internal object RestProbe {
    private const val TAG = "RestProbe"

    private val client: OkHttpClient by lazy {
        OkHttpClient.Builder()
            .connectTimeout(500, TimeUnit.MILLISECONDS)
            .readTimeout(500, TimeUnit.MILLISECONDS)
            .callTimeout(800, TimeUnit.MILLISECONDS)
            .retryOnConnectionFailure(false)
            .build()
    }

    suspend fun probe(host: String, apiPort: Int): ServerEndpoint? = withContext(Dispatchers.IO) {
        val url = "http://$host:$apiPort/api/status"
        val request = Request.Builder().url(url).get().build()
        try {
            client.newCall(request).execute().use { response ->
                if (!response.isSuccessful) return@withContext null
                val body = response.body?.string() ?: return@withContext null
                parseStatusJson(body, host, apiPort)
            }
        } catch (_: Exception) {
            null
        }
    }

    /** Parse the JSON response without bringing in a JSON library. */
    private fun parseStatusJson(json: String, host: String, apiPort: Int): ServerEndpoint? {
        return try {
            val obj = JSONObject(json)
            val status = obj.optString("status", "")
            if (status != "running") return null
            val wirePort = obj.optInt("port", 3240).coerceIn(1, 65535)
            ServerEndpoint(
                host = host,
                apiPort = apiPort,
                wirePort = wirePort,
                serverId = obj.optString("server_id").takeIf { it.isNotBlank() && it != "null" },
                serverName = obj.optString("server_name").takeIf { it.isNotBlank() && it != "null" },
                source = DiscoverySource.REST,
            )
        } catch (_: Exception) {
            null
        }
    }
}
