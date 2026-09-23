import { deflateSync } from "node:zlib";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../public/icons");
await mkdir(root, { recursive: true });
const desktopIcons = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../desktop/src-tauri/icons");
await mkdir(desktopIcons, { recursive: true });

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const name = Buffer.from(type);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const checksum = Buffer.alloc(4);
  checksum.writeUInt32BE(crc32(Buffer.concat([name, data])));
  return Buffer.concat([length, name, data, checksum]);
}

function colorAt(x, y, size) {
  const nx = x / size;
  const ny = y / size;
  const edge = 0.17;
  const dx = Math.max(edge - nx, 0, nx - (1 - edge));
  const dy = Math.max(edge - ny, 0, ny - (1 - edge));
  const rounded = dx * dx + dy * dy > edge * edge;
  if (rounded) return [0, 0, 0, 0];

  const inBubble = nx > 0.23 && nx < 0.77 && ny > 0.23 && ny < 0.65;
  const inTail = ny >= 0.59 && ny < 0.77 && nx > 0.59 && nx < 0.77 && ny > 0.59 + (nx - 0.59) * 0.3;
  if (inBubble || inTail) {
    const dotY = 0.445;
    for (const dotX of [0.39, 0.5, 0.61]) {
      const px = nx - dotX;
      const py = ny - dotY;
      if (px * px + py * py < 0.00125) return [70, 82, 125, 255];
    }
    return [255, 255, 255, 255];
  }
  const blend = Math.max(0, Math.min(1, ny));
  return [Math.round(49 - 8 * blend), Math.round(63 - 13 * blend), Math.round(113 - 4 * blend), 255];
}

function png(size) {
  const raw = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y += 1) {
    const row = y * (size * 4 + 1);
    raw[row] = 0;
    for (let x = 0; x < size; x += 1) {
      const color = colorAt(x, y, size);
      const offset = row + 1 + x * 4;
      raw.set(color, offset);
    }
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header[8] = 8;
  header[9] = 6;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const sizes = [32, 48, 128, 192, 256, 512];
await Promise.all(sizes.map((size) => writeFile(path.join(root, `icon-${size}.png`), png(size))));
await writeFile(path.join(desktopIcons, "icon.png"), png(512));

const icoSizes = [16, 32, 48, 256];
const images = icoSizes.map((size) => png(size));
const header = Buffer.alloc(6 + icoSizes.length * 16);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(icoSizes.length, 4);
let offset = header.length;
for (let index = 0; index < icoSizes.length; index += 1) {
  const entry = 6 + index * 16;
  const size = icoSizes[index] === 256 ? 0 : icoSizes[index];
  header[entry] = size;
  header[entry + 1] = size;
  header[entry + 2] = 0;
  header[entry + 3] = 0;
  header.writeUInt16LE(1, entry + 4);
  header.writeUInt16LE(32, entry + 6);
  header.writeUInt32LE(images[index].length, entry + 8);
  header.writeUInt32LE(offset, entry + 12);
  offset += images[index].length;
}
await writeFile(path.join(desktopIcons, "icon.ico"), Buffer.concat([header, ...images]));
