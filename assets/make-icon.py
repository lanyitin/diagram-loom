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
import shutil
import struct
import subprocess
import sys
import tempfile
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

SOCIAL_W = 1280
SOCIAL_H = 640


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


GRADIENT = ('<linearGradient id="loom-body" x1="0" y1="0" x2="0" y2="1">'
            '<stop offset="0" stop-color="#27B489"/>'
            '<stop offset="1" stop-color="#127A58"/>'
            '</linearGradient>')

FONT = "-apple-system, 'Helvetica Neue', 'PingFang TC', sans-serif"


def 標記(x0, y0, size):
    """織布機標記的 SVG。logo 與社群預覽圖共用，所以兩邊不可能長得不一樣。

    座標沿用原本那個 120×120 的標記空間，等比縮放到 `size`。
    """
    k = size / 120
    at = lambda mx, my: f'{x0 + mx * k:.2f},{y0 + my * k:.2f}'

    warp = ''.join(
        f'<line x1="{x0 + mx * k:.2f}" y1="{y0 + 28 * k:.2f}"'
        f' x2="{x0 + mx * k:.2f}" y2="{y0 + 92 * k:.2f}" stroke="#fff"'
        f' stroke-opacity=".34" stroke-width="{7 * k:.2f}" stroke-linecap="round"/>'
        for mx in (25, 60, 95))
    weft = (f'<path d="M{at(25, 25)} C{at(45, 25)} {at(45, 60)} {at(60, 60)}'
            f' S{at(80, 95)} {at(95, 95)}" fill="none" stroke="#F26B3A"'
            f' stroke-width="{9 * k:.2f}" stroke-linecap="round" stroke-linejoin="round"/>')
    nodes = ''.join(
        f'<rect x="{x0 + (mx - 9) * k:.2f}" y="{y0 + (my - 9) * k:.2f}"'
        f' width="{18 * k:.2f}" height="{18 * k:.2f}" rx="{5.5 * k:.2f}" fill="#FFF2EA"/>'
        for mx, my in ((25, 25), (60, 60), (95, 95)))

    return (f'<path d="{squircle_path(x0, y0, size)}" fill="url(#loom-body)"/>'
            f'{warp}{weft}{nodes}')


