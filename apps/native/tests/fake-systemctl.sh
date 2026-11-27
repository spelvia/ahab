#!/bin/sh
# systemd's user manager, pretended (as crates/ahab/tests/modules/machines/ssh.rs
# does): "starting" ahab.service runs the installed `ahab up --foreground` in
# the background, in this $HOME. Nothing reaches the real user manager. Used
# by the app's tests and by `make app-sandbox`.
s="$HOME/.fake-systemd"; mkdir -p "$s"
echo "$*" >> "$s/calls"
[ "$1" = --user ] && shift
running() { [ -f "$s/pid" ] && kill -0 "$(cat "$s/pid")" 2>/dev/null; }
start() { running || { nohup "$HOME/.local/bin/ahab" up --foreground >>"$s/log" 2>&1 </dev/null & echo $! > "$s/pid"; }; }
stop() { if running; then kill "$(cat "$s/pid")"; sleep 0.3; fi; rm -f "$s/pid"; }
case "$1" in
  start) start;;
  restart) stop; start;;
  disable) [ "$2" = --now ] && stop; rm -f "$s/enabled";;
  enable) touch "$s/enabled";;
  show)
    if [ -f "$HOME/.config/systemd/user/ahab.service" ]; then echo LoadState=loaded; else echo LoadState=not-found; fi
    if [ -f "$s/enabled" ]; then echo UnitFileState=enabled; else echo UnitFileState=disabled; fi
    if running; then echo ActiveState=active; else echo ActiveState=inactive; fi;;
esac
exit 0
