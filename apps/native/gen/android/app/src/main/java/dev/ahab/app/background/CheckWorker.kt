package dev.ahab.app.background

import android.content.Context
import android.util.Log
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.TimeUnit

/**
 * The periodic check: every 15 minutes or so, fetches the unread
 * notifications the connection missed (after a restart, or when Android
 * stopped it) and shows the new ones.
 */
class CheckWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    companion object {
        private const val NAME = "ahab.check"

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<CheckWorker>(15, TimeUnit.MINUTES)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }

        fun cancel(context: Context) {
            WorkManager.getInstance(context).cancelUniqueWork(NAME)
        }
    }

    override fun doWork(): Result {
        val config = Background.config(applicationContext) ?: return Result.success()
        // The connection shows them as they come.
        if (StreamService.running) return Result.success()
        return try {
            val opened = URL("${config.server}/api/v1/notifications?unread=true&limit=20")
                .openConnection() as HttpURLConnection
            opened.setRequestProperty("Authorization", "Bearer ${config.token}")
            opened.connectTimeout = 15_000
            opened.readTimeout = 20_000
            val code = opened.responseCode
            if (code == 401 || code == 403) {
                Background.forget(applicationContext)
                return Result.success()
            }
            if (code != 200) return Result.retry()
            val inbox = JSONObject(opened.inputStream.bufferedReader().use { it.readText() })
            opened.disconnect()
            val items = inbox.optJSONArray("items") ?: return Result.success()
            val last = Background.lastShown(applicationContext)
            // Oldest first, those newer than the last one shown.
            for (index in items.length() - 1 downTo 0) {
                val notification = items.getJSONObject(index)
                val id = notification.optString("id")
                if (last != null && id <= last) continue
                if (last != null || index < 3) Notifier.show(applicationContext, config.server, notification)
                Background.shown(applicationContext, id)
            }
            Result.success()
        } catch (error: Exception) {
            Log.i("AhabBackground", "the check failed: $error")
            Result.retry()
        }
    }
}
