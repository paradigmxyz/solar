# /// script
# dependencies = ["matplotlib"]
# ///
"""Render the blog figures in blog/images/ from the raw results.

Usage: uv run blog/make_charts.py

Fonts (Source Serif 4, IBM Plex Mono) are downloaded to ~/.cache/solar-blog-fonts on first run.
"""

import json
import math
import sys
import urllib.request
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib import font_manager, ticker
from matplotlib.patches import FancyArrowPatch, Rectangle

BLOG = Path(__file__).resolve().parent
sys.path.insert(0, str(BLOG))
import make_tables as mt

OUT = BLOG / "images"
BOX = BLOG / "box"
EVIDENCE = BLOG / "solar-blog-evidence"

GREEN = "#00E10A"
GREY = "#D9D9D9"
PALE = "#DDE8DD"
MID = "#8CEB90"
FRAME = "#E3E3E3"
GRID = "#EFEFEF"
MUTED = "#8C8C8C"
INK = "#111111"

FONTS = {
    "SourceSerif4-Regular.ttf": "https://github.com/adobe-fonts/source-serif/raw/release/TTF/SourceSerif4-Regular.ttf",
    "IBMPlexMono-Regular.ttf": "https://github.com/google/fonts/raw/main/ofl/ibmplexmono/IBMPlexMono-Regular.ttf",
}


def load_fonts():
    cache = Path.home() / ".cache/solar-blog-fonts"
    cache.mkdir(parents=True, exist_ok=True)
    for name, url in FONTS.items():
        path = cache / name
        if not path.exists():
            urllib.request.urlretrieve(url, path)
        font_manager.fontManager.addfont(str(path))


SERIF = {"family": "Source Serif 4"}
MONO = {"family": "IBM Plex Mono"}

W, H = 9.0, 5.05
DPI = 200


def figure(title, subtitle=None, footer=None, height=H):
    """A white card with a thin frame, a serif title and a mono subtitle, like Paradigm's charts."""
    fig = plt.figure(figsize=(W, height), dpi=DPI)
    fig.patch.set_facecolor("white")
    fig.add_artist(
        Rectangle(
            (0.002, 0.002),
            0.996,
            0.996,
            transform=fig.transFigure,
            fill=False,
            ec=FRAME,
            lw=1.5,
        )
    )
    top = 1 - 0.38 / height
    fig.text(0.04, top, title, fontsize=22, color=INK, va="top", **SERIF)
    if subtitle:
        fig.text(
            0.04,
            top - 0.44 / height,
            subtitle,
            fontsize=11.5,
            color=MUTED,
            va="top",
            **MONO,
        )
    if footer:
        y = 0.62 / height
        fig.add_artist(
            plt.Line2D(
                [0.002, 0.998], [y, y], transform=fig.transFigure, color=FRAME, lw=1.5
            )
        )
        n = len(footer)
        for i, (text, style) in enumerate(footer):
            if i:
                x = i / n
                fig.add_artist(
                    plt.Line2D(
                        [x, x],
                        [0.002, y],
                        transform=fig.transFigure,
                        color=FRAME,
                        lw=1.5,
                    )
                )
            font = MONO if style == "mono" else SERIF
            fig.text(
                (i + 0.5) / n,
                y / 2,
                text,
                fontsize=11.5,
                color=MUTED if style == "muted" else INK,
                ha="center",
                va="center",
                **font,
            )
    return fig


def style_axes(ax, xgrid=False):
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
    for side in ("left", "bottom"):
        ax.spines[side].set_color(FRAME)
        ax.spines[side].set_linewidth(1.2)
    ax.tick_params(colors=INK, length=0, labelsize=11)
    for label in ax.get_xticklabels() + ax.get_yticklabels():
        label.set_fontfamily(MONO["family"])
    ax.grid(axis="x" if xgrid else "y", color=GRID, lw=1.2)
    ax.set_axisbelow(True)


