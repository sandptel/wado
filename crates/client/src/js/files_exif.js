// wado bridge — what a camera wrote into a photo (EXIF): when it was taken, with what, how, and
// where. For the viewer's details panel. JPEG only (where cameras and phones put it).
//
//   const x = await W.files.exif(blob)   → { taken, camera, lens, exposure, fnumber, iso, focal,
//                                            width, height, lat, lon } | null
//
// Reads the first 256 KiB of the file and walks the TIFF structure inside the APP1 segment.
// Every read is bounds-checked: a photo is data from the computer, and a malformed one must give
// "no details", never an exception.

(() => {
  const F = W.files;
  F.exif = async (blob) => {
    try {
      const buf = new DataView(await blob.slice(0, 256 << 10).arrayBuffer());
      if (buf.byteLength < 4 || buf.getUint16(0) !== 0xffd8) return null;
      let off = 2;
      while (off + 4 <= buf.byteLength) {
        const marker = buf.getUint16(off), len = buf.getUint16(off + 2);
        if (marker === 0xffe1 && off + 10 <= buf.byteLength && buf.getUint32(off + 4) === 0x45786966) return tiff(buf, off + 10);
        if ((marker & 0xff00) !== 0xff00 || marker === 0xffda) return null;
        off += 2 + len;
      }
    } catch (_) {}
    return null;
  };

  function tiff(v, t) {
    const le = v.getUint16(t) === 0x4949;
    const u16 = (o) => v.getUint16(t + o, le), u32 = (o) => v.getUint32(t + o, le);
    const ok = (o, n) => t + o >= 0 && t + o + n <= v.byteLength;
    const tags = {};
    const read = (ifd, depth) => {
      if (depth > 3 || !ok(ifd, 2)) return;
      const n = u16(ifd);
      for (let i = 0; i < n && i < 400; i++) {
        const e = ifd + 2 + i * 12;
        if (!ok(e, 12)) return;
        const tag = u16(e), type = u16(e + 2), count = u32(e + 4);
        const size = { 1: 1, 2: 1, 3: 2, 4: 4, 5: 8, 7: 1, 9: 4, 10: 8 }[type] || 1;
        const at = size * count > 4 ? u32(e + 8) : e + 8;
        if (!ok(at, Math.min(size * count, 64))) continue;
        let val;
        if (type === 2) { let s = ""; for (let k = 0; k < Math.min(count, 64); k++) { const c = v.getUint8(t + at + k); if (!c) break; s += String.fromCharCode(c); } val = s.trim(); }
        else if (type === 3) val = u16(at);
        else if (type === 4) val = u32(at);
        else if (type === 5 || type === 10) {
          const r = (k) => { const d = u32(at + k * 8 + 4); return d ? u32(at + k * 8) / d : 0; };
          val = count === 1 ? r(0) : Array.from({ length: Math.min(count, 4) }, (_, k) => r(k));
        }
        tags[tag] = val;
        if (tag === 0x8769 || tag === 0x8825) read(val, depth + 1); // Exif and GPS sub-directories
      }
    };
    if (!ok(4, 4)) return null;
    read(u32(4), 0);
    const dms = (a, ref) => (Array.isArray(a) ? (a[0] + a[1] / 60 + a[2] / 3600) * (ref === "S" || ref === "W" ? -1 : 1) : null);
    const x = {
      taken: tags[0x9003] || tags[0x0132] || "",
      camera: [tags[0x010f], tags[0x0110]].filter(Boolean).join(" ").replace(/^(\w+) \1 /i, "$1 "),
      lens: tags[0xa434] || "",
      exposure: tags[0x829a] ? (tags[0x829a] >= 1 ? `${tags[0x829a]} s` : `1/${Math.round(1 / tags[0x829a])} s`) : "",
      fnumber: tags[0x829d] ? `ƒ/${tags[0x829d].toFixed(1)}` : "",
      iso: tags[0x8827] ? `ISO ${tags[0x8827]}` : "",
      focal: tags[0x920a] ? `${Math.round(tags[0x920a])} mm` : "",
      lat: dms(tags[0x0002], tags[0x0001]),
      lon: dms(tags[0x0004], tags[0x0003]),
    };
    // "2024:05:01 13:22:10" → a Date
    const m = /^(\d{4}):(\d\d):(\d\d) (\d\d):(\d\d)/.exec(x.taken);
    x.taken = m ? new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5]) : null;
    return Object.values(x).some((y) => y) ? x : null;
  }
})();
