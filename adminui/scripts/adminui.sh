#!/bin/sh
# adminui supervisor — installed at /opt/upt/apps/adminui/adminui.sh
#   adminui.sh start|stop|restart|status
# `start` backgrounds a supervisor that waits for the LAN bridge and the config
# DB, then runs the server and restarts it (with backoff) if it exits.
# Kill switch: touch /userconfig/adminui.disable

DIR=/opt/upt/apps/adminui
BIN=$DIR/adminui
LOG=$DIR/adminui.log
PIDF=/var/run/adminui-supervisor.pid
DISABLE=/userconfig/adminui.disable
PATH=/sbin:/bin:/usr/sbin:/usr/bin
export PATH

log() { echo "$(date '+%Y-%m-%d %H:%M:%S') supervisor: $*" >> "$LOG"; }

rotate() {
    if [ -f "$LOG" ] && [ "$(wc -c < "$LOG")" -gt 524288 ]; then
        mv -f "$LOG" "$LOG.1"
    fi
}

wait_ready() {
    i=0
    while [ $i -lt 300 ]; do
        if ip -4 addr show br0 2>/dev/null | grep -q 'inet ' && \
           sidbg 1 DB p DevInfo 2>/dev/null | grep -q '<Tbl'; then
            return 0
        fi
        sleep 2; i=$((i + 2))
    done
    log "LAN/DB not ready after 300s; starting anyway"
}

supervise() {
    echo $$ > "$PIDF"
    rotate
    wait_ready
    delay=5
    while [ ! -f "$DISABLE" ]; do
        rotate
        log "starting adminui"
        started=$(cut -d. -f1 /proc/uptime)
        "$BIN" serve --dir "$DIR" >> "$LOG" 2>&1
        rc=$?
        ran=$(( $(cut -d. -f1 /proc/uptime) - started ))
        log "adminui exited rc=$rc after ${ran}s"
        [ $ran -gt 120 ] && delay=5
        sleep $delay
        [ $delay -lt 60 ] && delay=$((delay * 2))
    done
    log "disabled by $DISABLE"
    rm -f "$PIDF"
}

stop() {
    if [ -f "$PIDF" ]; then
        kill "$(cat "$PIDF")" 2>/dev/null
        rm -f "$PIDF"
    fi
    killall adminui 2>/dev/null
    return 0
}

case "$1" in
    start|boot)
        [ -f "$DISABLE" ] && { echo "adminui disabled ($DISABLE)"; exit 0; }
        if [ -f "$PIDF" ] && kill -0 "$(cat "$PIDF")" 2>/dev/null; then
            echo "already running"; exit 0
        fi
        setsid "$0" supervise < /dev/null > /dev/null 2>&1 &
        echo "adminui supervisor started"
        ;;
    supervise) supervise ;;
    stop) stop; echo "adminui stopped" ;;
    restart) stop; sleep 1; "$0" start ;;
    status)
        if pidof adminui > /dev/null; then echo "running (pid $(pidof adminui))"; else echo "not running"; fi
        ;;
    *) echo "usage: $0 start|stop|restart|status"; exit 2 ;;
esac
