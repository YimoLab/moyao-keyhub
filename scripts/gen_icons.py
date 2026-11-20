# -*- coding: utf-8 -*-
"""
墨钥 KeyHub · 应用图标生成器

生成 Tauri bundle 所需的全部图标尺寸：
  32x32.png / 128x128.png / 128x128@2x.png(=256) / icon.png / icon.ico / icon.icns

设计：与侧栏 logo-seal 一致的「墨」字蓝底方章。
背景圆角矩形用 accent #2563eb，字形用白色，风格与 design-spec 对齐。

依赖 Pillow（若缺失则先安装）。
"""
import io
import os
import struct
import sys

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    print("NEED_PIL")
    sys.exit(9)

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "icons")
os.makedirs(OUT, exist_ok=True)

ACCENT = (37, 99, 235, 255)      # #2563eb
ACCENT_DARK = (29, 78, 216, 255)  # 稍深，用于底纹
INK = (255, 255, 255, 255)

# 尝试找一个能渲染中文的字体（只列公共路径，不写死本机目录）
FONT_CANDIDATES = [
    r"C:\Windows\Fonts\msyhbd.ttc",
    r"C:\Windows\Fonts\msyh.ttc",
    r"C:\Windows\Fonts\simhei.ttf",
    r"C:\Windows\Fonts\simsun.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Bold.ttc",
    "/System/Library/Fonts/PingFang.ttc",
]


def pick_font(size):
    for p in FONT_CANDIDATES:
        if os.path.exists(p):
            try:
                return ImageFont.truetype(p, size)
            except Exception:
                continue
    return None


def make_icon(px):
    """生成一枚图标：圆角方章 + 墨字。"""
    S = px * 4  # 4x 超采样，缩小后边缘干净
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # 圆角半径：约 22% 边长，视觉上接近 design-spec 的 logo-seal 比例
    r = int(S * 0.22)
    d.rounded_rectangle([0, 0, S - 1, S - 1], radius=r, fill=ACCENT)

    # 左上角极淡径向高光（低透明度，不破坏字形可读性）
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gd.ellipse([-S * 0.35, -S * 0.35, S * 0.75, S * 0.75], fill=(255, 255, 255, 26))
    img = Image.alpha_composite(img, glow)
    # 重新裁回圆角（高光溢出圆角外要抹掉）
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, S - 1, S - 1], radius=r, fill=255)
    img.putalpha(mask)
    d = ImageDraw.Draw(img)

    # 居中「墨」字
    txt = "墨"
    fs = int(S * 0.56)
    font = pick_font(fs)
    if font is None:
        # 拿不到中文字体时退化为几何钥匙孔，保证图标不空
        cx, cy = S // 2, S // 2
        rr = int(S * 0.17)
        d.ellipse(
            [cx - rr, cy - rr * 1.5, cx + rr, cy + rr * 0.5],
            outline=INK,
            width=int(S * 0.07),
        )
        d.line([cx, cy + rr * 0.4, cx, cy + rr * 2.0], fill=INK, width=int(S * 0.075))
        d.line(
            [cx, cy + rr * 1.3, cx + rr * 0.9, cy + rr * 1.3],
            fill=INK,
            width=int(S * 0.075),
        )
    else:
        d.text((S / 2, S / 2), txt, font=font, fill=INK, anchor="mm")

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

    # ICO：内嵌多尺寸，Windows 任务栏/资源管理器各取所需
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    ico_imgs = []
    for px in ico_sizes:
        if px not in cache:
            cache[px] = make_icon(px)
        ico_imgs.append(cache[px])
    cache[256].save(
        os.path.join(OUT, "icon.ico"),
        format="ICO",
        sizes=[(p, p) for p in ico_sizes],
    )

    # ICNS：macOS 需要，使用 Pillow 的 icns 写出（仅支持有限尺寸集）
    try:
        cache[512].save(
            os.path.join(OUT, "icon.icns"),
            format="ICNS",
        )
    except Exception as e:
        print("ICNS_WARN:", e)

    print("OK")
    for f in sorted(os.listdir(OUT)):
        print(" ", f, os.path.getsize(os.path.join(OUT, f)))


if __name__ == "__main__":
    main()
