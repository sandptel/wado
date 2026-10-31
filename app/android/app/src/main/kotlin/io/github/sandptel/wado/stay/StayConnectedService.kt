package io.github.sandptel.wado.stay

import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import io.github.sandptel.wado.MainActivity
import io.github.sandptel.wado.R
import io.github.sandptel.wado.notify.Notifier

/**
 * "Stay connected" (off by default): keeps wado's process — and with it the light links to the
 * computers — alive while the app is closed, so their notifications still arrive. The price is a
 * permanent, quiet notification and some battery; Android allows nothing less for a connection
 * held in the background. "Turn off" in the notification tells Dart (`stayOff`) and stops.
 */
class StayConnectedService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            onStop?.invoke()
            ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
        }
        val stop = PendingIntent.getService(
            this, 1, Intent(this, StayConnectedService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE,
        )
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val n = NotificationCompat.Builder(this, Notifier.CH_STAY)
            .setSmallIcon(R.drawable.ic_stat_wado)
            .setContentTitle(getString(R.string.stay_title))
            .setContentText(getString(R.string.stay_text))
            .setOngoing(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setContentIntent(open)
            .addAction(0, getString(R.string.stay_off), stop)
            .build()
        val type = if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING else 0
        ServiceCompat.startForeground(this, ID, n, type)
        return START_STICKY
    }

    companion object {
        private const val ID = 7
        private const val ACTION_STOP = "stop"
        /** The notification's "Turn off": the activity forwards it to Dart. */
        var onStop: (() -> Unit)? = null

        fun sync(context: Context, on: Boolean) {
            val i = Intent(context, StayConnectedService::class.java)
            if (on) ContextCompat.startForegroundService(context, i) else context.stopService(i)
        }
    }
}
