/**
 * The desktop (and later Android) app's own screens: first launch,
 * servers, pairing, this computer. They are part of the web app, one UI for
 * every client; the app shows them from its own bundled copy before a server
 * is connected, and only there do they reach the app's privileged commands.
 */
export { NativeApp, isBuiltinScreen } from "@/modules/native/shell";
export type { Boot, ThisComputer } from "@/modules/native/shell";
