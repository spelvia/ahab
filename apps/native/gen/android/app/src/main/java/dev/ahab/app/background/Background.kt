package dev.ahab.app.background

import android.content.Context
import android.content.SharedPreferences

/**
 * What the background connection knows between runs: the paired
 * server, the app's credential for it, whether to stay connected when no
 * agent works, and the newest notification already shown. In the app's
 * private storage, as the rest of the app's state is.
 */
object Background {
    private const val FILE = "ahab.background"

    /** Whether an activity of the app is in front: it shows its own toasts then. */
    @Volatile
    var inFront: Boolean = false

    private fun prefs(context: Context): SharedPreferences =
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    data class Config(val server: String, val token: String, val name: String)

    fun config(context: Context): Config? {
        val prefs = prefs(context)
        val server = prefs.getString("server", null) ?: return null
        val token = prefs.getString("token", null) ?: return null
        return Config(server, token, prefs.getString("name", null) ?: "Ahab")
    }

    fun configure(context: Context, config: Config) {
        prefs(context).edit()
            .putString("server", config.server.trimEnd('/'))
            .putString("token", config.token)
            .putString("name", config.name)
            .apply()
    }

    /** Forgets the server (unpaired): nothing runs until configured again. */
    fun forget(context: Context) {
        prefs(context).edit().clear().apply()
    }

    fun stayConnected(context: Context): Boolean = prefs(context).getBoolean("stay", false)

    fun setStayConnected(context: Context, stay: Boolean) {
        prefs(context).edit().putBoolean("stay", stay).apply()
    }

    /** The newest notification shown (UUID v7s sort by time). */
    fun lastShown(context: Context): String? = prefs(context).getString("lastShown", null)

    fun shown(context: Context, id: String) {
        val last = lastShown(context)
        if (last == null || id > last) prefs(context).edit().putString("lastShown", id).apply()
    }
}
