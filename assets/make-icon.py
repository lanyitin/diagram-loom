#!/usr/bin/env python3
"""產生 macOS 規格的 app icon：`mise run icon`。

# 為什麼要自己寫光柵化

macOS 的 icon 需要**透明的留白**。手邊唯一能把 SVG 轉點陣的工具是
`qlmanage`，它會把圖壓在白底上——透明度全沒了，做出來就是一塊白方磚。
沒有 PIL、numpy、cairosvg，所以自己算。形狀夠簡單，值得。

# 規格是量出來的，不是查來的

網路上關於 macOS icon 的說法很多且互相矛盾。這裡的數字全部量自
系統內建的 Notes.app 與 Calculator.app（兩者完全一致）：

    畫布 1024×1024
    本體 824×824，四周留白 100
    底下多 12px 是陰影

圓角是**超橢圓**（Big Sur 之後的連續曲率圓角），把 Apple 的角輪廓
逐列量出來後擬合，得到 R=252、N=2.9（RMS 誤差 0.012）。
常見說法的 R=0.225×寬 明顯太方。

# 圖案

織布機：本體就是框，淺色直線是經線，珊瑚色那條是緯線穿過三個節點。
刻意做成實心——線稿在 32px 下會糊成一團。
"""

import math
import pathlib
import struct
import sys
import zlib

CANVAS = 1024
BODY = 824
MARGIN = 100
R = 252.0
N = 2.9

SHADOW_DY = 10
SHADOW_BLUR = 14
SHADOW_ALPHA = 0.26

TOP = (0x27, 0xB4, 0x89)
BOTTOM = (0x12, 0x7A, 0x58)
WARP = (0xFF, 0xFF, 0xFF, 0.34)
WEFT = (0xF2, 0x6B, 0x3A)
NODE = (0xFF, 0xF2, 0xEA)


def 夾(v, lo=0.0, hi=1.0):
    return lo if v < lo else hi if v > hi else v


def squircle_path(x0, y0, size, steps=26):
    """同一個形狀的 SVG 版本，給 logo 用。

    參數與上面共用，所以 icon 跟 logo 的圓角不可能長得不一樣。
    """
    r = size * (R / BODY)
    x1, y1 = x0 + size, y0 + size

    def corner(cx, cy, sx, sy):
        pts = []
        for i in range(steps + 1):
            t = math.pi / 2 * i / steps
            # math.cos(pi/2) 在這台機器回傳 -1.6e-16。負數開分數次方會變複數，
            # 產出的路徑就會出現 "363.30+0.00j" 這種東西。夾一下。
            c = max(0.0, math.cos(t)) ** (2 / N)
            s = max(0.0, math.sin(t)) ** (2 / N)
            pts.append((cx + sx * r * c, cy + sy * r * s))
        return pts

    pts = corner(x1 - r, y0 + r, 1, -1)[::-1]
    pts += corner(x1 - r, y1 - r, 1, 1)
    pts += corner(x0 + r, y1 - r, -1, 1)[::-1]
    pts += corner(x0 + r, y0 + r, -1, -1)
    return 'M' + 'L'.join(f'{x:.2f},{y:.2f}' for x, y in pts) + 'Z'


