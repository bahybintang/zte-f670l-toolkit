#!/bin/bash
# Build adminui (web + server), push it to the router over tftp, install it
# and (re)start it. Idempotent; the previous binary is kept as adminui.prev.
set -e

readonly SCRIPT_NAME="$(basename "$0")"
readonly SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
readonly ADMINUI_DIR="$(dirname "$SCRIPT_DIR")"
readonly REPO_DIR="$(dirname "$ADMINUI_DIR")"
readonly TARGET="armv7-unknown-linux-musleabi"
readonly TFTP_DIR="$REPO_DIR/debug/tftp"
readonly TFTP_PORT=6969
readonly DEVICE_DIR="/opt/upt/apps/adminui"
readonly LOG_FILE="$REPO_DIR/debug/deploy.log"

function log {
  local readonly level="$1"
  shift
  local readonly line="$(date '+%Y-%m-%d %H:%M:%S') [$SCRIPT_NAME] [$level] $*"
  echo "$line" >&2
  echo "$line" >> "$LOG_FILE"
}
function log_info { log INFO "$@"; }
function log_warn { log WARN "$@"; }
function log_error { log ERROR "$@"; }

function print_usage {
  printf 'Usage: %s [options]\n\nBuild adminui and install it on the router.\n\nOptions:\n' "$SCRIPT_NAME" >&2
  printf '  %s\t%s\n' \
    "--router IP" "Router LAN address (default 192.168.1.1)" \
    "--host IP" "This Mac's address on the router LAN, for tftp (default 192.168.1.100)" \
    "--skip-web" "Do not rebuild the web app (reuse web/dist)" \
    "--no-start" "Install but do not (re)start the service" \
    "--help" "Show this help" >&2
}

function assert_is_installed {
  local readonly bin="$1"
  if ! command -v "$bin" > /dev/null; then
    log_error "required tool '$bin' is not installed"
    exit 1
  fi
}

function assert_not_empty {
  local readonly name="$1"
  local readonly value="$2"
  if [[ -z "$value" ]]; then
    log_error "$name must not be empty"
    exit 1
  fi
}

function telnet_run {
  "$REPO_DIR/.venv/bin/python" -m zte670 telnet "$1"
}

function ensure_tftpd {
  if lsof -nP -iUDP:"$TFTP_PORT" > /dev/null 2>&1; then
    return 0
  fi
  log_info "starting tftpd on :$TFTP_PORT serving $TFTP_DIR"
  nohup python3 "$REPO_DIR/tools/tftpd.py" --dir "$TFTP_DIR" --port "$TFTP_PORT" \
    >> "$REPO_DIR/debug/tftpd.log" 2>&1 &
  sleep 1
}

function build {
  local readonly skip_web="$1"
  if [[ "$skip_web" != "true" ]]; then
    log_info "building web app"
    (cd "$ADMINUI_DIR/web" && npm run build --silent)
  fi
  log_info "cross-compiling server for $TARGET"
  (cd "$ADMINUI_DIR/server" && cargo zigbuild --release --target "$TARGET" --quiet)
  mkdir -p "$TFTP_DIR"
  cp "$ADMINUI_DIR/server/target/$TARGET/release/adminui" "$TFTP_DIR/adminui.new"
  cp "$SCRIPT_DIR/adminui.sh" "$SCRIPT_DIR/S99zadminui" "$TFTP_DIR/"
  log_info "binary size: $(wc -c < "$TFTP_DIR/adminui.new") bytes"
}

function install_on_device {
  local readonly host="$1"
  local readonly start="$2"
  local readonly get="tftp -g -r"
  local cmd="mkdir -p $DEVICE_DIR/backups && cd $DEVICE_DIR"
  cmd+=" && $get adminui.new -l adminui.new $host $TFTP_PORT"
  cmd+=" && $get adminui.sh -l adminui.sh $host $TFTP_PORT"
  cmd+=" && $get S99zadminui -l S99zadminui $host $TFTP_PORT"
  cmd+=" && chmod 755 adminui.new adminui.sh S99zadminui"
  cmd+=" && ./adminui.new version"
  cmd+=" && { ls backups/initial-*.xml >/dev/null 2>&1 || cp /userconfig/cfg/db_user_cfg.xml backups/initial-\$(date +%s).xml; }"
  cmd+=" && ./adminui.sh stop"
  cmd+=" && { [ -f adminui ] && mv -f adminui adminui.prev; true; }"
  cmd+=" && mv -f adminui.new adminui"
  cmd+=" && cp -f S99zadminui /etc/rcS.d/S99zadminui && chmod 755 /etc/rcS.d/S99zadminui && rm -f S99zadminui"
  if [[ "$start" == "true" ]]; then
    cmd+=" && ./adminui.sh start"
  fi
  cmd+=" && echo DEPLOY_OK"
  log_info "installing on router"
  local out
  out="$(telnet_run "$cmd")"
  echo "$out" >> "$LOG_FILE"
  if [[ "$out" != *DEPLOY_OK* ]]; then
    log_error "install failed; tail of telnet output:"
    echo "$out" | tail -15 >&2
    return 1
  fi
}

function verify {
  local readonly router="$1"
  local i
  for i in $(seq 1 30); do
    if curl -fsS -m 3 "http://$router:8088/api/session" > /dev/null 2>&1; then
      log_info "adminui is up: http://$router:8088/"
      return 0
    fi
    sleep 1
  done
  log_error "adminui did not answer on http://$router:8088 within 30s (see $DEVICE_DIR/adminui.log)"
  return 1
}

function main {
  local router="192.168.1.1"
  local host="192.168.1.100"
  local skip_web="false"
  local start="true"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --router) router="$2"; shift ;;
      --host) host="$2"; shift ;;
      --skip-web) skip_web="true" ;;
      --no-start) start="false" ;;
      --help) print_usage; exit 0 ;;
      *) log_error "unknown argument $1"; print_usage; exit 1 ;;
    esac
    shift
  done
  assert_not_empty "--router" "$router"
  assert_not_empty "--host" "$host"
  mkdir -p "$(dirname "$LOG_FILE")"
  export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
  assert_is_installed cargo
  assert_is_installed cargo-zigbuild
  assert_is_installed curl
  [[ "$skip_web" == "true" ]] || assert_is_installed npm

  build "$skip_web"
  ensure_tftpd
  install_on_device "$host" "$start"
  if [[ "$start" == "true" ]]; then
    verify "$router"
  fi
}

main "$@"
