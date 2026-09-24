// md5.hpp
#pragma once
#include <cstring>
#include <string>
#include <sstream>
#include <iomanip>

// 公共领域的 MD5 实现
namespace md5_impl {

typedef unsigned int uint32;

struct Context {
    uint32 buf[4];
    uint32 bits[2];
    unsigned char in[64];
};

inline void byteReverse(unsigned char* buf, unsigned longs) {
    uint32 t;
    do {
        t = (uint32)((unsigned)buf[3] << 8 | buf[2]) << 16 | ((unsigned)buf[1] << 8 | buf[0]);
        *(uint32*)buf = t;
        buf += 4;
    } while (--longs);
}

#define F1(x, y, z) (z ^ (x & (y ^ z)))
#define F2(x, y, z) F1(z, x, y)
#define F3(x, y, z) (x ^ y ^ z)
#define F4(x, y, z) (y ^ (x | ~z))

#define MD5STEP(f, w, x, y, z, data, s) (w += f(x, y, z) + data, w = w << s | w >> (32 - s), w += x)

inline void transform(uint32 buf[4], const unsigned char in[64]) {
    uint32 a = buf[0], b = buf[1], c = buf[2], d = buf[3];
    uint32 in32[16];

    for (int i = 0; i < 16; i++) {
        in32[i] = (uint32)in[i * 4] | ((uint32)in[i * 4 + 1] << 8) |
                  ((uint32)in[i * 4 + 2] << 16) | ((uint32)in[i * 4 + 3] << 24);
    }

    MD5STEP(F1, a, b, c, d, in32[0]  + 0xd76aa478, 7);
    MD5STEP(F1, d, a, b, c, in32[1]  + 0xe8c7b756, 12);
    MD5STEP(F1, c, d, a, b, in32[2]  + 0x242070db, 17);
    MD5STEP(F1, b, c, d, a, in32[3]  + 0xc1bdceee, 22);
    MD5STEP(F1, a, b, c, d, in32[4]  + 0xf57c0faf, 7);
    MD5STEP(F1, d, a, b, c, in32[5]  + 0x4787c62a, 12);
    MD5STEP(F1, c, d, a, b, in32[6]  + 0xa8304613, 17);
    MD5STEP(F1, b, c, d, a, in32[7]  + 0xfd469501, 22);
    MD5STEP(F1, a, b, c, d, in32[8]  + 0x698098d8, 7);
    MD5STEP(F1, d, a, b, c, in32[9]  + 0x8b44f7af, 12);
    MD5STEP(F1, c, d, a, b, in32[10] + 0xffff5bb1, 17);
    MD5STEP(F1, b, c, d, a, in32[11] + 0x895cd7be, 22);
    MD5STEP(F1, a, b, c, d, in32[12] + 0x6b901122, 7);
    MD5STEP(F1, d, a, b, c, in32[13] + 0xfd987193, 12);
    MD5STEP(F1, c, d, a, b, in32[14] + 0xa679438e, 17);
    MD5STEP(F1, b, c, d, a, in32[15] + 0x49b40821, 22);

    MD5STEP(F2, a, b, c, d, in32[1]  + 0xf61e2562, 5);
    MD5STEP(F2, d, a, b, c, in32[6]  + 0xc040b340, 9);
    MD5STEP(F2, c, d, a, b, in32[11] + 0x265e5a51, 14);
    MD5STEP(F2, b, c, d, a, in32[0]  + 0xe9b6c7aa, 20);
    MD5STEP(F2, a, b, c, d, in32[5]  + 0xd62f105d, 5);
    MD5STEP(F2, d, a, b, c, in32[10] + 0x02441453, 9);
    MD5STEP(F2, c, d, a, b, in32[15] + 0xd8a1e681, 14);
    MD5STEP(F2, b, c, d, a, in32[4]  + 0xe7d3fbc8, 20);
    MD5STEP(F2, a, b, c, d, in32[9]  + 0x21e1cde6, 5);
    MD5STEP(F2, d, a, b, c, in32[14] + 0xc33707d6, 9);
    MD5STEP(F2, c, d, a, b, in32[3]  + 0xf4d50d87, 14);
    MD5STEP(F2, b, c, d, a, in32[8]  + 0x455a14ed, 20);
    MD5STEP(F2, a, b, c, d, in32[13] + 0xa9e3e905, 5);
    MD5STEP(F2, d, a, b, c, in32[2]  + 0xfcefa3f8, 9);
    MD5STEP(F2, c, d, a, b, in32[7]  + 0x676f02d9, 14);
    MD5STEP(F2, b, c, d, a, in32[12] + 0x8d2a4c8a, 20);

    MD5STEP(F3, a, b, c, d, in32[5]  + 0xfffa3942, 4);
    MD5STEP(F3, d, a, b, c, in32[8]  + 0x8771f681, 11);
    MD5STEP(F3, c, d, a, b, in32[11] + 0x6d9d6122, 16);
    MD5STEP(F3, b, c, d, a, in32[14] + 0xfde5380c, 23);
    MD5STEP(F3, a, b, c, d, in32[1]  + 0xa4beea44, 4);
    MD5STEP(F3, d, a, b, c, in32[4]  + 0x4bdecfa9, 11);
    MD5STEP(F3, c, d, a, b, in32[7]  + 0xf6bb4b60, 16);
    MD5STEP(F3, b, c, d, a, in32[10] + 0xbebfbc70, 23);
    MD5STEP(F3, a, b, c, d, in32[13] + 0x289b7ec6, 4);
    MD5STEP(F3, d, a, b, c, in32[0]  + 0xeaa127fa, 11);
    MD5STEP(F3, c, d, a, b, in32[3]  + 0xd4ef3085, 16);
    MD5STEP(F3, b, c, d, a, in32[6]  + 0x04881d05, 23);
    MD5STEP(F3, a, b, c, d, in32[9]  + 0xd9d4d039, 4);
    MD5STEP(F3, d, a, b, c, in32[12] + 0xe6db99e5, 11);
    MD5STEP(F3, c, d, a, b, in32[15] + 0x1fa27cf8, 16);
    MD5STEP(F3, b, c, d, a, in32[2]  + 0xc4ac5665, 23);

    MD5STEP(F4, a, b, c, d, in32[0]  + 0xf4292244, 6);
    MD5STEP(F4, d, a, b, c, in32[7]  + 0x432aff97, 10);
    MD5STEP(F4, c, d, a, b, in32[14] + 0xab9423a7, 15);
    MD5STEP(F4, b, c, d, a, in32[5]  + 0xfc93a039, 21);
    MD5STEP(F4, a, b, c, d, in32[12] + 0x655b59c3, 6);
    MD5STEP(F4, d, a, b, c, in32[3]  + 0x8f0ccc92, 10);
    MD5STEP(F4, c, d, a, b, in32[10] + 0xffeff47d, 15);
    MD5STEP(F4, b, c, d, a, in32[1]  + 0x85845dd1, 21);
    MD5STEP(F4, a, b, c, d, in32[8]  + 0x6fa87e4f, 6);
    MD5STEP(F4, d, a, b, c, in32[15] + 0xfe2ce6e0, 10);
    MD5STEP(F4, c, d, a, b, in32[6]  + 0xa3014314, 15);
    MD5STEP(F4, b, c, d, a, in32[13] + 0x4e0811a1, 21);
    MD5STEP(F4, a, b, c, d, in32[4]  + 0xf7537e82, 6);
    MD5STEP(F4, d, a, b, c, in32[11] + 0xbd3af235, 10);
    MD5STEP(F4, c, d, a, b, in32[2]  + 0x2ad7d2bb, 15);
    MD5STEP(F4, b, c, d, a, in32[9]  + 0xeb86d391, 21);

    buf[0] += a;
    buf[1] += b;
    buf[2] += c;
    buf[3] += d;
}

inline void init(Context* ctx) {
    ctx->buf[0] = 0x67452301;
    ctx->buf[1] = 0xefcdab89;
    ctx->buf[2] = 0x98badcfe;
    ctx->buf[3] = 0x10325476;
    ctx->bits[0] = 0;
    ctx->bits[1] = 0;
}

inline void update(Context* ctx, const unsigned char* buf, unsigned len) {
    uint32 t = ctx->bits[0];
    if ((ctx->bits[0] = t + ((uint32)len << 3)) < t)
        ctx->bits[1]++;
    ctx->bits[1] += len >> 29;

    t = (t >> 3) & 0x3f;

    if (t) {
        unsigned char* p = (unsigned char*)ctx->in + t;
        t = 64 - t;
        if (len < t) {
            memcpy(p, buf, len);
            return;
        }
        memcpy(p, buf, t);
        byteReverse(ctx->in, 16);
        transform(ctx->buf, ctx->in);
        buf += t;
        len -= t;
    }

    while (len >= 64) {
        memcpy(ctx->in, buf, 64);
        byteReverse(ctx->in, 16);
        transform(ctx->buf, ctx->in);
        buf += 64;
        len -= 64;
    }

    memcpy(ctx->in, buf, len);
}

inline void final(unsigned char digest[16], Context* ctx) {
    unsigned count = (ctx->bits[0] >> 3) & 0x3F;
    unsigned char* p = ctx->in + count;
    *p++ = 0x80;
    count = 64 - 1 - count;
    if (count < 8) {
        memset(p, 0, count);
        byteReverse(ctx->in, 16);
        transform(ctx->buf, ctx->in);
        memset(ctx->in, 0, 56);
    } else {
        memset(p, 0, count - 8);
    }
    byteReverse(ctx->in, 14);
    ((uint32*)ctx->in)[14] = ctx->bits[0];
    ((uint32*)ctx->in)[15] = ctx->bits[1];
    transform(ctx->buf, ctx->in);
    byteReverse((unsigned char*)ctx->buf, 4);
    memcpy(digest, ctx->buf, 16);
    memset(ctx, 0, sizeof(*ctx));
}

} // namespace md5_impl

inline std::string md5(const std::string& input) {
    md5_impl::Context ctx;
    md5_impl::init(&ctx);
    md5_impl::update(&ctx, (const unsigned char*)input.c_str(), input.size());

    unsigned char digest[16];
    md5_impl::final(digest, &ctx);

    std::ostringstream oss;
    for (int i = 0; i < 16; i++) {
        oss << std::hex << std::setw(2) << std::setfill('0') << (int)digest[i];
    }
    return oss.str();
}