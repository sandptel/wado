package io.github.sandptel.wado.media

import android.Manifest
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import android.util.Base64
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.media.app.NotificationCompat.MediaStyle
import io.github.sandptel.wado.MainActivity
import io.github.sandptel.wado.R
import io.github.sandptel.wado.bridge.BridgeProtocol
import io.github.sandptel.wado.bridge.Hub
import io.github.sandptel.wado.bridge.NowPlaying
import io.github.sandptel.wado.notify.Notifier
import java.net.URL
import java.util.concurrent.Executors

/**
 * The computer's now-playing (its MPRIS player, via the page's host state) as this phone's own
 * media session: on the lock screen, in the shade, and on Bluetooth buttons. Play/pause, next,
 * previous and seek go back to the computer through the page.
 *
 * androidx.media's compat session, deprecated for media3 — whose session needs a `Player`, a
 * much larger shape for a remote that plays nothing locally. Moving is a follow-up.
 */
@Suppress("DEPRECATION")
class RemoteMedia(private val context: Context) {
    private val session = MediaSessionCompat(context, "wado-computer").apply {
        setCallback(object : MediaSessionCompat.Callback() {
            override fun onPlay() = op("play_pause")
            override fun onPause() = op("play_pause")
            override fun onSkipToNext() = op("next")
            override fun onSkipToPrevious() = op("previous")
            override fun onSeekTo(pos: Long) = Hub.send(BridgeProtocol.MEDIA) {
                put("op", "seek_to")
                put("ms", pos)
            }
        })
        setSessionActivity(
            PendingIntent.getActivity(
                context, 0, Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
            ),
        )
    }
    private val main = Handler(Looper.getMainLooper())
    private val io = Executors.newSingleThreadExecutor()
    private var artKey = ""
    private var art: Bitmap? = null
    private var last = NowPlaying()

    private fun op(name: String) = Hub.send(BridgeProtocol.MEDIA) { put("op", name) }

    fun update(np: NowPlaying) {
        last = np
        if (np.isEmpty) {
            session.isActive = false
            NotificationManagerCompat.from(context).cancel(NOTIFICATION)
            return
        }
        loadArt(np.art)
        var actions = PlaybackStateCompat.ACTION_PLAY_PAUSE or PlaybackStateCompat.ACTION_PLAY or
            PlaybackStateCompat.ACTION_PAUSE
        if (np.canNext) actions = actions or PlaybackStateCompat.ACTION_SKIP_TO_NEXT
        if (np.canPrev) actions = actions or PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS
        if (np.canSeek) actions = actions or PlaybackStateCompat.ACTION_SEEK_TO
        session.setPlaybackState(
            PlaybackStateCompat.Builder()
                .setActions(actions)
                .setState(
                    if (np.playing) PlaybackStateCompat.STATE_PLAYING else PlaybackStateCompat.STATE_PAUSED,
                    np.positionMs,
                    if (np.playing) 1f else 0f,
                )
                .build(),
        )
        session.setMetadata(
            MediaMetadataCompat.Builder()
                .putString(MediaMetadataCompat.METADATA_KEY_TITLE, np.title.ifBlank { np.app })
                .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, np.artist)
                .putString(MediaMetadataCompat.METADATA_KEY_ALBUM, np.app)
                .putLong(MediaMetadataCompat.METADATA_KEY_DURATION, if (np.lengthMs > 0) np.lengthMs else -1)
                .putBitmap(MediaMetadataCompat.METADATA_KEY_ALBUM_ART, art)
                .build(),
        )
        session.isActive = true
        post(np)
    }

    private fun post(np: NowPlaying) {
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return
        val n = NotificationCompat.Builder(context, Notifier.CH_MEDIA)
            .setSmallIcon(R.drawable.ic_stat_wado)
            .setContentTitle(np.title.ifBlank { np.app })
            .setContentText(listOf(np.artist, np.app).filter { it.isNotBlank() }.joinToString(" · "))
            .setSubText(context.getString(R.string.media_on_computer))
            .setLargeIcon(art)
            .setSilent(true)
            .setOngoing(false)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setContentIntent(session.controller.sessionActivity)
            .setStyle(MediaStyle().setMediaSession(session.sessionToken))
            .build()
        NotificationManagerCompat.from(context).notify(NOTIFICATION, n)
    }

    /** Cover art: a `data:` URI decoded here, an https URL fetched once off the main thread. */
    private fun loadArt(src: String) {
        if (src == artKey) return
        artKey = src
        art = null
        if (src.isBlank()) return
        if (src.startsWith("data:")) {
            art = runCatching {
                val bytes = Base64.decode(src.substringAfter(','), Base64.DEFAULT)
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
            }.getOrNull()
            return
        }
        if (!src.startsWith("https://")) return
        io.execute {
            val bmp = runCatching { URL(src).openStream().use(BitmapFactory::decodeStream) }.getOrNull()
            main.post {
                if (artKey == src && bmp != null) {
                    art = bmp
                    update(last)
                }
            }
        }
    }

    companion object {
        private const val NOTIFICATION = 9
    }
}
