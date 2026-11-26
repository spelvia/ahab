package dev.ahab.app.background

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.NotificationManagerCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class ConfigureArgs {
    lateinit var server: String
    lateinit var token: String
    var name: String = "Ahab"
}

@InvokeArg
class WorkingArgs {
    var working: Long = 0
}

@InvokeArg
class StayArgs {
    var stay: Boolean = false
}

/**
 * The app's side of background notifications on Android: what the Rust bridge asks of the
 * background connection. Registered by the app (`background.rs`).
 */
@TauriPlugin(
    permissions = [Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "notifications")],
)
class BackgroundPlugin(private val activity: Activity) : Plugin(activity) {
    private val context get() = activity.applicationContext

    private fun status(): JSObject {
        val allowed = NotificationManagerCompat.from(context).areNotificationsEnabled()
        val asked = Build.VERSION.SDK_INT >= 33 &&
            activity.shouldShowRequestPermissionRationale(Manifest.permission.POST_NOTIFICATIONS)
        return JSObject().apply {
            put("permission", if (allowed) "granted" else if (asked || Build.VERSION.SDK_INT < 33) "denied" else "prompt")
            put("stayConnected", Background.stayConnected(context))
            put("running", StreamService.running)
            put("configured", Background.config(context) != null)
        }
    }

    override fun onResume(activity: AppCompatActivity) {
        Background.inFront = true
    }

    override fun onPause(activity: AppCompatActivity) {
        Background.inFront = false
    }

    /** The server and the credential: kept for the service and the check. */
    @Command
    fun configure(invoke: Invoke) {
        val args = invoke.parseArgs(ConfigureArgs::class.java)
        Background.configure(context, Background.Config(args.server, args.token, args.name))
        Notifier.channels(context)
        CheckWorker.schedule(context)
        if (Background.stayConnected(context)) StreamService.start(context)
        invoke.resolve(status())
    }

    @Command
    fun forget(invoke: Invoke) {
        StreamService.stop(context)
        CheckWorker.cancel(context)
        Background.forget(context)
        invoke.resolve(status())
    }

    /** How many agents work, from the open app: connect while some do. */
    @Command
    fun working(invoke: Invoke) {
        val args = invoke.parseArgs(WorkingArgs::class.java)
        if (args.working > 0 || Background.stayConnected(context)) StreamService.start(context)
        invoke.resolve(status())
    }

    @Command
    fun setStayConnected(invoke: Invoke) {
        val args = invoke.parseArgs(StayArgs::class.java)
        Background.setStayConnected(context, args.stay)
        if (args.stay) StreamService.start(context)
        invoke.resolve(status())
    }

    @Command
    fun status(invoke: Invoke) {
        invoke.resolve(status())
    }

    /** Android 13's notification permission; before it, notifications are on. */
    @Command
    fun allow(invoke: Invoke) {
        if (Build.VERSION.SDK_INT < 33 || NotificationManagerCompat.from(context).areNotificationsEnabled()) {
            invoke.resolve(status())
            return
        }
        requestPermissionForAlias("notifications", invoke, "allowed")
    }

    @PermissionCallback
    private fun allowed(invoke: Invoke) {
        invoke.resolve(status())
    }

    /** Opens the settings where Android stops apps in the background. */
    @Command
    fun batterySettings(invoke: Invoke) {
        val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
            .setData(Uri.parse("package:${context.packageName}"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        activity.startActivity(intent)
        invoke.resolve(status())
    }
}
