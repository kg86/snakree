#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""Draws SVG charts from the TSV files written by `cargo bench --bench bench`.

    benches/py/plot.py scaling RESULT.tsv OUTDIR
    benches/py/plot.py compare BASE.tsv NEW.tsv OUTDIR

`scaling` writes scaling.svg (slide), search.svg, window-effect.svg, memory.svg and build.svg.
`compare` writes compare-time.svg and compare-heap.svg; bars show NEW / BASE.
Only the standard library is used.
The SVG files follow the light or dark colour scheme of the viewer, and hovering a mark shows its value.
"""

import math
import os
import sys
from html import escape

# The light colours are written as presentation attributes on every element, so the charts stay
# readable in viewers that ignore <style> or CSS variables. The style sheet only overrides them
# for the dark scheme.
FONT = 'system-ui, -apple-system, "Segoe UI", sans-serif'
LIGHT = {
    "bg": 'fill="#fcfcfb"',
    "title": 'fill="#0b0b0b" font-size="15" font-weight="600"',
    "panel": 'fill="#0b0b0b" font-size="11" font-weight="600"',
    "muted": 'fill="#898781" font-size="10"',
    "text": 'fill="#52514e" font-size="11"',
    "link": 'stroke="#c3c2b7" stroke-width="3" stroke-linecap="round"',
    "cellw": 'fill="#ffffff" font-size="10"',
    "cellk": 'fill="#0b0b0b" font-size="10"',
    "grid": 'stroke="#e1e0d9" stroke-width="1"',
    "axis": 'stroke="#c3c2b7" stroke-width="1"',
    "line1": 'fill="none" stroke="#2a78d6" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"',
    "line2": 'fill="none" stroke="#eb6834" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"',
    "dot1": 'fill="#2a78d6" stroke="#fcfcfb" stroke-width="2"',
    "dot2": 'fill="#eb6834" stroke="#fcfcfb" stroke-width="2"',
    "good": 'fill="#2a78d6"',
    "bad": 'fill="#e34948"',
}
STYLE = """
@media (prefers-color-scheme: dark) {
  .bg { fill: #1a1a19; }
  text { fill: #c3c2b7; }
  .title, .panel { fill: #f0efec; }
  .muted { fill: #898781; }
  .link { stroke: #4a4a47; }
  .cellw { fill: #ffffff; } .cellk { fill: #0b0b0b; }
  .grid { stroke: #2c2c2a; }
  .axis { stroke: #383835; }
  .line1 { stroke: #3987e5; } .line2 { stroke: #d95926; }
  .dot1 { fill: #3987e5; stroke: #1a1a19; } .dot2 { fill: #d95926; stroke: #1a1a19; }
  .good { fill: #3987e5; } .bad { fill: #e66767; }
}
"""


def cls(name):
    """Returns the class and the light presentation attributes of a mark."""
    return f'class="{name}" {LIGHT[name]}'


def read(path):
    """Reads a result table; the columns are named by its header line."""
    rows = []
    fields = None
    with open(path) as f:
        for line in f:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            if line.startswith("scenario\t"):
                fields = line.split("\t")
                continue
            r = dict(zip(fields, line.split("\t")))
            r["window"] = None if r["window"] == "-" else int(r["window"])
            r["pattern_len"] = None if r.get("pattern_len", "-") == "-" else int(r["pattern_len"])
            for k in ("median_ms", "rate", "peak_heap_bytes", "rss_bytes"):
                r[k] = float(r[k])
            rows.append(r)
    return rows


def fmt_bytes(n):
    for unit, size in (("GiB", 1 << 30), ("MiB", 1 << 20), ("KiB", 1 << 10)):
        if n >= size and n % size == 0:
            return f"{n // size} {unit}"
    return f"{n} B"


def fmt_num(v):
    if v >= 1000:
        return f"{v / 1000:g}K"
    return f"{v:g}"


def svg(width, height, body):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
            f'viewBox="0 0 {width} {height}" role="img">\n<style>{STYLE}</style>\n'
            f'<rect {cls("bg")} width="{width}" height="{height}"/>\n{body}\n</svg>\n')


def text(x, y, s, name="", anchor="start"):
    c = cls(name) if name else f'class="text" {LIGHT["text"]}'
    return f'<text x="{x:.1f}" y="{y:.1f}" {c} font-family=\'{FONT}\' text-anchor="{anchor}">{escape(s)}</text>'


def header(title, subtitle, width):
    """Title and a subtitle that may span several lines separated by newlines."""
    lines = subtitle.split("\n")
    out = [text(16, 24, title, "title")]
    out += [text(16, 42 + 16 * i, line) for i, line in enumerate(lines)]
    return "\n".join(out)


def header_width(title, subtitle):
    """Returns the width needed by the title and the subtitle, with a rough character width."""
    return int(max([len(title) * 8.5] + [len(l) * 6.0 for l in subtitle.split("\n")]) + 32)


def legend(items, x, y):
    """items: (label, class suffix); a short line key with a dot, as on the marks."""
    out = []
    for label, k in items:
        out.append(f'<line x1="{x}" y1="{y - 4}" x2="{x + 18}" y2="{y - 4}" {cls(f"line{k}")}/>')
        out.append(f'<circle cx="{x + 9}" cy="{y - 4}" r="4" {cls(f"dot{k}")}/>')
        out.append(text(x + 26, y, label))
        x += 26 + 7 * len(label) + 18
    return "\n".join(out)


def decades(lo, hi):
    return [10.0 ** e for e in range(math.floor(math.log10(lo)), math.ceil(math.log10(hi)) + 1)]


def dumbbell(title, subtitle, unit, inputs, windows, series, xlabel):
    """Dumbbell chart: one row per input, one panel per window, on a shared log axis.

    series: (label, class suffix, getter(input, window) -> value or None).
    The two dots of a row are joined, so the gap between the series is the length of the link.
    """
    values = [v for d in inputs for _, _, g in series for w in windows
              if (v := g(d, w)) is not None and v > 0]
    if not values:
        return None
    ticks = decades(min(values), max(values))
    lo, hi = math.log10(ticks[0]), math.log10(ticks[-1])
    # Fastest first, judged on the first series averaged over the windows.
    def score(d):
        vs = [math.log(v) for w in windows if (v := series[0][2](d, w)) is not None and v > 0]
        return -sum(vs) / len(vs) if vs else 0
    inputs = sorted(inputs, key=score)
    row_h, pw, gap = 22, 250, 34
    label_w = max(len(d) for d in inputs) * 6.4 + 12
    left = 16 + label_w
    lines = subtitle.count("\n") + 1
    legend_y = 42 + 16 * lines + 8
    top = legend_y + 44
    width = max(int(left + len(windows) * (pw + gap)), header_width(title, subtitle))
    base_h = top + len(inputs) * row_h + 36
    height = base_h + 14
    lx = lambda v: (math.log10(v) - lo) / (hi - lo) * pw
    out = [header(title, subtitle, width), legend([(s[0], s[1]) for s in series], 16, legend_y)]
    for i, d in enumerate(inputs):
        out.append(text(left - 8, top + i * row_h + 4, d, "", "end"))
    for p, w in enumerate(windows):
        ox = left + p * (pw + gap)
        out.append(text(ox, top - 22, f"window {fmt_bytes(w)}", "panel"))
        out.append(text(ox + pw / 2, base_h + 6, xlabel, "", "middle"))
        for t in ticks:
            x = ox + lx(t)
            out.append(f'<line {cls("grid")} x1="{x:.1f}" x2="{x:.1f}" y1="{top - 12}" y2="{base_h - 28}"/>')
            out.append(text(x, base_h - 12, fmt_num(t), "muted", "middle"))
        for i, d in enumerate(inputs):
            y = top + i * row_h
            pts = [(label, k, g(d, w)) for label, k, g in series]
            pts = [(label, k, v) for label, k, v in pts if v is not None and v > 0]
            if len(pts) > 1:
                xs = [ox + lx(v) for _, _, v in pts]
                out.append(f'<line {cls("link")} x1="{min(xs):.1f}" x2="{max(xs):.1f}" y1="{y}" y2="{y}"/>')
            for label, k, v in pts:
                out.append(f'<circle {cls(f"dot{k}")} cx="{ox + lx(v):.1f}" cy="{y}" r="5">'
                           f'<title>{escape(d)}, {escape(label)}, window {fmt_bytes(w)}: {v:.3g} {escape(unit)}</title></circle>')
    return svg(width, height, "\n".join(out))


# Sequential blue ramp, light to dark, used by the heatmap.
RAMP = [(0xcd, 0xe2, 0xfb), (0x86, 0xb6, 0xef), (0x39, 0x87, 0xe5), (0x1c, 0x5c, 0xab), (0x0d, 0x36, 0x6b)]


def ramp(t):
    """Returns the colour at position t in [0, 1] of the ramp as a hex string."""
    t = min(max(t, 0.0), 1.0) * (len(RAMP) - 1)
    i = min(int(t), len(RAMP) - 2)
    f = t - i
    return "#%02x%02x%02x" % tuple(round(a + (b - a) * f) for a, b in zip(RAMP[i], RAMP[i + 1]))


def fmt_cell(v):
    return f"{v:.2f}" if v < 10 else f"{v:.1f}"


def heatmap(title, subtitle, unit, inputs, windows, columns, get, xlabel, column_label=str):
    """Table with one row per input and one panel per window, coloured by log value.

    columns are the column keys; get(input, window, column) -> value or None.
    """
    values = [v for d in inputs for w in windows for c in columns if (v := get(d, w, c)) is not None and v > 0]
    if not values:
        return None
    lo, hi = math.log(min(values)), math.log(max(values))
    pos = lambda v: (math.log(v) - lo) / ((hi - lo) or 1)
    last = windows[-1]

    def score(d):
        vs = [math.log(v) for c in columns if (v := get(d, last, c)) is not None and v > 0]
        return -sum(vs) / len(vs) if vs else 0
    inputs = sorted(inputs, key=score)
    cw, ch, gap = 52, 24, 30
    label_w = max(len(d) for d in inputs) * 6.4 + 12
    left = 16 + label_w
    lines = subtitle.count("\n") + 1
    legend_y = 42 + 16 * lines + 8
    top = legend_y + 52
    pw = cw * len(columns)
    width = max(int(left + len(windows) * (pw + gap)), header_width(title, subtitle), 16 + 4 * 130)
    base_h = top + len(inputs) * ch + 8
    height = base_h + 28
    out = [header(title, subtitle, width)]
    # Colour key: a strip of swatches with the extreme values and the decades between them.
    n = 40
    kx, kw = 16, 260
    for i in range(n):
        out.append(f'<rect x="{kx + i * kw / n:.1f}" y="{legend_y - 10}" width="{kw / n + 0.5:.1f}" height="12" fill="{ramp(i / (n - 1))}"/>')
    for d in decades(min(values), max(values)):
        if min(values) <= d <= max(values):
            x = kx + pos(d) * kw
            out.append(text(x, legend_y + 14, fmt_num(d), "muted", "middle"))
    out.append(text(kx + kw + 10, legend_y, f"{unit}, log colour scale"))
    for i, d in enumerate(inputs):
        out.append(text(left - 8, top + i * ch + ch / 2 + 4, d, "", "end"))
    for p, w in enumerate(windows):
        ox = left + p * (pw + gap)
        out.append(text(ox, top - 10, f"window {fmt_bytes(w)}", "panel"))
        for j, c in enumerate(columns):
            out.append(text(ox + j * cw + cw / 2, base_h + 14, column_label(c), "muted", "middle"))
        out.append(text(ox + pw / 2, base_h + 28, xlabel, "", "middle"))
        for i, d in enumerate(inputs):
            for j, c in enumerate(columns):
                v = get(d, w, c)
                x, y = ox + j * cw, top + i * ch
                if v is None or v <= 0:
                    out.append(text(x + cw / 2, y + ch / 2 + 4, "-", "muted", "middle"))
                    continue
                t = pos(v)
                out.append(f'<rect x="{x + 1}" y="{y + 1}" width="{cw - 2}" height="{ch - 2}" rx="2" fill="{ramp(t)}">'
                           f'<title>{escape(d)}, window {fmt_bytes(w)}, {escape(column_label(c))}: {v:.3g} {escape(unit)}</title></rect>')
                out.append(text(x + cw / 2, y + ch / 2 + 4, fmt_cell(v), "cellw" if t > 0.45 else "cellk", "middle"))
    return svg(width, base_h + 36, "\n".join(out))


def rounded_bar(x0, x1, y, h, r=4):
    """Bar from x0 to x1 with a rounded data end and a square baseline end."""
    if abs(x1 - x0) < 0.5:
        return ""
    r = min(r, abs(x1 - x0), h / 2)
    s = 1 if x1 > x0 else -1
    return (f'M{x0:.1f},{y:.1f} H{x1 - s * r:.1f} a{r},{r} 0 0 {1 if s > 0 else 0} {s * r},{r} '
            f'v{h - 2 * r:.1f} a{r},{r} 0 0 {1 if s > 0 else 0} {-s * r},{r} H{x0:.1f} Z')


def bar_chart(title, subtitle, items, axis, legend_items=None, xlabel=None):
    """Horizontal bars. items: (label, value, class, tooltip, value label).

    axis: dict with `x(v)` -> pixel, `ticks` [(value, label)], `base` value at the baseline.
    """
    row_h, bar_h = 20, 14
    label_w = max(len(i[0]) for i in items) * 6.4 + 12
    plot_w = 520
    left = 16 + label_w
    lines = subtitle.count("\n") + 1
    width = max(int(left + plot_w + 90), header_width(title, subtitle))
    top = 72 + 16 * lines
    base_h = top + len(items) * row_h + 30
    height = base_h + (22 if xlabel else 0)
    ax = axis["x"]
    out = [header(title, subtitle, width)]
    if legend_items:
        out.append(legend_bars(legend_items, 16, 42 + 16 * lines + 8))
    for v, lab in axis["ticks"]:
        x = left + ax(v)
        out.append(f'<line {cls("grid")} x1="{x:.1f}" x2="{x:.1f}" y1="{top - 6}" y2="{base_h - 24}"/>')
        out.append(text(x, base_h - 8, lab, "muted", "middle"))
    if xlabel:
        out.append(text(left + plot_w / 2, base_h + 12, xlabel, "", "middle"))
    bx = left + ax(axis["base"])
    out.append(f'<line {cls("axis")} x1="{bx:.1f}" x2="{bx:.1f}" y1="{top - 6}" y2="{base_h - 24}"/>')
    for i, (label, v, klass, tip, vlabel) in enumerate(items):
        y = top + i * row_h
        out.append(text(left - 8, y + 11, label, "", "end"))
        x1 = left + ax(v)
        d = rounded_bar(bx, x1, y, bar_h)
        out.append(f'<path {cls(klass)} d="{d}"><title>{escape(tip)}</title></path>' if d else
                   f'<title>{escape(tip)}</title>')
        tx = x1 + 6 if x1 >= bx else x1 - 6
        out.append(text(tx, y + 11, vlabel, "", "start" if x1 >= bx else "end"))
    return svg(width, height, "\n".join(out))


def legend_bars(items, x, y):
    out = []
    for label, cname in items:
        out.append(f'<rect {cls(cname)} x="{x}" y="{y - 10}" width="12" height="12" rx="2"/>')
        out.append(text(x + 18, y, label))
        x += 18 + 7 * len(label) + 20
    return "\n".join(out)


def write(path, content):
    if content is None:
        print(f"{path} skipped: no data")
        return
    with open(path, "w") as f:
        f.write(content)
    print(path)


def scaling(rows, outdir):
    # The search chart shows one pattern length: 16 if it was measured, else the middle one.
    plens = sorted({r["pattern_len"] for r in rows if r["pattern_len"] is not None})
    plen = 16 if 16 in plens or not plens else plens[len(plens) // 2]
    by = {(r["scenario"], r["dataset"], r["window"]): r for r in rows if r["pattern_len"] in (None, plen)}
    scenarios = {r["scenario"] for r in rows}
    second = next((n for n in ("search-near-miss", "search-miss") if n in scenarios), None)
    second_label = {"search-near-miss": "near miss", "search-miss": "miss"}.get(second, "")
    windows = sorted({r["window"] for r in rows if r["window"] is not None})
    inputs = list(dict.fromkeys(r["dataset"] for r in rows if r["window"] is not None))

    def getter(scenario, field="rate"):
        def get(d, w):
            r = by.get((scenario, d, w))
            return None if r is None else r[field]
        return get

    write(os.path.join(outdir, "scaling.svg"), dumbbell(
        "Sliding throughput: slide",
        "Higher is faster. Log axis, Mbyte/s. slide counts one push and one pop_front per byte.",
        "Mbyte/s", inputs, windows,
        [("slide", "1", getter("slide"))],
        "Throughput, Mbyte/s (log scale)"))
    hit_only = second is None
    write(os.path.join(outdir, "search.svg"), dumbbell(
        "Search throughput: hit" if hit_only else f"Search throughput: hit and {second_label}",
        f"Higher is faster. Log axis, million find() calls per second, patterns of {plen} bytes."
        + ("" if hit_only else f"\nThe link between the dots is the gap between a hit and a {second_label}."),
        "Mquery/s", inputs, windows,
        [("hit", "1", getter("search-hit"))] + ([] if hit_only else [(second_label, "2", getter(second))]),
        "Throughput, Mquery/s (log scale)"))

    # Throughput against pattern length, as a table: one chart per search kind.
    by_len = {(r["scenario"], r["dataset"], r["window"], r["pattern_len"]): r["rate"]
              for r in rows if r["pattern_len"] is not None}
    if len(plens) > 1:
        for scenario, label, name in (("search-hit", "hit", "search-length-hit.svg"),
                                      ("search-near-miss", "near miss", "search-length-near-miss.svg")):
            if scenario not in scenarios:
                continue
            write(os.path.join(outdir, name), heatmap(
                f"Search throughput by pattern length: {label}",
                "Higher is faster. Cell colour is on a log scale. A dash means the pattern could not be built.",
                "Mquery/s", inputs, windows, plens,
                lambda d, w, c, sc=scenario: by_len.get((sc, d, w, c)),
                "Pattern length (bytes)"))

    if len(windows) > 1 and any(k[0] == "slide" for k in by):
        first, last = windows[0], windows[-1]
        kept = [(d, by[("slide", d, last)]["rate"] / by[("slide", d, first)]["rate"])
                for d in inputs if ("slide", d, first) in by and ("slide", d, last) in by]
        kept.sort(key=lambda t: -t[1])
        top = max([1.0] + [x for _, x in kept])
        scale = 520 / top
        items = [(d, x, "good",
                  f'{d}: slide {by[("slide", d, first)]["rate"]:.3g} -> {by[("slide", d, last)]["rate"]:.3g} Mslide/s ({x:.0%} of the speed is kept)',
                  f"{x:.0%}") for d, x in kept]
        write(os.path.join(outdir, "window-effect.svg"), bar_chart(
            f"Speed of slide kept when the window grows from {fmt_bytes(first)} to {fmt_bytes(last)}",
            "slide throughput at the larger window as a share of that at the smaller window.\n"
            "100% means the larger window costs no speed, and 25% means it is four times slower.",
            items, {"x": lambda v: v * scale,
                    "ticks": [(t, f"{t:.0%}") for t in (0, 0.25, 0.5, 0.75, 1.0) if t <= top], "base": 0},
            xlabel=f"slide speed at {fmt_bytes(last)} / slide speed at {fmt_bytes(first)} (%)"))

    # The heap peak is the same for every input and every scenario, so the chart shows how much of it
    # the OS reports as resident. It uses the slide rows.
    mem_scenario = "slide"
    last = windows[-1] if windows else 0
    mem = [(d, by[(mem_scenario, d, last)]) for d in inputs if (mem_scenario, d, last) in by]
    if mem:
        per_byte = [r["peak_heap_bytes"] / last for _, r in mem]
        lo_b, hi_b = min(per_byte), max(per_byte)
        bytes_per_byte = f"{lo_b:.0f} bytes" if round(lo_b) == round(hi_b) else f"{lo_b:.0f} to {hi_b:.0f} bytes"
        share = sorted(((d, max(r["rss_bytes"], 0) / r["peak_heap_bytes"]) for d, r in mem), key=lambda t: -t[1])
        top = max([1.0] + [x for _, x in share])
        scale = 520 / top
        items = [(d, x, "good", f'{d}: RSS growth is {x:.0%} of the heap peak', f"{x:.0%}") for d, x in share]
        write(os.path.join(outdir, "memory.svg"), bar_chart(
            f"Resident memory as a share of the heap peak, window {fmt_bytes(last)} ({mem_scenario})",
            f"The heap peak is {bytes_per_byte} per window byte for every input.\nRSS is sampled and noisy.",
            items, {"x": lambda v: v * scale, "ticks": [(t, f"{t:.0%}") for t in (0, 0.25, 0.5, 0.75, 1.0) if t <= top],
                    "base": 0}, xlabel="RSS growth / heap peak (%)"))

    builds = sorted((r for r in rows if r["scenario"] == "build"), key=lambda r: -r["rate"])
    if builds:
        top = max(r["rate"] for r in builds)
        scale = 520 / top
        step = 10 ** math.floor(math.log10(top))
        step = step * (1 if top / step < 4 else 2 if top / step < 8 else 5)
        ticks = [(t, f"{t:g}") for t in [step * i for i in range(int(top / step) + 1)]]
        items = [(r["dataset"], r["rate"], "good", f'{r["dataset"]}: {r["rate"]:.3g} Mbyte/s',
                  f'{r["rate"]:.3g}') for r in builds]
        write(os.path.join(outdir, "build.svg"), bar_chart(
            "Construction throughput", "Higher is faster. Mbyte/s appending the input with no eviction.",
            items, {"x": lambda v: v * scale, "ticks": ticks, "base": 0},
            xlabel="Construction throughput (Mbyte/s)"))


def compare(base_rows, new_rows, outdir):
    key = lambda r: (r["scenario"], r["dataset"], r["input_bytes"], r["window"], r["pattern_len"])
    base = {key(r): r for r in base_rows}
    pairs = [(r, base[key(r)]) for r in new_rows if key(r) in base]
    if not pairs:
        sys.exit("no rows in common")
    for name, field, what in (("time", "median_ms", "time"), ("heap", "peak_heap_bytes", "peak heap")):
        usable = [(n, b) for n, b in pairs if b[field] > 0 and n[field] > 0]
        ratios = [n[field] / b[field] for n, b in usable]
        if not ratios or all(abs(math.log2(x)) < 0.001 for x in ratios):
            print(f"compare-{name}.svg skipped: no {what} difference")
            continue
        gm = math.exp(sum(math.log(x) for x in ratios) / len(ratios))
        total = len(usable)
        if name == "heap":
            # Most rows have an identical heap size, and a bar for each would bury the changes.
            changed = [(p, x) for p, x in zip(usable, ratios) if abs(x - 1) >= 0.005]
            usable, ratios = [p for p, _ in changed], [x for _, x in changed]
        note = "" if len(ratios) == total else f" {total - len(ratios)} unchanged rows are omitted."
        k = max(1, math.ceil(max(abs(math.log2(x)) for x in ratios)))
        half = 260
        ax = lambda v: half + math.log2(v) / k * half
        ticks = [(2.0 ** e, f"{2.0 ** e:g}x") for e in range(-k, k + 1)]
        items = []
        for (n, b), x in zip(usable, ratios):
            w = "" if n["window"] is None else f' {fmt_bytes(n["window"])}'
            plen = "" if n["pattern_len"] is None else f' len {n["pattern_len"]}'
            label = f'{n["scenario"]} {n["dataset"]}{w}{plen}'
            tip = f'{label}: {b[field]:.4g} -> {n[field]:.4g} ({x:.3f}x)'
            items.append((label, x, "good" if x <= 1 else "bad", tip, f"{x:.2f}x"))
        write(os.path.join(outdir, f"compare-{name}.svg"), bar_chart(
            f"{what.capitalize()} ratio, NEW / BASE",
            f"Log axis. Geometric mean {gm:.3f} over {total} rows.{note} Differences of a few percent are within noise.",
            items, {"x": ax, "ticks": ticks, "base": 1.0},
            [("below 1: smaller or faster", "good"), ("above 1: larger or slower", "bad")],
            xlabel=f"NEW / BASE {what} ratio (log scale; 1x means no change)"))


def main(argv):
    if len(argv) == 4 and argv[1] == "scaling":
        scaling(read(argv[2]), argv[3])
    elif len(argv) == 5 and argv[1] == "compare":
        compare(read(argv[2]), read(argv[3]), argv[4])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv)
