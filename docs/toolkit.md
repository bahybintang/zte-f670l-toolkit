# The `zte670` toolkit and helper tools

Python tools that run on your computer and work on the router's configuration files, or on the
router over root telnet. adminui does most of this from the browser. The toolkit is for when you
want the raw files, or don't want to install anything on the router.

## Setup

```bash
python3 -m venv .venv && . .venv/bin/activate
pip install -r requirements.txt      # pycryptodomex and zte-cfgs
```

## Commands

```bash
python3 -m zte670 unpack    backup.cfg config.xml           # USB config backup (.cfg) → plaintext XML
python3 -m zte670 pack      config.xml out.cfg -t backup.cfg # plaintext XML → .cfg, using the original as template
python3 -m zte670 wifi      config.xml                      # list every network with its SSID and password
python3 -m zte670 set-wifi  config.xml 5 --ssid NAME --psk PASS -o new.xml   # edit one Wi-Fi network
python3 -m zte670 unpack-db db_user_cfg.xml live.xml        # live DB file → XML
python3 -m zte670 pack-db   live.new.xml db_user_cfg.new -t db_user_cfg.xml  # XML → DB file, byte-exact
python3 -m zte670 telnet    "<command>" ["<command>" …]       # run commands as root (this can change things)
python3 -m zte670 --ip 192.168.1.1 …                        # a different router address
```

- **Wi-Fi network numbers:** `1`–`4` are the 2.4 GHz networks and `5`–`8` the 5 GHz ones.
  Passwords live in the `WLANPSK` table (`KeyPassphrase`).
- **`pack-db`** writes a file the router accepts byte for byte. An edit that keeps every value's
  length is exact. A length-changing edit should go through `pack` and the stock UI's USB
  restore page instead.
- **`wifi` and the decoded XML show passwords in plain text.** Keep them out of shared places.

## Change 5 GHz through the config file

This route works without adminui, because it never touches the broken stock handler (see
[5ghz-bug.md](5ghz-bug.md)). It does need a reboot. Replace `<your-ip>` with your computer's
address on the router's LAN.

```bash
mkdir -p transfer && python3 tools/tftpd.py --dir transfer --port 6969 &

# 1. pull the live config
python3 -m zte670 telnet "cd /userconfig/cfg && tftp -p -l db_user_cfg.xml -r db_user_cfg.xml <your-ip> 6969"
cp transfer/db_user_cfg.xml transfer/db_user_cfg.orig        # keep the original to revert

# 2. edit
python3 -m zte670 unpack-db transfer/db_user_cfg.xml live.xml
python3 -m zte670 set-wifi  live.xml 5 --ssid "MyWifi-5G" --psk "my-password" -o live.new.xml
python3 -m zte670 pack-db   live.new.xml transfer/db_user_cfg.new -t transfer/db_user_cfg.xml

# 3. push it back to both copies and reboot
python3 -m zte670 telnet \
  "cd /userconfig/cfg && tftp -g -l db_user_cfg.xml -r db_user_cfg.new <your-ip> 6969" \
  "cd /userconfig/cfg && tftp -g -l db_backup_cfg.xml -r db_user_cfg.new <your-ip> 6969" \
  "reboot"
```

To revert, push `db_user_cfg.orig` back the same way and reboot.

## Helper tools

| Tool | What it does |
|---|---|
| `tools/tftpd.py` | Minimal TFTP server, the reliable way to move files to and from the router |
| `tools/set_wifi_live.py` | Sets a network's SSID and password with the firmware's own DB tool (`sidbg`) over telnet, then reboots (or `--no-reboot`). `--show` lists the current SSIDs |
| `tools/apply_wifi.py` | The config-file route above as one command, with `--dry-run` and `--revert` |
| `tools/webui/server.py` | A small local web panel (`http://127.0.0.1:8666`) that changes any Wi-Fi network live over telnet with `iwpriv` and `sidbg`. adminui is its successor |

`tools/apply_wifi.py` assumes your computer is at `192.168.1.100`; edit `MAC_IP` at the top if
it isn't.