def callout(ax, x, y, text, **kw):
    ax.annotate(
        text,
        (x, y),
        ha="center",
        va="bottom",
        fontsize=12,
        color=INK,
        fontfamily=MONO["family"],
        bbox={"boxstyle": "square,pad=0.45", "fc": "white", "ec": "#CFCFCF", "lw": 1.2},
        **kw,
    )


def legend(fig, items, height=H, x=0.80):
    top = 1 - 0.42 / height
    for i, (label, color) in enumerate(items):
        y = top - i * 0.3 / height
        fig.add_artist(
            Rectangle(
                (x, y - 0.09 / height),
                0.026,
                0.1 / height,
                transform=fig.transFigure,
                color=color,
            )
        )
        fig.text(
            x + 0.035,
            y - 0.04 / height,
            label,
            fontsize=12,
            va="center",
            color=INK,
            **MONO,
        )


def save(fig, name):
    OUT.mkdir(exist_ok=True)
    fig.savefig(OUT / name, dpi=DPI, facecolor="white")
    plt.close(fig)
    print(OUT / name)


def results(name):
    return {
        r["test_id"]: r["compilers"]
        for r in json.loads((BOX / name).read_text())["results"]
    }


SHARED = results("shared-all-results.json")
SLANG = results("shared-slang-results.json")
ORIGINAL = results("original-all-results.json")

HW = ("AMD EPYC 9254, 24 cores", "serif")
SRC = ("Source: Solar benchmark suite", "muted")


def ctime(rows, case, compiler):
    e = rows[case].get(compiler)
    return mt.ctime(e) if mt.ok(e) else None


def corpus_ratios(path, ref, suites):
    """Geomean speedup, runtime gas and deploy gas ratios of Solar against `ref`, as in make_tables."""
    data = json.loads((BOX / path).read_text())["results"]
    t, g, d = [], [], []
    for r in data:
        if r["suite"] not in suites:
            continue
        s, o = r["compilers"].get("solar"), r["compilers"].get(ref)
        if not mt.ok(s) or not mt.ok(o):
            continue
        if mt.ctime(o) and mt.ctime(s):
            t.append(mt.ctime(o) / mt.ctime(s))
        gs, go = mt.gas(s, o), mt.gas(o, s)
        if gs and go:
            g.append(gs / go)
        if s.get("deploy_gas") and o.get("deploy_gas"):
            d.append(s["deploy_gas"] / o["deploy_gas"])
    return mt.gm(t), mt.gm(g), mt.gm(d)


def morpho():
    case = "morpho-blue-project"
    bars = [
        ("solc 0.8.38", ctime(SHARED, case, "solc"), GREY),
        ("oksolc 0.2.2", ctime(SHARED, case, "oksolc"), GREY),
        ("slang-solx 0.1.8", ctime(SHARED, case, "solx"), GREY),
        ("Slang preview", ctime(SLANG, case, "solx"), GREY),
        ("Solar", ctime(SHARED, case, "solar"), GREEN),
    ]
    fig = figure(
        "Compiling Morpho Blue",
        "Build time (seconds), lower is better",
        [HW, ("via-IR, Cancun, median of 5", "mono"), SRC],
    )
    ax = fig.add_axes([0.24, 0.22, 0.68, 0.53])
    ys = range(len(bars))[::-1]
    ax.barh(ys, [b[1] for b in bars], color=[b[2] for b in bars], height=0.62)
    ax.set_yticks(list(ys), [b[0] for b in bars])
    for y, (_, v, _) in zip(ys, bars):
        ax.text(
            v + 0.35,
            y,
            f"{v:.2f} s",
            va="center",
            fontsize=11.5,
            color=INK,
            fontfamily=MONO["family"],
        )
    speedup = bars[0][1] / bars[-1][1]
    ax.text(
        bars[-1][1] + 5.5,
        0,
        f"{speedup:.0f}× faster than solc",
        va="center",
        fontsize=12,
        color=INK,
        fontfamily=MONO["family"],
        bbox={"boxstyle": "square,pad=0.45", "fc": "white", "ec": "#CFCFCF", "lw": 1.2},
    )
    ax.set_xlim(0, 37)
    style_axes(ax, xgrid=True)
    for label in ax.get_yticklabels():
        label.set_fontfamily(SERIF["family"])
        label.set_fontsize(14)
    save(fig, "compile-morpho.png")


