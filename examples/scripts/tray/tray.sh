#!/usr/bin/env bash
# tray.sh — Reference abar tray script.
#
# Connects to trayd's Unix socket, subscribes to the SNI item-update event stream,
# and emits one JSON array of tray items per stdout line.  abar reads those lines
# as Vec<MinimalTrayItem>.
#
# Click behaviour is configured in [tray] in config.toml, not here.
#
# Requires: socat, jq, trayd
# Socket:   $XDG_RUNTIME_DIR/trayd.sock

SOCKET="${XDG_RUNTIME_DIR}/trayd.sock"

# Subscribe and stream events from trayd.
# `{ printf ...; sleep infinity; }` keeps the socat connection open after the
# subscribe message has been delivered — socat closes on stdin EOF by default.
read_events() {
    { printf '{"v":1,"cmd":"subscribe"}\n'; sleep infinity; } \
        | socat - "UNIX-CONNECT:$SOCKET" 2>/dev/null
}

# Outer loop: reconnect automatically when trayd is not running or restarts.
while true; do
    read_events | while IFS= read -r line; do
        items=$(printf '%s' "$line" \
            | jq -c 'select(.type == "event" and .event.kind == "update") | .event.items' \
            2>/dev/null)
        [ -n "$items" ] && printf '%s\n' "$items"
    done
    # trayd not available or disconnected; pause before retrying.
    sleep 3
done