def 寫logo(path, 字色, 次字色):
    """實心版的 logo。線稿在小尺寸會糊成一團，所以主要版本改成實心。

    深色底的 README 會把 `#14201c` 的字吃光，所以另外產一份亮字的，
    由 `<picture>` 的 `prefers-color-scheme` 去挑。
    """
    pathlib.Path(path).write_text(f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 680 230" width="680" height="230">
  <!-- 由 assets/make-icon.py 產生。圓角與 app icon 共用同一組超橢圓參數。 -->
  <defs>{GRADIENT}</defs>
  {標記(280, 15, 120)}
  <text x="340" y="175" text-anchor="middle" font-family="sans-serif" font-size="36" font-weight="600" fill="{字色}">DiagramLoom</text>
  <text x="340" y="205" text-anchor="middle" font-family="sans-serif" font-size="13" fill="{次字色}">Weave data into C4 model diagrams</text>
</svg>
''')
    print(f'寫出 {path}')


def 估寬(s, size):
    """粗估一段字的寬度。只用來排 chip，不必準，不重疊就夠了。"""
    return sum((1.0 if ord(c) > 0x2E80 else 0.55) * size for c in s)


def 藥丸(x0, y0, 標籤, size=15):
    """一排圓角標籤。回傳 (svg, 右緣)。"""
    out, x = [], x0
    for t in 標籤:
        w = 估寬(t, size) + 28
        out.append(
            f'<rect x="{x:.1f}" y="{y0}" width="{w:.1f}" height="34" rx="17" fill="#E7F4EE"/>'
            f'<text x="{x + w / 2:.1f}" y="{y0 + 22}" text-anchor="middle"'
            f' font-family="{FONT}" font-size="{size}" fill="#1C6B52">{t}</text>')
        x += w + 12
    return ''.join(out), x - 12


def 寫social(path):
    """GitHub 的 social preview（1280×640）。

    左邊是身分，右邊是這個工具真正在乎的事——覆蓋率矩陣上那一格紅的。
    畫圖的工具很多，會告訴你「dev 少了一條」的沒幾個，所以那格要是主角。
    """
    欄 = ('prod', 'test', 'dev')
    欄心 = (990, 1068, 1146)
    列 = (('order-api', (1, 1, 1)),
          ('redis', (1, 1, 0)),
          ('consul', (1, 1, 1)),
          ('batch-job', (1, 1, 1)))
    列心 = (216, 268, 320, 372)

    # 背景那幾條淡到幾乎看不見的直線，是經線。
    經線 = ''.join(f'<line x1="{x}" y1="0" x2="{x}" y2="640" stroke="#E9F1EE" stroke-width="1.5"/>'
                   for x in range(40, 1280, 40))

    格 = []
    for (名字, 有), cy in zip(列, 列心):
        格.append(f'<text x="740" y="{cy + 6}" font-family="{FONT}" font-size="17"'
                  f' fill="#2A3A35">{名字}</text>')
        for cx, 這格 in zip(欄心, 有):
            if 這格:
                格.append(f'<path d="M{cx - 8},{cy} L{cx - 2.5},{cy + 6} L{cx + 8},{cy - 7}"'
                          f' fill="none" stroke="#27B489" stroke-width="3.4"'
                          f' stroke-linecap="round" stroke-linejoin="round"/>')
            else:
                格.append(
                    f'<rect x="{cx - 26}" y="{cy - 18}" width="52" height="36" rx="9" fill="#FBE1DD"/>'
                    f'<line x1="{cx - 6}" y1="{cy - 6}" x2="{cx + 6}" y2="{cy + 6}"'
                    f' stroke="#E0483B" stroke-width="3.4" stroke-linecap="round"/>'
                    f'<line x1="{cx - 6}" y1="{cy + 6}" x2="{cx + 6}" y2="{cy - 6}"'
                    f' stroke="#E0483B" stroke-width="3.4" stroke-linecap="round"/>')
        if cy != 列心[-1]:
            格.append(f'<line x1="736" y1="{cy + 26}" x2="1168" y2="{cy + 26}" stroke="#EFF5F2"/>')

    欄名 = ''.join(f'<text x="{cx}" y="168" text-anchor="middle" font-family="{FONT}"'
                   f' font-size="14" fill="#8A9B95">{名}</text>'
                   for cx, 名 in zip(欄心, 欄))

    藥丸1, _ = 藥丸(80, 428, ('Rust core', 'Tauri + Vue', 'maxGraph'))
    藥丸2, _ = 藥丸(80, 474, ('Excel 匯入', '.drawio', 'MCP 給 AI Agent'))

    # 畫布刻意做成正方形、卡片垂直置中——這是為了遷就 qlmanage 與 sips：
    #   * qlmanage 一律輸出正方形縮圖，而且固定用 2× 算，宣告成 640×640
    #     才會剛好回來 1280×1280（直接宣告 1280 會超過上限，從右邊被裁掉）。
    #   * sips 的 --cropOffset 沒有作用，裁切永遠從中心。
    # 兩件事湊起來的唯一解就是：置中擺，然後從中心裁。
    pathlib.Path(path).write_text(f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {SOCIAL_W} {SOCIAL_W}" width="{SOCIAL_W // 2}" height="{SOCIAL_W // 2}">
  <!-- 由 assets/make-icon.py 產生。改了要重跑 `mise run icon` 並重新上傳到 GitHub。 -->
  <defs>{GRADIENT}</defs>
  <rect width="{SOCIAL_W}" height="{SOCIAL_W}" fill="#fff"/>
  <g transform="translate(0,{(SOCIAL_W - SOCIAL_H) // 2})">
  <rect width="{SOCIAL_W}" height="{SOCIAL_H}" fill="#F4F8F6"/>
  {經線}

  {標記(80, 92, 104)}
  <text x="208" y="150" font-family="{FONT}" font-size="44" font-weight="600" fill="#14201C">DiagramLoom</text>
  <text x="210" y="182" font-family="{FONT}" font-size="18" fill="#6B7B76">Weave data into C4 model diagrams</text>

  <text x="80" y="290" font-family="{FONT}" font-size="36" font-weight="600" fill="#14201C">把部署與連線資訊</text>
  <text x="80" y="338" font-family="{FONT}" font-size="36" font-weight="600" fill="#14201C">織成 C4 Model 圖</text>
  <text x="80" y="386" font-family="{FONT}" font-size="20" fill="#5E706B">重點不是畫圖，是數百條連線一條都不要漏</text>
  {藥丸1}
  {藥丸2}
  <text x="80" y="556" font-family="{FONT}" font-size="16" fill="#93A6A0">github.com/lanyitin/diagram-loom</text>

  <rect x="700" y="76" width="504" height="488" rx="22" fill="#fff" stroke="#DFEAE5"/>
  <text x="736" y="124" font-family="{FONT}" font-size="23" font-weight="600" fill="#14201C">環境覆蓋率</text>
  {欄名}
  <line x1="736" y1="182" x2="1168" y2="182" stroke="#E6EFEB"/>
  {''.join(格)}

  <rect x="736" y="432" width="432" height="80" rx="14" fill="#FDEDEA"/>
  <circle cx="766" cy="472" r="6" fill="#E0483B"/>
  <text x="786" y="466" font-family="{FONT}" font-size="16" font-weight="600" fill="#A32F25">L001 · redis 在 dev 沒有任何實現</text>
  <text x="786" y="492" font-family="{FONT}" font-size="14" fill="#9A6259">存檔時 lint，缺什麼直接條列出來</text>
  </g>
</svg>
''')
    print(f'寫出 {path}')


def 光柵化(svg, png, w, h):
    """把 SVG 轉成 PNG——這次可以用 qlmanage。

    社群預覽圖本來就有不透明底，所以 qlmanage 把圖壓在白底上不是問題，
    app icon 才不能走這條路（它需要透明的留白，見檔頭）。

    `svg` 必須是邊長 `w` 的正方形、內容垂直置中——理由寫在 `寫social`。
    """
    svg, png = pathlib.Path(svg), pathlib.Path(png)
    with tempfile.TemporaryDirectory() as d:
        subprocess.run(['qlmanage', '-t', '-s', str(w), '-o', d, str(svg)],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        出 = pathlib.Path(d) / (svg.name + '.png')
        if not 出.exists():
            raise SystemExit(f'qlmanage 沒有產出 {出.name}')
        shutil.copyfile(出, png)
    subprocess.run(['sips', '-c', str(h), str(w), str(png)],
                   check=True, stdout=subprocess.DEVNULL)
    print(f'寫出 {png}')


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
    寫logo(這裡 / 'diagram-loom-logo-solid.svg', '#14201C', '#6B7B76')
    寫logo(這裡 / 'diagram-loom-logo-dark.svg', '#EAF2EE', '#9FB3AC')
    寫social(這裡 / 'social-preview.svg')
    光柵化(這裡 / 'social-preview.svg', 這裡 / 'social-preview.png', SOCIAL_W, SOCIAL_H)
    main(sys.argv[1] if len(sys.argv) > 1 else str(這裡 / 'app-icon.png'))