PROJECTS = [
    ("morpho-blue-project", "Morpho Blue"),
    ("solady-0.1.26-project", "Solady"),
    ("prb-math-4.1.1-project", "PRBMath"),
    ("forge-std-1.16.1-project", "forge-std"),
    ("openzeppelin-5.6.1-project", "OpenZeppelin"),
    ("tempo-zones-project", "Tempo Zones"),
]


def projects():
    fig = figure(
        "Solar vs. solc Build Time",
        "Whole project (seconds)",
        [HW, ("via-IR, Cancun, median of 5", "mono"), SRC],
    )
    legend(fig, [("solc 0.8.38", GREY), ("Solar", GREEN)])
    ax = fig.add_axes([0.09, 0.25, 0.88, 0.5])
    w = 0.36
    for i, (case, _) in enumerate(PROJECTS):
        solc, solar = ctime(SHARED, case, "solc"), ctime(SHARED, case, "solar")
        ax.bar(i - w / 2, solc, w, color=GREY)
        ax.bar(i + w / 2, solar, w, color=GREEN)
        callout(ax, i, solc + 5, f"{solc / solar:.0f}×\n{solar:.2f} s")
    ax.set_xticks(range(len(PROJECTS)), [p[1] for p in PROJECTS])
    ax.set_ylim(0, 112)
    style_axes(ax)
    for label in ax.get_xticklabels():
        label.set_fontfamily(SERIF["family"])
        label.set_fontsize(12.5)
    save(fig, "compile-projects.png")


def compare():
    refs = [
        ("vs. solc 0.8.38", "shared-all-results.json", "solc"),
        ("vs. slang-solx 0.1.8", "shared-all-results.json", "solx"),
        ("vs. Slang preview", "shared-slang-results.json", "solx"),
        ("vs. oksolc 0.2.2", "shared-all-results.json", "oksolc"),
    ]
    rows = []
    for label, path, ref in refs:
        t, _, _ = corpus_ratios(path, ref, {"heavy"})
        _, g, d = corpus_ratios(path, ref, {"repository", "large"})
        rows.append((label, t, 100 * (1 - g), 100 * (1 - d)))
    fig = figure(
        "Solar vs. Every Solidity Compiler",
        "Same settings for every compiler: via-IR, Cancun",
        [HW, ("Gas: 15 real workloads", "mono"), SRC],
    )
    panels = [
        ("Faster builds", 1, "{:.0f}×", 95),
        ("Less gas per call", 2, "{:.1f}%", 7.6),
        ("Cheaper deploys", 3, "{:.0f}%", 50),
    ]
    ys = list(range(len(rows)))[::-1]
    for k, (name, idx, f, xmax) in enumerate(panels):
        left = 0.22 + k * 0.265
        ax = fig.add_axes([left, 0.2, 0.2, 0.46])
        vals = [r[idx] for r in rows]
        ax.barh(ys, vals, color=GREEN, height=0.6)
        for y, v in zip(ys, vals):
            ax.text(
                v + xmax * 0.03,
                y,
                f.format(v),
                va="center",
                fontsize=11.5,
                color=INK,
                fontfamily=MONO["family"],
            )
        ax.set_xlim(0, xmax * 1.3)
        ax.set_xticks([])
        style_axes(ax, xgrid=True)
        ax.spines["bottom"].set_visible(False)
        ax.set_title(
            name, fontsize=15, color=INK, loc="left", pad=10, fontfamily=SERIF["family"]
        )
        if k == 0:
            ax.set_yticks(ys, [r[0] for r in rows])
            for label in ax.get_yticklabels():
                label.set_fontfamily(SERIF["family"])
                label.set_fontsize(13)
        else:
            ax.set_yticks([])
    save(fig, "compare-compilers.png")


