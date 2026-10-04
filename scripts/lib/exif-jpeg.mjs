// A JPEG with a real EXIF block, for testing the viewer's details panel (js/files_exif.js):
// camera, date taken, exposure, aperture, ISO, focal length and a GPS position.
//
//   withExif(jpegBytes) → Buffer

export function withExif(jpeg) {
  const data = [];
  // Little-endian TIFF. Offsets are from the start of the TIFF header.
  const ifd = (entries, at) => {
    const out = Buffer.alloc(2 + entries.length * 12 + 4);
    out.writeUInt16LE(entries.length, 0);
    entries.forEach((e, i) => {
      const o = 2 + i * 12;
      out.writeUInt16LE(e[0], o); out.writeUInt16LE(e[1], o + 2); out.writeUInt32LE(e[2], o + 4);
      if (typeof e[3] === "number") out.writeUInt32LE(e[3], o + 8); else e[3].copy(out, o + 8);
    });
    return out;
  };
  const asc = (s) => Buffer.from(s + "\0");
  const rat = (pairs) => { const b = Buffer.alloc(pairs.length * 8); pairs.forEach(([n, d], i) => { b.writeUInt32LE(n, i * 8); b.writeUInt32LE(d, i * 8 + 4); }); return b; };
  // Layout: header(8) | IFD0 | Exif IFD | GPS IFD | data area
  const make = asc("Google"), model = asc("Pixel 8"), taken = asc("2024:05:01 13:22:10"), lens = asc("Pixel 8 back camera");
  const fnum = rat([[17, 10]]), expo = rat([[1, 250]]), focal = rat([[69, 10]]);
  const lat = rat([[48, 1], [51, 1], [2952, 100]]), lon = rat([[2, 1], [17, 1], [4000, 100]]);
  const ifd0Len = 2 + 3 * 12 + 4, exifLen = 2 + 5 * 12 + 4, gpsLen = 2 + 4 * 12 + 4;
  const ifd0At = 8, exifAt = ifd0At + ifd0Len, gpsAt = exifAt + exifLen;
  let d = gpsAt + gpsLen;
  const put = (b) => { const at = d; data.push(b); d += b.length; return at; };
  const ifd0 = ifd([[0x010f, 2, make.length, put(make)], [0x0110, 2, model.length, put(model)], [0x8769, 4, 1, exifAt]]);
  const exif = ifd([[0x829a, 5, 1, put(expo)], [0x829d, 5, 1, put(fnum)], [0x8827, 3, 1, 160], [0x9003, 2, taken.length, put(taken)], [0x920a, 5, 1, put(focal)]]);
  const gps = ifd([[0x0001, 2, 2, Buffer.from("N\0\0\0")], [0x0002, 5, 3, put(lat)], [0x0003, 2, 2, Buffer.from("E\0\0\0")], [0x0004, 5, 3, put(lon)]]);
  const head = Buffer.from([0x49, 0x49, 0x2a, 0x00, 8, 0, 0, 0]);
  // IFD0 points at the Exif and GPS directories, as real files do.
  const ifd0full = ifd([[0x010f, 2, make.length, ifd0.readUInt32LE(2 + 8)], [0x0110, 2, model.length, ifd0.readUInt32LE(2 + 12 + 8)], [0x8769, 4, 1, exifAt], [0x8825, 4, 1, gpsAt]]);
  // ifd0full has 4 entries: shift everything after it by 12 bytes.
  const shift = 12;
  const fix = (b, n) => { for (let i = 0; i < n; i++) { const o = 2 + i * 12; const type = b.readUInt16LE(o + 2), count = b.readUInt32LE(o + 4); const size = { 2: 1, 3: 2, 4: 4, 5: 8 }[type] * count; if (size > 4 || b.readUInt16LE(o) === 0x8769 || b.readUInt16LE(o) === 0x8825) b.writeUInt32LE(b.readUInt32LE(o + 8) + shift, o + 8); } return b; };
  const tiff = Buffer.concat([head, fix(ifd0full, 4), fix(exif, 5), fix(gps, 4), ...data]);
  const app1 = Buffer.concat([Buffer.from([0xff, 0xe1, 0, 0]), Buffer.from("Exif\0\0"), tiff]);
  app1.writeUInt16BE(app1.length - 2, 2);
  return Buffer.concat([jpeg.subarray(0, 2), app1, jpeg.subarray(2)]);
}
