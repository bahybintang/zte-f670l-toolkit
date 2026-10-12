"""Minimal TFTP server (octet) for backing up / restoring the router over LAN.

Handles RRQ (device GET, i.e. push a file to the router) and WRQ (device PUT,
i.e. pull a file off the router). Serves/stores files in --dir. Single-threaded,
one transfer at a time; fine for a few config files and partitions.
"""
import argparse
import os
import socket
import struct

OPCODE = {1: "RRQ", 2: "WRQ", 3: "DATA", 4: "ACK", 5: "ERROR", 6: "OACK"}


def parse_rq(data):
    parts = data[2:].split(b"\0")
    name = parts[0].decode("latin1")
    mode = parts[1].decode("latin1").lower() if len(parts) > 1 else "octet"
    opts = {}
    rest = parts[2:]
    for i in range(0, len(rest) - 1, 2):
        if rest[i]:
            opts[rest[i].decode("latin1").lower()] = rest[i + 1].decode("latin1")
    return name, mode, opts


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=".")
    ap.add_argument("--host", default="0.0.0.0")
    ap.add_argument("--port", type=int, default=6969)
    a = ap.parse_args()
    os.makedirs(a.dir, exist_ok=True)

    srv = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    srv.bind((a.host, a.port))
    print(f"tftpd on {a.host}:{a.port} dir={a.dir}", flush=True)

    while True:
        data, client = srv.recvfrom(65536)
        op = struct.unpack("!H", data[:2])[0]
        name, mode, opts = parse_rq(data) if op in (1, 2) else ("", "", {})
        safe = os.path.join(a.dir, os.path.basename(name))
        xfer = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        xfer.bind((a.host, 0))
        xfer.settimeout(10)
        blksize = int(opts.get("blksize", 512))
        try:
            if op == 2:  # WRQ: device sends us a file
                print(f"WRQ {name} -> {safe}", flush=True)
                # OACK if options, else ACK block 0
                if opts:
                    oack = b"\0\6" + b"".join(
                        k.encode() + b"\0" + v.encode() + b"\0" for k, v in opts.items())
                    xfer.sendto(oack, client)
                else:
                    xfer.sendto(b"\0\4\0\0", client)
                with open(safe, "wb") as f:
                    expect = 1
                    while True:
                        pkt, client = xfer.recvfrom(blksize + 4)
                        blk = struct.unpack("!H", pkt[2:4])[0]
                        if pkt[1] == 3 and blk == expect:
                            f.write(pkt[4:])
                            xfer.sendto(b"\0\4" + struct.pack("!H", blk), client)
                            if len(pkt) - 4 < blksize:
                                break
                            expect = (expect + 1) & 0xFFFF
                print(f"  wrote {os.path.getsize(safe)} bytes", flush=True)
            elif op == 1:  # RRQ: device wants a file from us
                print(f"RRQ {name} <- {safe}", flush=True)
                payload = open(safe, "rb").read()
                if opts:
                    o = dict(opts)
                    if "tsize" in o:
                        o["tsize"] = str(len(payload))
                    oack = b"\0\6" + b"".join(
                        k.encode() + b"\0" + v.encode() + b"\0" for k, v in o.items())
                    xfer.sendto(oack, client)
                    pkt, client = xfer.recvfrom(1024)  # expect ACK 0
                blk = 1
                for i in range(0, len(payload) + 1, blksize):
                    chunk = payload[i:i + blksize]
                    xfer.sendto(b"\0\3" + struct.pack("!H", blk & 0xFFFF) + chunk, client)
                    xfer.recvfrom(1024)
                    blk += 1
                    if len(chunk) < blksize:
                        break
                print("  sent", flush=True)
        except Exception as e:  # noqa: BLE001
            print(f"  xfer error: {e}", flush=True)
        finally:
            xfer.close()


if __name__ == "__main__":
    main()