WORKLOADS = {
    "openzeppelin-p256": "OpenZeppelin P256",
    "solady-lib-string": "Solady LibString",
    "nitro-one-step-proof": "Arbitrum one-step proof",
    "aave-l2-encoder": "Aave L2 encoder",
    "solady-encoding": "Solady encoding",
    "solady-algorithms": "Solady algorithms",
    "tempo-zones-portal": "Tempo Zones portal",
    "openzeppelin-governor": "OpenZeppelin Governor",
    "openzeppelin-vesting-wallet": "OpenZeppelin VestingWallet",
    "lilweb3-ens": "lil web3 ENS",
    "lilweb3-flashloan": "lil web3 flash loan",
    "maple-erc20": "Maple ERC20",
    "solady-signature-checker": "Solady SignatureChecker",
    "openzeppelin-erc20-mock": "OpenZeppelin ERC20",
    "lilweb3-fractional": "lil web3 fractional",
}


def gas_workloads():
    rows = []
    for case, label in WORKLOADS.items():
        s, o = ORIGINAL[case]["solar"], ORIGINAL[case]["solc"]
        rows.append((label, 100 * (1 - mt.gas(s, o) / mt.gas(o, s))))
    rows.sort(key=lambda r: r[1])
    h = 7.2
    fig = figure(
        "Gas Saved vs. solc",
        "Per-call gas on real contracts, each project's own settings",
        [("Includes 21,000 intrinsic gas per call", "mono"), SRC],
        height=h,
    )
    ax = fig.add_axes([0.33, 0.13, 0.6, 0.72])
    ys = range(len(rows))
    ax.barh(ys, [r[1] for r in rows], color=GREEN, height=0.62)
    ax.set_yticks(list(ys), [r[0] for r in rows])
    for y, (_, v) in zip(ys, rows):
        ax.text(
            v + 0.25,
            y,
            f"{v:.1f}%",
            va="center",
            fontsize=11,
            color=INK,
            fontfamily=MONO["family"],
        )
    ax.set_xlim(0, 18)
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:.0f}%"))
    style_axes(ax, xgrid=True)
    for label in ax.get_yticklabels():
        label.set_fontfamily(SERIF["family"])
        label.set_fontsize(13)
    save(fig, "gas-vs-solc.png")


def factorial():
    case = "factorial"
    s = ORIGINAL[case]["solar"]
    bars = [
        ("solc 0.8.38", mt.gas(ORIGINAL[case]["solc"], s), GREY),
        ("slang-solx 0.1.8", mt.gas(ORIGINAL[case]["solx"], s), GREY),
        ("Slang preview", mt.gas(SLANG[case]["solx"], SLANG[case]["solar"]), GREY),
        ("Solar", mt.gas(s, ORIGINAL[case]["solc"]), GREEN),
    ]
    fig = figure(
        "Gas Cost of a Checked Factorial",
        "Transaction gas, summed over the workload, lower is better",
        [("No unchecked blocks", "mono"), SRC],
    )
    ax = fig.add_axes([0.12, 0.25, 0.84, 0.52])
    xs = range(len(bars))
    ax.bar(xs, [b[1] for b in bars], color=[b[2] for b in bars], width=0.55)
    for x, (_, v, _) in zip(xs, bars):
        callout(ax, x, v + 9000, f"{v:,}")
    ax.set_xticks(list(xs), [b[0] for b in bars])
    ax.set_ylim(0, 280000)
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v / 1000:.0f}K"))
    style_axes(ax)
    for label in ax.get_xticklabels():
        label.set_fontfamily(SERIF["family"])
        label.set_fontsize(14)
    save(fig, "gas-factorial.png")


SUITES = [
    "FixedPointMathLib",
    "SafeTransferLib",
    "ERC20",
    "ERC721",
    "ERC1155",
    "ERC4626",
    "LibString",
    "MerkleProofLib",
    "SignatureCheckerLib",
    "LibSort",
    "LibBit",
    "Base64",
    "ReentrancyGuard",
]


