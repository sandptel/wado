package io.github.sandptel.wado.media

import android.Manifest
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Build
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
import io.github.sandptel.wado.notify.Notifier

/**
 * The computer's now-playing as this phone's own media session (from the native app's
 * RemoteMedia): on the lock screen, in the shade, and on Bluetooth buttons. Play/pause, next,
 * previous and seek go back to the computer through Dart (`op`).
 *
 * androidx.media's compat session: media3's needs a `Player`, a much larger shape for a remote
 * that plays nothing locally.
 */
@Suppress("DEPRECATION")
class RemoteMedia(private val context: Context, private val op: (String, Long?) -> Unit) {
    private val session = MediaSessionCompat(context, "wado-computer").apply {
        setCallback(object : MediaSessionCompat.Callback() {
            override fun onPlay() = op("play_pause", null)
            override fun onPause() = op("play_pause", null)
            override fun onSkipToNext() = op("next", null)
            override fun onSkipToPrevious() = op("previous", null)
            override fun onSeekTo(pos: Long) = op("seek_to", pos)
        })
        setSessionActivity(PendingIntent.getActivity(context, 0, Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE))
    }
    private var artKey = ""
    private var art: Bitmap? = null

    /** {title, artist, app, art, playing, position_ms, length_ms, can_next, can_prev, can_seek}, or empty. */
    fun update(np: Map<String, Any?>) {
        if (np.isEmpty()) {
            session.isActive = false
            NotificationManagerCompat.from(context).cancel(NOTIFICATION)
            return
        }
        val title = (np["title"] as? String).orEmpty()
        val app = (np["app"] as? String).orEmpty()
        val artist = (np["artist"] as? String).orEmpty()
        val playing = np["playing"] == true
        loadArt((np["art"] as? String).orEmpty())
        var actions = PlaybackStateCompat.ACTION_PLAY_PAUSE or PlaybackStateCompat.ACTION_PLAY or PlaybackStateCompat.ACTION_PAUSE
        if (np["can_next"] == true) actions = actions or PlaybackStateCompat.ACTION_SKIP_TO_NEXT
        if (np["can_prev"] == true) actions = actions or PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS
        if (np["can_seek"] == true) actions = actions or PlaybackStateCompat.ACTION_SEEK_TO
        val length = (np["length_ms"] as? Number)?.toLong() ?: 0L
        session.setPlaybackState(
            PlaybackStateCompat.Builder()
                .setActions(actions)
                .setState(
                    if (playing) PlaybackStateCompat.STATE_PLAYING else PlaybackStateCompat.STATE_PAUSED,
                    (np["position_ms"] as? Number)?.toLong() ?: 0L,
                    if (playing) 1f else 0f,
                )
                .build(),
        )
        session.setMetadata(
            MediaMetadataCompat.Builder()
                .putString(MediaMetadataCompat.METADATA_KEY_TITLE, title.ifBlank { app })
                .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, artist)
                .putString(MediaMetadataCompat.METADATA_KEY_ALBUM, app)
                .putLong(MediaMetadataCompat.METADATA_KEY_DURATION, if (length > 0) length else -1)
                .putBitmap(MediaMetadataCompat.METADATA_KEY_ALBUM_ART, art)
                .build(),
        )
        session.isActive = true
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return
        val n = NotificationCompat.Builder(context, Notifier.CH_MEDIA)
            .setSmallIcon(R.drawable.ic_stat_wado)
            .setContentTitle(title.ifBlank { app })
            .setContentText(listOf(artist, app).filter { it.isNotBlank() }.joinToString(" · "))
            .setSubText(context.getString(R.string.media_on_computer))
            .setLargeIcon(art)
            .setSilent(true)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setContentIntent(session.controller.sessionActivity)
            .setStyle(MediaStyle().setMediaSession(session.sessionToken))
            .build()
        NotificationManagerCompat.from(context).notify(NOTIFICATION, n)
    }

    /** Cover art from a `data:` URI. ponytail: no https fetch — the daemon sends art inline. */
    private fun loadArt(src: String) {
        if (src == artKey) return
        artKey = src
        art = if (!src.startsWith("data:")) null else runCatching {
            val bytes = Base64.decode(src.substringAfter(','), Base64.DEFAULT)
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
        }.getOrNull()
    }

    companion object {
        private const val NOTIFICATION = 9
    }
}
