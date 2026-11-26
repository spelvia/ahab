package dev.ahab.app.background

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import dev.ahab.app.MainActivity
import dev.ahab.app.R
import org.json.JSONObject

/** Ahab's notifications on the phone, and the ongoing one the service needs. */
object Notifier {
    private const val TOLD = "ahab.notifications"
    private const val ONGOING = "ahab.background"
    const val ONGOING_ID = 1

    fun channels(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(TOLD, "Agents and machines", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "An agent finished, needs you or stopped; a machine went offline."
            },
        )
        manager.createNotificationChannel(
            NotificationChannel(ONGOING, "While agents work", NotificationManager.IMPORTANCE_MIN).apply {
                description = "Shown while Ahab stays connected to tell you about them."
                setShowBadge(false)
            },
        )
    }

    /** Opens `path` of the server in the app (an `ahab://open` link). */
    private fun opening(context: Context, server: String, path: String?, code: Int): PendingIntent {
        val link = Uri.Builder()
            .scheme("ahab")
            .authority("open")
            .appendQueryParameter("server", server)
            .appendQueryParameter("path", path ?: "/")
            .build()
        val intent = Intent(Intent.ACTION_VIEW, link)
            .setClass(context, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        return PendingIntent.getActivity(
            context,
            code,
            intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }

    /** The page a notification's target opens, as the web app's routes name it. */
    fun pathOf(target: JSONObject?): String? {
        if (target == null) return null
        return when (target.optString("type")) {
            "session" -> {
                val project = target.optString("projectId")
                val session = target.optString("sessionId")
                val chat = if (target.isNull("chatId")) "" else target.optString("chatId")
                if (project.isEmpty() || session.isEmpty()) null
                else if (chat.isEmpty()) "/p/$project/s/$session"
                else "/p/$project/s/$session?chat=$chat"
            }
            "machine" -> target.optString("machineId").takeIf { it.isNotEmpty() }?.let { "/settings/machines/$it" }
            "route" -> target.optString("path").takeIf { it.startsWith("/") && !it.startsWith("//") }
            else -> null
        }
    }

    /** Shows a notification as the server sent it (`NotificationDto`). */
    fun show(context: Context, server: String, notification: JSONObject) {
        if (!NotificationManagerCompat.from(context).areNotificationsEnabled()) return
        val id = notification.optString("id")
        // One per group: a newer one of a chat replaces the older.
        val tag = notification.optString("groupKey").ifEmpty { id }
        val body = notification.optString("body").takeIf { !notification.isNull("body") && it.isNotEmpty() }
        val level = notification.optString("level")
        val built = NotificationCompat.Builder(context, TOLD)
            .setSmallIcon(R.drawable.ic_stat_ahab)
            .setContentTitle(notification.optString("title", "Ahab"))
            .setContentText(body)
            .setStyle(body?.let { NotificationCompat.BigTextStyle().bigText(it) })
            .setAutoCancel(true)
            .setCategory(NotificationCompat.CATEGORY_STATUS)
            .setPriority(
                if (level == "warning" || level == "error") NotificationCompat.PRIORITY_HIGH
                else NotificationCompat.PRIORITY_DEFAULT,
            )
            .setContentIntent(opening(context, server, pathOf(notification.optJSONObject("target")), tag.hashCode()))
            .build()
        try {
            NotificationManagerCompat.from(context).notify(tag, 0, built)
        } catch (_: SecurityException) {
            // Not allowed (Android 13's permission): nothing to show.
        }
    }

    /** The ongoing notification: what the connection is for. */
    fun ongoing(context: Context, name: String, working: Long?, server: String): android.app.Notification {
        val text = when (working) {
            null -> "Connecting to $name…"
            0L -> "Connected to $name"
            1L -> "1 agent working on $name"
            else -> "$working agents working on $name"
        }
        return NotificationCompat.Builder(context, ONGOING)
            .setSmallIcon(R.drawable.ic_stat_ahab)
            .setContentTitle("Ahab")
            .setContentText(text)
            .setOngoing(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setContentIntent(opening(context, server, "/", 0))
            .build()
    }
}
