package com.anyplug.discovery

import android.content.Context
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map

/**
 * Persisted "last server the user successfully connected to".
 *
 * On app start, [load] returns this value so the discovery layer can
 * probe it immediately — the app feels instant after the first
 * successful connect even when mDNS is broken.
 *
 * DataStore (not SharedPreferences) is used because it is the
 * modern Android API, the project already declares it, and it gives
 * us Flow-based observation for free.
 */
internal class LastKnownServerStore(private val appContext: Context) {
    private val Context.dataStore by preferencesDataStore(name = STORE_NAME)

    suspend fun load(): ServerEndpoint? {
        val prefs = appContext.dataStore.data.first()
        return decode(prefs)
    }

    suspend fun save(endpoint: ServerEndpoint) {
        appContext.dataStore.edit { prefs ->
            prefs[KEY_HOST] = endpoint.host
            prefs[KEY_API_PORT] = endpoint.apiPort.toString()
            prefs[KEY_WIRE_PORT] = endpoint.wirePort.toString()
            endpoint.serverId?.let { prefs[KEY_SERVER_ID] = it }
            endpoint.serverName?.let { prefs[KEY_SERVER_NAME] = it }
        }
    }

    suspend fun clear() {
        appContext.dataStore.edit { it.clear() }
    }

    private fun decode(prefs: Preferences): ServerEndpoint? {
        val host = prefs[KEY_HOST] ?: return null
        val apiPort = prefs[KEY_API_PORT]?.toIntOrNull() ?: return null
        val wirePort = prefs[KEY_WIRE_PORT]?.toIntOrNull() ?: return null
        return ServerEndpoint(
            host = host,
            apiPort = apiPort,
            wirePort = wirePort,
            serverId = prefs[KEY_SERVER_ID],
            serverName = prefs[KEY_SERVER_NAME],
            source = DiscoverySource.LAST_KNOWN,
        )
    }

    private companion object {
        const val STORE_NAME = "discovery_prefs"
        val KEY_HOST = stringPreferencesKey("last_known_host")
        val KEY_API_PORT = stringPreferencesKey("last_known_api_port")
        val KEY_WIRE_PORT = stringPreferencesKey("last_known_wire_port")
        val KEY_SERVER_ID = stringPreferencesKey("last_known_server_id")
        val KEY_SERVER_NAME = stringPreferencesKey("last_known_server_name")
    }
}