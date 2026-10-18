/*
 * wappfix: LD_PRELOAD shim that lets the firmware's MediaTek `wapp` understand
 * its Wi-Fi driver's WAPP events.
 *
 * The driver sends each WAPP event (wireless IWEVCUSTOM, flags 0x0647) as a
 * packed struct: len(1) event_id(1) ifindex(4) data... — data at offset 6.
 * This `wapp` build reads them with ifindex aligned to 4: ifindex at offset 4,
 * data at offset 8 (verified in its dispatcher: `ldr r1,[ev,#4]`, `add r2,ev,#8`).
 * Every event is therefore misparsed (dev_type, MAC and ifname shifted by two
 * bytes), so wapp registers no radio and EasyMesh onboarding never starts.
 *
 * This shim rewrites those events as wapp receives them from netlink: two zero
 * bytes are inserted after event_id and every enclosing length is adjusted.
 * Requests in the other direction already match (wapp_req is packed in both),
 * so nothing else is touched.
 *
 * Build: zig cc -target arm-linux-gnueabi.2.26 -mcpu=arm1176jzf_s -shared -fPIC -Os
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdint.h>
#include <string.h>
#include <sys/socket.h>
#include <linux/netlink.h>
#include <linux/rtnetlink.h>
#include <linux/wireless.h>

#define WAPP_EVENT_FLAGS 0x0647
#define PAD 2
#define MAX_BUF 16384

static ssize_t (*real_recvfrom)(int, void *, size_t, int, struct sockaddr *, socklen_t *);

static int is_netlink(int fd) {
    struct sockaddr_storage ss;
    socklen_t l = sizeof ss;
    return getsockname(fd, (struct sockaddr *)&ss, &l) == 0 && ss.ss_family == AF_NETLINK;
}

static uint16_t rd16(const unsigned char *p) { uint16_t v; memcpy(&v, p, 2); return v; }
static void wr16(unsigned char *p, uint16_t v) { memcpy(p, &v, 2); }

/* Rewrite one IFLA_WIRELESS stream; returns the new length or -1 if it does not fit. */
static int fix_stream(const unsigned char *in, int n, unsigned char *out, int cap) {
    int i = 0, o = 0;
    while (i + 4 <= n) {
        int len = rd16(in + i);
        uint16_t cmd = rd16(in + i + 2);
        if (len < 4 || i + len > n) break;
        int fix = cmd == IWEVCUSTOM && len >= 8 && rd16(in + i + 6) == WAPP_EVENT_FLAGS && rd16(in + i + 4) >= 2;
        int newlen = len + (fix ? PAD : 0);
        if (o + newlen > cap) return -1;
        if (!fix) {
            memcpy(out + o, in + i, len);
        } else {
            const unsigned char *d = in + i + 8;
            int dlen = rd16(in + i + 4);
            unsigned char *q = out + o;
            wr16(q, (uint16_t)newlen);
            wr16(q + 2, cmd);
            wr16(q + 4, (uint16_t)(dlen + PAD));
            wr16(q + 6, WAPP_EVENT_FLAGS);
            q[8] = d[0];                      /* len */
            q[9] = d[1];                      /* event_id */
            q[10] = q[11] = 0;                /* the padding wapp expects */
            memcpy(q + 12, d + 2, dlen - 2);  /* ifindex, data */
            /* copy anything after the declared data inside this event */
            if (len > 8 + dlen) memcpy(q + 12 + dlen - 2, in + i + 8 + dlen, len - 8 - dlen);
        }
        i += len;
        o += newlen;
    }
    if (i < n) {                              /* trailing bytes: keep as-is */
        if (o + (n - i) > cap) return -1;
        memcpy(out + o, in + i, n - i);
        o += n - i;
    }
    return o;
}

/* Rewrite a netlink datagram; returns the new length or -1 to leave it unchanged. */
static int fix_netlink(const unsigned char *in, int n, unsigned char *out, int cap) {
    int i = 0, o = 0, changed = 0;
    while (i + (int)sizeof(struct nlmsghdr) <= n) {
        const struct nlmsghdr *h = (const void *)(in + i);
        int mlen = h->nlmsg_len;
        if (mlen < (int)sizeof *h || i + mlen > n) break;
        int start = o;
        if (h->nlmsg_type != RTM_NEWLINK || mlen < (int)NLMSG_LENGTH(sizeof(struct ifinfomsg))) {
            if (o + NLMSG_ALIGN(mlen) > cap) return -1;
            memcpy(out + o, in + i, mlen);
            o += NLMSG_ALIGN(mlen);
        } else {
            int hdr = NLMSG_LENGTH(sizeof(struct ifinfomsg));
            if (o + hdr > cap) return -1;
            memcpy(out + o, in + i, hdr);
            o += NLMSG_ALIGN(hdr);
            int a = i + NLMSG_ALIGN(hdr);
            while (a + (int)sizeof(struct rtattr) <= i + mlen) {
                const struct rtattr *ra = (const void *)(in + a);
                int alen = ra->rta_len;
                if (alen < (int)sizeof *ra || a + alen > i + mlen) break;
                if (o + (int)sizeof *ra > cap) return -1;
                struct rtattr *rb = (void *)(out + o);
                rb->rta_type = ra->rta_type;
                int body = alen - (int)sizeof *ra, nb;
                if (ra->rta_type == IFLA_WIRELESS) {
                    nb = fix_stream(in + a + sizeof *ra, body, out + o + sizeof *ra, cap - o - (int)sizeof *ra);
                    if (nb < 0) return -1;
                    if (nb != body) changed = 1;
                } else {
                    if (o + alen > cap) return -1;
                    memcpy(out + o + sizeof *ra, in + a + sizeof *ra, body);
                    nb = body;
                }
                rb->rta_len = (unsigned short)(sizeof *ra + nb);
                int pad = RTA_ALIGN(rb->rta_len) - rb->rta_len;
                if (o + RTA_ALIGN(rb->rta_len) > cap) return -1;
                memset(out + o + rb->rta_len, 0, pad);
                o += RTA_ALIGN(rb->rta_len);
                a += RTA_ALIGN(alen);
            }
            ((struct nlmsghdr *)(out + start))->nlmsg_len = o - start;
        }
        i += NLMSG_ALIGN(mlen);
    }
    return changed ? o : -1;
}

ssize_t recvfrom(int fd, void *buf, size_t len, int flags, struct sockaddr *src, socklen_t *slen) {
    if (!real_recvfrom) real_recvfrom = dlsym(RTLD_NEXT, "recvfrom");
    ssize_t r = real_recvfrom(fd, buf, len, flags, src, slen);
    if (r <= 0 || (flags & MSG_PEEK) || !is_netlink(fd)) return r;
    static unsigned char tmp[MAX_BUF];
    int cap = len < MAX_BUF ? (int)len : MAX_BUF;
    int e = errno;
    int n = fix_netlink(buf, (int)r, tmp, cap);
    if (n > 0) {
        memcpy(buf, tmp, n);
        r = n;
    }
    errno = e;
    return r;
}
