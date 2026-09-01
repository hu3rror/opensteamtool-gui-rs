"""Generate a placeholder app.ico (256x256 PNG-in-ICO) with pure stdlib."""
import struct, zlib

SIZE = 256
NAVY = (27, 40, 56, 255)       # Steam dark blue
TEAL = (102, 192, 244, 255)    # Steam light blue
WHITE = (255, 255, 255, 255)
TRANSPARENT = (0, 0, 0, 0)

def rounded_rect_mask(x, y, r):
    """True if (x,y) is inside a SIZE x SIZE rounded rect with corner radius r."""
    x0, y0, x1, y1 = r, r, SIZE - 1 - r, SIZE - 1 - r
    if x0 <= x <= x1 or y0 <= y <= y1:
        return True
    cx = x0 if x < x0 else (x1 if x > x1 else x)
    cy = y0 if y < y0 else (y1 if y > y1 else y)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r

def inside_circle(x, y, cx, cy, rad):
    return (x - cx) ** 2 + (y - cy) ** 2 <= rad * rad

def inside_triangle(x, y, pts):
    def sign(a, b, c):
        return (a[0] - c[0]) * (b[1] - c[1]) - (b[0] - c[0]) * (a[1] - c[1])
    d1 = sign((x, y), pts[0], pts[1])
    d2 = sign((x, y), pts[1], pts[2])
    d3 = sign((x, y), pts[2], pts[0])
    neg = d1 < 0 or d2 < 0 or d3 < 0
    pos = d1 > 0 or d2 > 0 or d3 > 0
    return not (neg and pos)

# --- draw ---
rows = bytearray()
for y in range(SIZE):
    rows.append(0)  # PNG filter: None
    for x in range(SIZE):
        px = TRANSPARENT
        if rounded_rect_mask(x, y, 24):
            px = NAVY
        # outer teal ring
        if inside_circle(x, y, SIZE // 2, SIZE // 2, 92):
            if not inside_circle(x, y, SIZE // 2, SIZE // 2, 74):
                px = TEAL
        # inner white play triangle
        if inside_circle(x, y, SIZE // 2, SIZE // 2, 74):
            pts = [(108, 108), (108, 148), (156, 128)]
            if inside_triangle(x, y, pts):
                px = WHITE
        rows += bytes(px)

# --- PNG chunking ---
def chunk(typ, data):
    return (struct.pack(">I", len(data)) + typ + data
            + struct.pack(">I", zlib.crc32(typ + data) & 0xFFFFFFFF))

def png():
    ihdr = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    idat = zlib.compress(bytes(rows), 9)
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", idat)
            + chunk(b"IEND", b""))

png_data = png()

# --- ICO container (single 256x256 PNG entry) ---
header = struct.pack("<HHH", 0, 1, 1)
entry = struct.pack("<BBBBHHII", 0, 0, 0, 0, 1, 32, len(png_data), 22)
with open("app.ico", "wb") as f:
    f.write(header + entry + png_data)

print(f"app.ico written: {len(png_data) + 22} bytes")