def 寫logo(path):
    """實心版的 logo。線稿在小尺寸會糊成一團，所以主要版本改成實心。"""
    mark = squircle_path(280, 15, 120)
    warp = ''.join(
        f'<line x1="{280 + x}" y1="43" x2="{280 + x}" y2="107" stroke="#fff" '
        f'stroke-opacity=".34" stroke-width="7" stroke-linecap="round"/>'
        for x in (25, 60, 95))
    weft = ('<path d="M305,40 C325,40 325,75 340,75 S360,110 375,110" fill="none" '
            'stroke="#F26B3A" stroke-width="9" stroke-linecap="round" stroke-linejoin="round"/>')
    nodes = ''.join(
        f'<rect x="{280 + x - 9}" y="{15 + y - 9}" width="18" height="18" rx="5.5" fill="#FFF2EA"/>'
        for x, y in ((25, 25), (60, 60), (95, 95)))

    pathlib.Path(path).write_text(f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 680 230" width="680" height="230">
  <!-- 由 assets/make-icon.py 產生。圓角與 app icon 共用同一組超橢圓參數。 -->
  <defs>
    <linearGradient id="loom-body" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#27B489"/>
      <stop offset="1" stop-color="#127A58"/>
    </linearGradient>
  </defs>
  <path d="{mark}" fill="url(#loom-body)"/>
  {warp}
  {weft}
  {nodes}
  <text x="340" y="175" text-anchor="middle" font-family="sans-serif" font-size="36" font-weight="600" fill="#14201c">DiagramLoom</text>
  <text x="340" y="205" text-anchor="middle" font-family="sans-serif" font-size="13" fill="#6B7B76">Weave data into C4 model diagrams</text>
</svg>
''')
    print(f'寫出 {path}')


def 覆蓋(d):
    """把「距離邊界幾個像素」換成 0..1 的覆蓋率。負值代表在裡面。"""
    return 夾(0.5 - d)


def 本體距離(x, y):
    """到超橢圓圓角矩形邊界的距離（近似，夠做 1px 抗鋸齒）。"""
    x0, y0 = MARGIN, MARGIN
    x1, y1 = MARGIN + BODY, MARGIN + BODY

    dx = max(x0 + R - x, x - (x1 - R), 0.0)
    dy = max(y0 + R - y, y - (y1 - R), 0.0)

    if dx == 0.0 and dy == 0.0:
        # 直邊區：距離就是離最近那條邊多遠。
        return -min(x - x0, x1 - x, y - y0, y1 - y)

    f = (dx / R) ** N + (dy / R) ** N
    # f=1 正好在邊界上。把它換算回像素距離。
    return (f ** (1.0 / N) - 1.0) * R


def 線段距離(px, py, ax, ay, bx, by):
    vx, vy = bx - ax, by - ay
    wx, wy = px - ax, py - ay
    L = vx * vx + vy * vy
    t = 0.0 if L == 0 else 夾((wx * vx + wy * vy) / L)
    return math.hypot(px - (ax + t * vx), py - (ay + t * vy))


def 折線距離(px, py, pts):
    best = 1e9
    for i in range(len(pts) - 1):
        d = 線段距離(px, py, pts[i][0], pts[i][1], pts[i + 1][0], pts[i + 1][1])
        if d < best:
            best = d
    return best


def 貝茲(p0, p1, p2, p3, n=48):
    out = []
    for i in range(n + 1):
        t = i / n
        u = 1 - t
        x = u**3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t**3 * p3[0]
        y = u**3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t**3 * p3[1]
        out.append((x, y))
    return out


def 圓角方距離(px, py, x, y, w, h, r):
    cx = abs(px - (x + w / 2)) - (w / 2 - r)
    cy = abs(py - (y + h / 2)) - (h / 2 - r)
    if cx <= 0 or cy <= 0:
        return max(cx, cy) - r
    return math.hypot(cx, cy) - r


def 模糊(a, w, h, r, 次=3):
    """三次盒狀模糊，效果接近高斯。"""
    for _ in range(次):
        out = [0.0] * (w * h)
        for y in range(h):
            base = y * w
            acc = 0.0
            for x in range(-r, w + r):
                if x + r < w:
                    acc += a[base + x + r]
                if x - r - 1 >= 0:
                    acc -= a[base + x - r - 1]
                if 0 <= x < w:
                    out[base + x] = acc / (2 * r + 1)
        a = out
        out = [0.0] * (w * h)
        for x in range(w):
            acc = 0.0
            for y in range(-r, h + r):
                if y + r < h:
                    acc += a[(y + r) * w + x]
                if y - r - 1 >= 0:
                    acc -= a[(y - r - 1) * w + x]
                if 0 <= y < h:
                    out[y * w + x] = acc / (2 * r + 1)
        a = out
    return a


def 疊(dst, i, r, g, b, alpha):
    if alpha <= 0:
        return
    da = dst[i * 4 + 3] / 255
    na = alpha + da * (1 - alpha)
    if na <= 0:
        return
    for k, c in enumerate((r, g, b)):
        dc = dst[i * 4 + k] / 255
        dst[i * 4 + k] = int(round(((c / 255) * alpha + dc * da * (1 - alpha)) / na * 255))
    dst[i * 4 + 3] = int(round(na * 255))


def 寫png(path, w, h, rgba):
    def chunk(t, d):
        c = t + d
        return struct.pack('>I', len(d)) + c + struct.pack('>I', zlib.crc32(c))

    raw = bytearray()
    for y in range(h):
        raw.append(0)
        raw += rgba[y * w * 4:(y + 1) * w * 4]
    png = (b'\x89PNG\r\n\x1a\n'
           + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0))
           + chunk(b'IDAT', zlib.compress(bytes(raw), 9))
           + chunk(b'IEND', b''))
    pathlib.Path(path).write_bytes(png)


def main(out):
    # ── 陰影：低解析度算完再放大，反正它本來就是糊的。 ──────────
    S = 128
    k = CANVAS / S
    小 = [覆蓋(本體距離((x + 0.5) * k, (y + 0.5) * k) / k) for y in range(S) for x in range(S)]
    小 = 模糊(小, S, S, max(1, round(SHADOW_BLUR / k)))

    px = bytearray(CANVAS * CANVAS * 4)

    for y in range(CANVAS):
        sy = (y - SHADOW_DY) / k - 0.5
        y0 = int(math.floor(sy))
        fy = sy - y0
        for x in range(CANVAS):
            sx = x / k - 0.5
            x0 = int(math.floor(sx))
            fx = sx - x0
            v = 0.0
            for dy in (0, 1):
                yy = y0 + dy
                if not 0 <= yy < S:
                    continue
                wy = fy if dy else 1 - fy
                for dx in (0, 1):
                    xx = x0 + dx
                    if not 0 <= xx < S:
                        continue
                    v += 小[yy * S + xx] * wy * (fx if dx else 1 - fx)
            if v > 0.002:
                疊(px, y * CANVAS + x, 0, 0, 0, v * SHADOW_ALPHA)

    # ── 本體 ───────────────────────────────────────────────
    for y in range(CANVAS):
        t = 夾((y - MARGIN) / BODY)
        r = round(TOP[0] + (BOTTOM[0] - TOP[0]) * t)
        g = round(TOP[1] + (BOTTOM[1] - TOP[1]) * t)
        b = round(TOP[2] + (BOTTOM[2] - TOP[2]) * t)
        for x in range(CANVAS):
            a = 覆蓋(本體距離(x + 0.5, y + 0.5))
            if a > 0:
                疊(px, y * CANVAS + x, r, g, b, a)

    # ── 圖案。座標沿用原 logo 的 120×120 標記空間。 ────────────
    s = BODY / 120 * 0.74
    ox = MARGIN + (BODY - 120 * s) / 2
    oy = MARGIN + (BODY - 120 * s) / 2
    P = lambda mx, my: (ox + mx * s, oy + my * s)

    經線 = [(P(mx, 13), P(mx, 107)) for mx in (25, 60, 95)]
    經寬 = 7 * s / 2

    緯 = 貝茲(P(25, 25), P(45, 25), P(45, 60), P(60, 60)) + 貝茲(
        P(60, 60), P(75, 60), P(75, 95), P(95, 95))
    緯寬 = 9 * s / 2

    節點 = [(P(mx - 9, my - 9), 18 * s, 5.5 * s) for mx, my in ((25, 25), (60, 60), (95, 95))]

    # 只掃圖案可能出現的範圍，其他像素不必算。
    lo = int(oy) - 8
    hi = int(oy + 120 * s) + 8
    for y in range(max(0, lo), min(CANVAS, hi)):
        for x in range(max(0, lo), min(CANVAS, hi)):
            i = y * CANVAS + x
            fx, fy = x + 0.5, y + 0.5

            for (ax, ay), (bx, by) in 經線:
                a = 覆蓋(線段距離(fx, fy, ax, ay, bx, by) - 經寬)
                if a > 0:
                    疊(px, i, WARP[0], WARP[1], WARP[2], a * WARP[3])

            a = 覆蓋(折線距離(fx, fy, 緯) - 緯寬)
            if a > 0:
                疊(px, i, *WEFT, a)

            for (nx, ny), size, rr in 節點:
                a = 覆蓋(圓角方距離(fx, fy, nx, ny, size, size, rr))
                if a > 0:
                    疊(px, i, *NODE, a)

    寫png(out, CANVAS, CANVAS, px)
    print(f'寫出 {out}')


if __name__ == '__main__':
    這裡 = pathlib.Path(__file__).parent
    寫logo(這裡 / 'diagram-loom-logo-solid.svg')
    main(sys.argv[1] if len(sys.argv) > 1 else str(這裡 / 'app-icon.png'))
