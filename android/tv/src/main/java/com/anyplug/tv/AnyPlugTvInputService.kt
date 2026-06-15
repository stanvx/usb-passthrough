package com.anyplug.tv

import android.content.Intent
import android.media.tv.TvInputService
import android.net.Uri
import android.view.Surface

/**
 * TvInputService that registers AnyPlug as a TV input source.
 *
 * When a USB video/audio device is imported via USB/IP, this service
 * can present its stream as a TV input channel accessible from the
 * Android TV home screen's input picker.
 *
 * Currently provides placeholder infrastructure. Full video streaming
 * integration requires the USB Video Class (UVC) bridge to pipe
 * isochronous URB data into a MediaCodec decoder.
 */
class AnyPlugTvInputService : TvInputService() {

    /**
     * Session that manages a single TV input channel.
     * Each tune request creates a new session tied to a Surface.
     */
    override fun onCreateSession(inputId: String): Session? {
        return AnyPlugSession(this)
    }

    /**
     * A single TV input session tied to a Surface for rendering.
     */
    inner class AnyPlugSession(context: AnyPlugTvInputService) : Session(context) {

        private var currentSurface: Surface? = null

        override fun onSetCaptionEnabled(enabled: Boolean) {
            // Captions not yet supported; no-op for now
        }

        override fun onSetSurface(surface: Surface?): Boolean {
            currentSurface = surface
            return true
        }

        override fun onSetStreamVolume(volume: Float) {
            // TODO: control audio level of USB audio device
        }

        override fun onTune(channelUri: Uri): Boolean {
            // TODO: parse channel URI to identify the USB device,
            //       establish USB/IP client connection, and start
            //       streaming video/audio to the surface.
            notifyVideoAvailable()
            notifyTracksChanged(null)
            return true
        }

        override fun onRelease() {
            currentSurface = null
        }

        override fun onAppPrivateCommand(action: String, data: android.os.Bundle?) {
            super.onAppPrivateCommand(action, data)
            // Reserved for custom USB/IP commands
        }
    }
}