def safe_solady():
    data = json.loads(
        (EVIDENCE / "safe-solady-upstream-suites-results.json").read_text()
    )
    up, safe = data["solc-upstream"]["tests"], data["solar-safe"]["tests"]
    ratios = [
        safe[n]["gas"] / t["gas"]
        for n, t in up.items()
        if n in safe
        and t["status"] == safe[n]["status"] == "Success"
        and t["gas"]
        and safe[n]["gas"]
    ]
    suites = {}
    for line in (
        (EVIDENCE / "safe-solady-upstream-suites-gas.md").read_text().splitlines()
    ):
        cells = [c.strip() for c in line.strip("|").split("|")]
        if line.startswith("| `test/"):
            suites[cells[0].strip("`").removeprefix("test/").removesuffix(".t.sol")] = (
                float(cells[4].rstrip("x"))
            )
    rows = [
        (
            "All tests (geomean)",
            100 * math.exp(sum(map(math.log, ratios)) / len(ratios)),
        )
    ]
    rows += sorted(((s, 100 * suites[s]) for s in SUITES), key=lambda r: r[1])
    h = 7.2
    fig = figure(
        "Safe Solady vs. Solady",
        "Gas per suite vs. Solady built by solc",
        [("Solady's own test suite", "mono"), ("13 of 70 suites shown", "muted")],
        height=h,
    )
    legend(
        fig, [("Solady, solc", GREY), ("Safe Solady, Solar", GREEN)], height=h, x=0.70
    )
    ax = fig.add_axes([0.28, 0.13, 0.66, 0.68])
    ys = list(range(len(rows)))[::-1]
    w = 0.38
    ax.barh([y + w / 2 for y in ys], [100] * len(rows), w, color=GREY)
    ax.barh([y - w / 2 for y in ys], [r[1] for r in rows], w, color=GREEN)
    for y, (_, v) in zip(ys, rows):
        ax.text(
            v + 1.2,
            y - w / 2,
            f"{v:.0f}%",
            va="center",
            fontsize=10.5,
            color=INK,
            fontfamily=MONO["family"],
        )
    ax.set_yticks(ys, [r[0] for r in rows])
    ax.set_xlim(0, 112)
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:.0f}%"))
    style_axes(ax, xgrid=True)
    for label in ax.get_yticklabels():
        label.set_fontfamily(SERIF["family"])
        label.set_fontsize(13)
    save(fig, "safe-solady.png")


def pipeline():
    fig = plt.figure(figsize=(W, 3.3), dpi=DPI)
    fig.add_artist(
        Rectangle(
            (0.002, 0.004),
            0.996,
            0.992,
            transform=fig.transFigure,
            fill=False,
            ec=FRAME,
            lw=1.5,
        )
    )
    ax = fig.add_axes([0, 0, 1, 1])
    ax.set_xlim(0, 100)
    ax.set_ylim(0, 34)
    ax.axis("off")
    stages = ["Solidity", "AST", "HIR", "MIR", "EVM IR", "Bytecode"]
    notes = {
        3: "inlining, CSE,\ncheck removal,\nloop opts",
        4: "tail merging,\nblock layout,\npeepholes",
    }
    bw, gap, x0, y0, bh = 12.4, 4.0, 3.8, 15, 8
    for i, s in enumerate(stages):
        x = x0 + i * (bw + gap)
        ax.add_patch(Rectangle((x, y0), bw, bh, color=PALE, lw=0))
        ax.text(
            x + bw / 2,
            y0 + bh / 2,
            s,
            ha="center",
            va="center",
            fontsize=15,
            color=INK,
            **SERIF,
        )
        if i:
            ax.add_patch(
                FancyArrowPatch(
                    (x - gap + 0.4, y0 + bh / 2),
                    (x - 0.3, y0 + bh / 2),
                    arrowstyle="-|>",
                    mutation_scale=12,
                    color="#7A7A7A",
                    lw=1.5,
                )
            )
        if i in notes:
            # Self-loop over the box: the stage rewrites its own IR.
            ax.add_patch(
                FancyArrowPatch(
                    (x + bw * 0.75, y0 + bh),
                    (x + bw * 0.25, y0 + bh),
                    connectionstyle="arc3,rad=1.1",
                    arrowstyle="-|>",
                    mutation_scale=12,
                    color=GREEN,
                    lw=2,
                )
            )
            ax.text(
                x + bw / 2,
                y0 + bh + 4.2,
                "optimization\npasses",
                ha="center",
                va="bottom",
                fontsize=9.5,
                color=INK,
                **MONO,
            )
            ax.plot([x + bw / 2] * 2, [y0, 12], color=FRAME, lw=1.5)
            ax.add_patch(Rectangle((x - 0.8, 2), bw + 1.6, 10, color="#EBEBEB", lw=0))
            ax.text(
                x + bw / 2,
                7,
                notes[i],
                ha="center",
                va="center",
                fontsize=8.5,
                color=INK,
                linespacing=1.4,
                **MONO,
            )
    save(fig, "pipeline.png")


