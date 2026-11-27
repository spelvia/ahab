package dev.ahab.app.background

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import org.json.JSONObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.HttpURLConnection
import java.net.URL

/**
 * The background connection: holds the server's notification
 * stream with the app's credential while agents work, and shows what it
 * tells. Android requires the ongoing notification it shows. It stops two
 * minutes after the last agent stops, unless "Stay connected" is on.
 */
class StreamService : Service() {
    companion object {
        private const val TAG = "AhabBackground"
        /** How long it stays once no agent works. */
        private const val IDLE_FOR = 2 * 60 * 1000L

        @Volatile
        var running: Boolean = false
            private set

        fun start(context: Context) {
            if (Background.config(context) == null) return
            try {
                ContextCompat.startForegroundService(context, Intent(context, StreamService::class.java))
            } catch (error: Exception) {
                // Android refuses starting it from the background: the periodic check catches up.
                Log.i(TAG, "cannot start the background connection: $error")
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, StreamService::class.java))
        }
    }

    @Volatile
    private var stopping = false
    private var worker: Thread? = null
    @Volatile
    private var connection: HttpURLConnection? = null
    private var idleSince: Long? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val config = Background.config(this)
        if (config == null) {
            stopSelf()
            return START_NOT_STICKY
        }
        Notifier.channels(this)
        ServiceCompat.startForeground(
            this,
            Notifier.ONGOING_ID,
            Notifier.ongoing(this, config.name, null, config.server),
            if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0,
        )
        running = true
        if (worker == null) {
            stopping = false
            worker = Thread({ follow() }, "ahab-stream").apply {
                isDaemon = true
                start()
            }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        stopping = true
        running = false
        connection?.disconnect()
        worker?.interrupt()
        worker = null
        super.onDestroy()
    }

    /** Connects, reads, and connects again after a pause, until stopped. */
    private fun follow() {
        var pause = 2_000L
        while (!stopping) {
            val config = Background.config(this) ?: break
            try {
                read(config)
                pause = 2_000L
            } catch (error: Unpaired) {
                Log.i(TAG, "the server no longer knows this phone")
                Background.forget(this)
                break
            } catch (error: Exception) {
                if (stopping) break
                Log.i(TAG, "the stream ended: $error")
            }
            if (stopping) break
            try {
                Thread.sleep(pause)
            } catch (_: InterruptedException) {
                break
            }
            pause = (pause * 2).coerceAtMost(60_000L)
        }
        stopSelf()
    }

    private class Unpaired : Exception()

    private fun read(config: Background.Config) {
        val opened = (URL("${config.server}/api/v1/notification-events").openConnection() as HttpURLConnection)
        connection = opened
        opened.setRequestProperty("Authorization", "Bearer ${config.token}")
        opened.setRequestProperty("Accept", "text/event-stream")
        opened.connectTimeout = 15_000
        // The server says something at least every 20 seconds.
        opened.readTimeout = 60_000
        try {
            when (opened.responseCode) {
                200 -> {}
                401, 403 -> throw Unpaired()
                else -> throw IllegalStateException("the server answered ${opened.responseCode}")
            }
            BufferedReader(InputStreamReader(opened.inputStream, Charsets.UTF_8)).use { lines ->
                while (!stopping) {
                    val line = lines.readLine() ?: return
                    if (line.startsWith("data:")) heard(config, JSONObject(line.removePrefix("data:").trim()))
                }
            }
        } finally {
            opened.disconnect()
            connection = null
        }
    }

    private fun heard(config: Background.Config, message: JSONObject) {
        when (message.optString("type")) {
            "created" -> {
                val notification = message.optJSONObject("notification") ?: return
                Background.shown(this, notification.optString("id"))
                // In front, the app shows it as a toast.
                if (!Background.inFront) Notifier.show(this, config.server, notification)
            }
            "activity" -> {
                val working = message.optLong("working")
                NotificationManagerCompat.from(this).let { manager ->
                    try {
                        manager.notify(Notifier.ONGOING_ID, Notifier.ongoing(this, config.name, working, config.server))
                    } catch (_: SecurityException) {
                    }
                }
                if (working > 0 || Background.stayConnected(this)) {
                    idleSince = null
                } else {
                    val since = idleSince ?: System.currentTimeMillis().also { idleSince = it }
                    if (System.currentTimeMillis() - since >= IDLE_FOR) {
                        Log.i(TAG, "no agent works: closing the background connection")
                        stopping = true
                        stopSelf()
                    }
                }
            }
        }
    }
}
