# -*- coding: utf-8 -*-
"""
墨钥 KeyHub · 应用图标生成器

Logo A「墨印 · 钥孔」：墨黑圆角方印 + 白色钥匙孔 + 酸橙绿点。
几何参数与 src/assets/brand/logo.svg 一致（512 画布，四周 16px 透明边距）。

生成 Tauri bundle 所需的全部图标尺寸：
  32x32.png / 128x128.png / 128x128@2x.png(=256) / icon.png / icon.ico / icon.icns
  以及 Windows Store 的 Square*Logo / StoreLogo。

说明：
  · 推荐直接用 `npx tauri icon src/assets/brand/logo.svg` 生成到 src-tauri/icons；
  · 本脚本输出到 scripts/icons（不随仓库分发），供离线/自定义尺寸时使用；
  · 依赖 Pillow。
"""
import os
import sys

try:
    from PIL import Image, ImageDraw
except ImportError:
    print("NEED_PIL")
    sys.exit(9)

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "icons")
os.makedirs(OUT, exist_ok=True)

# 品牌色（与鉴枢令牌一致）
INK = (38, 38, 38, 255)        # #262626
WHITE = (255, 255, 255, 255)   # 白色钥匙孔
ACID = (120, 234, 87, 255)     # #78EA57


def make_icon(px):
    """按 512 坐标系绘制「墨印 · 钥孔」，4x 超采样后缩小。"""
    S = px * 4

    def v(n):
        return int(round(S * n / 512))

    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # 圆角方印（16px 透明边距，圆角 120）
    d.rounded_rectangle(
        [v(16), v(16), v(496), v(496)],
        radius=v(120),
        fill=INK,
    )

    # 钥匙孔：圆头 + 柄 + 齿
    cx, cy = v(236), v(214)
    d.ellipse(
        [cx - v(66), cy - v(66), cx + v(66), cy + v(66)],
        fill=WHITE,
    )
    stem = v(44)
    d.rounded_rectangle(
        [cx - stem // 2, v(280), cx + stem // 2, v(378)],
        radius=stem // 2,
        fill=WHITE,
    )
    tooth = v(30)
    d.rounded_rectangle(
        [cx, v(326) - tooth // 2, v(288), v(326) + tooth // 2],
        radius=tooth // 2,
        fill=WHITE,
    )

    # 品牌绿点
    gx, gy, gr = v(386), v(386), v(24)
    d.ellipse([gx - gr, gy - gr, gx + gr, gy + gr], fill=ACID)

    return img.resize((px, px), Image.LANCZOS)


def main():
    sizes = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
        "Square30x30Logo.png": 30,
        "Square44x44Logo.png": 44,
        "Square71x71Logo.png": 71,
        "Square89x89Logo.png": 89,
        "Square107x107Logo.png": 107,
        "Square142x142Logo.png": 142,
        "Square150x150Logo.png": 150,
        "Square284x284Logo.png": 284,
        "Square310x310Logo.png": 310,
        "StoreLogo.png": 50,
    }

    cache = {}
    for name, px in sizes.items():
        if px not in cache:
            cache[px] = make_icon(px)
        cache[px].save(os.path.join(OUT, name), "PNG")

    # ICO：内嵌多尺寸
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    for px in ico_sizes:
        if px not in cache:
            cache[px] = make_icon(px)
    cache[256].save(
        os.path.join(OUT, "icon.ico"),
        format="ICO",
        sizes=[(p, p) for p in ico_sizes],
    )

    # ICNS：macOS
    try:
        cache[512].save(os.path.join(OUT, "icon.icns"), format="ICNS")
    except Exception as e:
        print("ICNS_WARN:", e)

    print("OK")
    for f in sorted(os.listdir(OUT)):
        print(" ", f, os.path.getsize(os.path.join(OUT, f)))


if __name__ == "__main__":
    main()