def execution():
    ops = json.loads((BOX / "opcodes-per-case.json").read_text())["objectives"]["gas"][
        "cases"
    ]
    evm2 = json.loads((BOX / "evm2-per-case.json").read_text())["cases"]
    rows = []
    for case, label in WORKLOADS.items():
        o = ops.get(case, {})
        s, legacy, ir = (
            evm2[v].get(case) for v in ("solar-gas", "solc-legacy200", "solc-ir200")
        )
        if not (
            all(
                "steps" in o.get(k, {}) for k in ("solar", "solc-legacy", "solc-via-ir")
            )
            and s
            and legacy
            and ir
        ):
            continue
        steps = o["solar"]["steps"] / min(
            o["solc-legacy"]["steps"], o["solc-via-ir"]["steps"]
        )
        cycles = s["interp_cycles"] / min(legacy["interp_cycles"], ir["interp_cycles"])
        rows.append((label, 100 * (1 - steps), 100 * (1 - cycles)))
    rows.sort(key=lambda r: r[1])

    def gm(i):
        return 100 * (
            1 - math.exp(sum(math.log(1 - r[i] / 100) for r in rows) / len(rows))
        )

    h = 8.4
    fig = figure(
        "Fewer Opcodes, Faster Execution",
        "Per call vs. the better solc pipeline",
        [("-Ogas vs. legacy or via-IR; evm2 interpreter", "mono"), SRC],
        height=h,
    )
    legend(
        fig,
        [
            (f"Opcodes executed, {gm(1):.0f}% fewer", GREEN),
            (f"Interpreter cycles, {gm(2):.0f}% fewer", MID),
        ],
        height=h,
        x=0.56,
    )
    ax = fig.add_axes([0.33, 0.11, 0.6, 0.72])
    w = 0.38
    for y, (_, steps, cycles) in enumerate(rows):
        for dy, v, color in ((w / 2, steps, GREEN), (-w / 2, cycles, MID)):
            ax.barh(y + dy, v, w, color=color)
            ax.text(
                v + 0.6,
                y + dy,
                f"{v:.0f}%",
                va="center",
                fontsize=9.5,
                color=INK,
                fontfamily=MONO["family"],
            )
    ax.set_yticks(range(len(rows)), [r[0] for r in rows])
    ax.set_xlim(0, 55)
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:.0f}%"))
    style_axes(ax, xgrid=True)
    for label in ax.get_yticklabels():
        label.set_fontfamily(SERIF["family"])
        label.set_fontsize(13)
    save(fig, "execution.png")


if __name__ == "__main__":
    load_fonts()
    morpho()
    projects()
    compare()
    gas_workloads()
    factorial()
    safe_solady()
    pipeline()
    execution()
