"""Composite the recorded terminal and its exported ER diagram over a desktop."""

import argparse
import base64
import subprocess
from collections import Counter
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageEnhance, ImageFilter, ImageOps


def browser_capture(work, chrome, width, height):
    svg = (work / "diagram.svg").read_bytes()
    url = "data:image/svg+xml;base64," + base64.b64encode(svg).decode()
    page = work / "viewer.html"
    page.write_text(
        '<meta charset="utf-8"><style>*{box-sizing:border-box}body{margin:0;background:white;'
        "display:grid;place-items:center;height:100vh}img{width:92%;height:83%;object-fit:contain}"
        f'</style><img src="{url}">'
    )
    screenshot = work / "viewer.png"
    subprocess.run(
        [
            chrome,
            "--headless",
            "--disable-gpu",
            "--no-first-run",
            "--disable-background-networking",
            f"--user-data-dir={work}/chrome-profile",
            f"--window-size={width},{height}",
            "--hide-scrollbars",
            f"--screenshot={screenshot}",
            page.as_uri(),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return Image.open(screenshot).convert("RGB")


def transparent_terminal(frame, desktop, panel):
    # VHS draws the whole terminal with one background color. Key only that
    # color inside the window, preserving the application's selection colors.
    diff = ImageChops.difference(
        frame, Image.new("RGB", frame.size, frame.getpixel((200, 40)))
    )
    bands = diff.split()
    keep = ImageChops.lighter(ImageChops.lighter(bands[0], bands[1]), bands[2]).point(
        lambda x: 255 if x > 2 else 0
    )
    return Image.composite(frame, desktop, ImageChops.multiply(keep, panel))


def finish_view(frame, graph):
    final = frame.copy()
    w, h = frame.size
    x, y = (w - graph.width) // 2, (h - graph.height - 38) // 2
    bounds = (x, y, x + graph.width, y + graph.height + 38)
    shadow = Image.new("L", frame.size)
    ImageDraw.Draw(shadow).rounded_rectangle(bounds, 12, fill=150)
    final.paste((0, 0, 0), (0, 0, w, h), shadow.filter(ImageFilter.GaussianBlur(16)))
    window = Image.new("RGB", (graph.width, graph.height + 38), "#272831")
    window.paste(graph, (0, 38))
    draw = ImageDraw.Draw(window)
    for cx, color in [(18, "#fb6057"), (38, "#f7ba38"), (58, "#29c76f")]:
        draw.ellipse((cx - 5, 14, cx + 5, 24), fill=color)
    from PIL import ImageFont

    font = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 16)
    draw.text(
        (window.width // 2, 19),
        "er_partial_public_products.svg",
        font=font,
        anchor="mm",
        fill="#dadbe0",
    )
    mask = Image.new("L", window.size)
    ImageDraw.Draw(mask).rounded_rectangle(
        (0, 0, window.width - 1, window.height - 1), 12, fill=255
    )
    final.paste(window, (x, y), mask)
    return final


def compose(args):
    work = args.work.resolve()
    src = Image.open(work / "terminal.gif")
    w, h = src.size
    graph = browser_capture(work, args.chrome, int(w * 0.77), int(h * 0.36))
    wallpaper = ImageOps.fit(Image.open(args.wallpaper).convert("RGB"), src.size)
    wallpaper = ImageEnhance.Brightness(wallpaper).enhance(0.84)
    panel = Image.new("L", src.size)
    ImageDraw.Draw(panel).rounded_rectangle((22, 22, w - 23, h - 23), 11, fill=255)
    glass = Image.blend(
        wallpaper.filter(ImageFilter.GaussianBlur(4)),
        Image.new("RGB", src.size, "#1d202b"),
        0.82,
    )
    desktop = Image.composite(glass, wallpaper, panel)

    # Reserve colors for text, focus, and diff highlights before quantizing
    # the photograph. A shared palette avoids shimmer between frames.
    ui_colors = Counter()
    for i in range(0, src.n_frames, 20):
        src.seek(i)
        ui_colors.update(
            dict((rgb, n) for n, rgb in src.convert("RGB").getcolors(w * h))
        )
    swatches = Image.new("RGB", (len(ui_colors), 1))
    swatches.putdata(list(ui_colors))
    accents = swatches.quantize(colors=100).convert("RGB").getdata()
    colors = [rgb for rgb, _ in ui_colors.most_common(24)] + list(
        dict.fromkeys(accents)
    )
    colors.extend([(253, 79, 76), (244, 186, 20), (1, 204, 26)])
    colors.extend([(255, 255, 255), (0, 0, 0), (173, 216, 230)])
    sample = Image.new("RGB", (480, 380))
    sample.paste(desktop.resize((480, 280)), (0, 0))
    sample.paste(wallpaper.resize((360, 100)), (0, 280))
    sample.paste(graph.resize((120, 100)), (360, 280))
    colors = list(dict.fromkeys(colors))
    extra = sample.quantize(colors=256 - len(colors)).getpalette()
    palette = Image.new("P", (1, 1))
    rgb = [v for c in colors for v in c] + extra
    palette.putpalette((rgb + [0] * 768)[:768])
    desktop = desktop.quantize(
        palette=palette, dither=Image.Dither.FLOYDSTEINBERG
    ).convert("RGB")

    src.seek(0)
    previous = None
    frames = []
    elapsed = printed = 0
    for i in range(src.n_frames):
        src.seek(i)
        frame = src.convert("RGB")
        duration = src.info.get("duration", 40)
        changed = previous is None or ImageChops.difference(frame, previous).getbbox()
        elapsed += duration / 1.15
        ticks = round(elapsed / 10) * 10 - printed
        printed += ticks
        if changed:
            terminal = transparent_terminal(frame, desktop, panel)
            frames.append(
                [terminal.quantize(palette=palette, dither=Image.Dither.NONE), ticks]
            )
            previous = frame
        else:
            frames[-1][1] += ticks
    final = finish_view(terminal, graph)
    frames.append([final.quantize(palette=palette, dither=Image.Dither.NONE), 3500])
    frames[0][0].save(
        args.output,
        save_all=True,
        append_images=[f for f, _ in frames[1:]],
        optimize=False,
        loop=0,
        duration=[d for _, d in frames],
        disposal=1,
    )
    print(f"{len(frames)} visual changes, {sum(d for _, d in frames) / 1000:.2f}s")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("work", type=Path)
    parser.add_argument("wallpaper", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument(
        "--chrome",
        default="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    )
    compose(parser.parse_args())
