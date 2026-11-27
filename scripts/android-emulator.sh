#!/bin/sh
# Starts the Android emulator without a window (AVD "ahab", created once
# with avdmanager), waits until it has booted, installs the debug APK and
# opens the app. Needs the Android SDK and emulator on PATH.
set -eu
apk=apps/native/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
if ! adb get-state >/dev/null 2>&1; then
    if ! avdmanager list avd 2>/dev/null | grep -q 'Name: ahab'; then
        echo no | avdmanager create avd -n ahab -k "system-images;android-36;google_apis;x86_64" -d pixel_7
    fi
    systemd-run --user --unit=ahab-emulator --collect \
        -E ANDROID_HOME="$ANDROID_HOME" -E ANDROID_SDK_ROOT="$ANDROID_HOME" \
        -E ANDROID_AVD_HOME="$HOME/.android/avd" \
        "$(command -v emulator)" -avd ahab -no-window -no-audio -no-boot-anim \
        -gpu swiftshader_indirect -no-snapshot-save -memory 3072 -cores 4
fi
echo "Waiting for the emulator to boot…"
until [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do sleep 3; done
adb install -r "$apk"
adb shell monkey -p dev.ahab.app -c android.intent.category.LAUNCHER 1 >/dev/null
echo "Ahab runs in the emulator. To see it from another computer, see apps/native/README."
