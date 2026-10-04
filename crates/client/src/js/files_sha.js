// wado bridge — incremental SHA-256, for the file manager's transfer checks.
//
// WebCrypto only hashes a whole buffer at once, and a transfer is a stream of chunks that may
// be gigabytes long, so this keeps the running state itself (FIPS 180-4). The daemon's half is
// `ring::digest`; scripts/files-e2e.mjs checks both agree on real transfers.
//
//   const h = W.sha256(); h.update(u8); …; h.hex()

(() => {
  const K = new Uint32Array([
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
  ]);

  W.sha256 = () => {
    const H = new Uint32Array([0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]);
    const w = new Uint32Array(64);
    const block = new Uint8Array(64);
    let fill = 0, total = 0;

    function compress(b, off) {
      for (let i = 0; i < 16; i++) {
        const j = off + i * 4;
        w[i] = (b[j] << 24) | (b[j + 1] << 16) | (b[j + 2] << 8) | b[j + 3];
      }
      for (let i = 16; i < 64; i++) {
        const x = w[i - 15], y = w[i - 2];
        const s0 = ((x >>> 7) | (x << 25)) ^ ((x >>> 18) | (x << 14)) ^ (x >>> 3);
        const s1 = ((y >>> 17) | (y << 15)) ^ ((y >>> 19) | (y << 13)) ^ (y >>> 10);
        w[i] = (w[i - 16] + s0 + w[i - 7] + s1) | 0;
      }
      let a = H[0], bb = H[1], c = H[2], d = H[3], e = H[4], f = H[5], g = H[6], h = H[7];
      for (let i = 0; i < 64; i++) {
        const S1 = ((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7));
        const t1 = (h + S1 + ((e & f) ^ (~e & g)) + K[i] + w[i]) | 0;
        const S0 = ((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10));
        const t2 = (S0 + ((a & bb) ^ (a & c) ^ (bb & c))) | 0;
        h = g; g = f; f = e; e = (d + t1) | 0; d = c; c = bb; bb = a; a = (t1 + t2) | 0;
      }
      H[0] += a; H[1] += bb; H[2] += c; H[3] += d; H[4] += e; H[5] += f; H[6] += g; H[7] += h;
    }

    return {
      update(u8) {
        let i = 0;
        total += u8.length;
        if (fill) {
          const n = Math.min(64 - fill, u8.length);
          block.set(u8.subarray(0, n), fill);
          fill += n; i = n;
          if (fill < 64) return;
          compress(block, 0); fill = 0;
        }
        for (; i + 64 <= u8.length; i += 64) compress(u8, i);
        if (i < u8.length) { block.set(u8.subarray(i), 0); fill = u8.length - i; }
      },
      hex() {
        const bits = total * 8;
        const pad = new Uint8Array((fill < 56 ? 56 : 120) - fill + 8);
        pad[0] = 0x80;
        const dv = new DataView(pad.buffer);
        dv.setUint32(pad.length - 8, Math.floor(bits / 2 ** 32));
        dv.setUint32(pad.length - 4, bits >>> 0);
        this.update(pad);
        return [...H].map((x) => (x >>> 0).toString(16).padStart(8, "0")).join("");
      },
    };
  };
})();
