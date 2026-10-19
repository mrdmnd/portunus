"""Render a narrated 3Blue1Brown-style video of the Portunus Burn RL loop."""

from __future__ import annotations

import math
import os
import shutil
import subprocess
import tempfile
from collections.abc import Callable
from concurrent.futures import ProcessPoolExecutor
from dataclasses import dataclass
from pathlib import Path

os.environ.setdefault("MPLCONFIGDIR", "/tmp/mpl_cache_portunus")

import matplotlib

matplotlib.use("Agg")
import matplotlib.patheffects as pe
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.axes import Axes
from matplotlib.figure import Figure
from matplotlib.patches import Circle, FancyArrowPatch, FancyBboxPatch, Polygon

# ---------------------------------------------------------------------------
# 3Blue1Brown / Manim Color Palette
# ---------------------------------------------------------------------------
BG_DARK = "#0c1017"
BG_PANEL = "#121924"
BG_CODE = "#182232"
GRID_LINE = "#182334"

BLUE_C = "#58C4DD"
BLUE_D = "#29ABCA"
BLUE_E = "#1C758A"
YELLOW_C = "#FFFF00"
GOLD_C = "#F4D03F"
GREEN_C = "#83C167"
GREEN_E = "#49A88F"
RED_C = "#FC6255"
MAROON_C = "#C55F73"
PURPLE_C = "#B189C6"
TEAL_C = "#5CD0B3"
ORANGE_C = "#FF862F"
WHITE = "#F8FAFC"
GREY_B = "#94A3B8"
GREY_C = "#64748B"

plt.rcParams["mathtext.fontset"] = "cm"
plt.rcParams["font.sans-serif"] = ["DejaVu Sans", "Arial", "Helvetica"]
plt.rcParams["font.monospace"] = ["DejaVu Sans Mono", "Menlo", "Courier New"]


# ---------------------------------------------------------------------------
# 5-Act 3Blue1Brown Narrative Voiceover Script
# ---------------------------------------------------------------------------
@dataclass(frozen=True)
class ActNarration:
    """Metadata and spoken voiceover script for one act of the video."""

    act_roman: str
    title: str
    still_filename: str
    narration: str


VOICEOVER_ACTS: tuple[ActNarration, ...] = (
    ActNarration(
        act_roman="ACT I",
        title="How Portunus Fits Together: Self-Play RL in Pure Rust + Burn",
        still_filename="act1_architecture.png",
        narration=(
            "How can an agent learn a complex World of Warcraft damage rotation completely from scratch, "
            "without reading a human guide? "
            "In Portunus, six Rust crates form a closed AlphaZero-style reinforcement learning loop, "
            "orchestrated by train R L in crates, run, source, train dot R S. "
            "First, Sim Env wraps a deterministic, cloneable discrete-event combat kernel. "
            "Second, Seat Obs Featurizer encodes the player's realistic observation into 132 floats, "
            "while Spec Action Space maps legal spells into 35 discrete actions. "
            "Third, Monte Carlo Tree Search forks the combat world 24 times per decision to discover high-damage combos. "
            "Fourth, Self Play Collector records those search trajectories. "
            "Fifth, Burn Trainer updates our two-headed Policy Value Network using Autodiff and Adam W. "
            "And finally, Burn Distiller compresses the trained network into a standalone Lua addon."
        ),
    ),
    ActNarration(
        act_roman="ACT II",
        title="From Combat State to Burn Tensors: SeatObs -> PolicyValueNet",
        still_filename="act2_burn_network.png",
        narration=(
            "Let's zoom in on a single decision point for an Elemental Shaman using the Stormbringer hero tree. "
            "In crates, run, source, observe dot R S, Seat Obs Featurizer reads only what a real player can see: "
            "65 Maelstrom, an open global cooldown, 4.2 seconds left on Flame Shock, two stacks of Stormkeeper, "
            "and Voltaic Blaze ready to cast. "
            "This 132-dimensional vector flows into Policy Value Net, defined in crates, policy, source, lib dot R S. "
            "Built on Burn 0.22's Flex backend, the network splits into two Multi-Layer Perceptron towers. "
            "The Policy Tower outputs logits across all 35 actions, where illegal or on-cooldown spells like Ascendance "
            "are masked to negative infinity before softmax. "
            "Meanwhile, the Value Tower predicts a single scalar: the expected discounted damage over the next six seconds."
        ),
    ),
    ActNarration(
        act_roman="ACT III",
        title="Lookahead Tree Search: PUCT MCTS Over Cloned Combat Worlds",
        still_filename="act3_puct_mcts.png",
        narration=(
            "Why doesn't the network just act on its initial guesses? Because lookahead search acts as a policy amplifier. "
            "In M C T S search, at line 632 of crates, policy, source, lib dot R S, "
            "we clone the entire combat simulation 24 times over a six-second lookahead horizon. "
            "Because raw damage numbers vary wildly across specs, we track the minimum and maximum Q values across the tree "
            "to normalize every branch into the unit interval, zero to one, and we scale truncated rollouts by horizon over elapsed time "
            "so fast instants and slow casts are compared by true D P S rate. "
            "Look at the right branch: when M C T S tests Voltaic Blaze, it sees that it applies Flame Shock immediately, "
            "turning the next Lava Burst into a guaranteed 100 percent critical strike. "
            "Q-sharpening concentrates 76 percent of the target probability onto Voltaic Blaze."
        ),
    ),
    ActNarration(
        act_roman="ACT IV",
        title="Hillclimbing Single-Target DPS: Elemental Shaman [Stormbringer]",
        still_filename="act4_dps_hillclimb.png",
        narration=(
            "Now watch what happens when we close the loop and hillclimb across six iterations. "
            "At Iteration 0, the untrained Burn network is completely clueless: it spams Frost Shock 419 times, "
            "dealing just 1,558 D P S, or 4.3 percent of the hand-written SimulationCraft priority list. "
            "In crates, learn, source, lib dot R S, Burn Trainer minimizes masked cross-entropy against the M C T S targets "
            "and mean squared error against the discounted returns. "
            "After just 107 decisions in Iteration 1, the network leaps to 39,944 D P S, beating the human script. "
            "By Iteration 6, policy loss drops from 1.25 to 0.60, and the pure neural network—running greedily at temperature zero "
            "with no tree search at test time—hits 45,797 D P S. "
            "That is 27.7 percent higher than the baseline script, because RL discovered that casting Voltaic Blaze on cooldown "
            "unlocks massive Flame Shock and Lava Burst synergy while pruning Frost Shock to zero."
        ),
    ),
    ActNarration(
        act_roman="ACT V",
        title="Knowledge Distillation & Zero-Dependency Export to model.lua",
        still_filename="act5_lua_distillation.png",
        narration=(
            "Finally, in crates, export, source, lib dot R S, we prepare the trained policy for the game client. "
            "Burn Distiller trains a compact, single-tower Student Net with just 32 hidden units to imitate the teacher across held-out combat states, "
            "achieving 99.1 percent agreement on Stormbringer and 100 percent on Arms Warrior. "
            "Lua Addon Emitter then serializes the Burn module weights directly into Portunus Rotation, model dot lua: "
            "a 57-kilobyte, zero-dependency Lua table that executes a pure ReLU forward pass in micro-seconds."
        ),
    ),
)


def smoothstep(edge0: float, edge1: float, x: float) -> float:
    """Hermite smoothstep interpolation in [0, 1]."""
    if edge1 <= edge0:
        return 1.0 if x >= edge0 else 0.0
    t = max(0.0, min(1.0, (x - edge0) / (edge1 - edge0)))
    return t * t * (3.0 - 2.0 * t)


def ease_out_cubic(t: float) -> float:
    """Cubic ease-out on t in [0, 1]."""
    c = max(0.0, min(1.0, t))
    return 1.0 - (1.0 - c) ** 3


def format_mmss(seconds: float) -> str:
    """Format seconds as MM:SS."""
    total = max(0, round(seconds))
    mins = total // 60
    secs = total % 60
    return f"{mins:02d}:{secs:02d}"


@dataclass(frozen=True)
class CodePointer:
    """A 3b1b-style floating file/line pointer badge with leader arrow."""

    file_ref: str
    symbol: str
    bx: float
    by: float
    tx: float
    ty: float
    color: str = GOLD_C
    rad: float = 0.18


def setup_canvas(fig: Figure) -> Axes:
    """Create a 16:9 coordinate canvas [0..16] x [0..9] with a subtle 3b1b grid."""
    ax: Axes = fig.add_axes((0.0, 0.0, 1.0, 1.0))
    ax.set_xlim(0.0, 16.0)
    ax.set_ylim(0.0, 9.0)
    ax.set_facecolor(BG_DARK)
    ax.axis("off")

    for x in np.arange(0.0, 16.5, 0.5):
        lw = 0.6 if math.isclose(float(x) % 2.0, 0.0) else 0.25
        alpha = 0.32 if math.isclose(float(x) % 2.0, 0.0) else 0.14
        ax.axvline(float(x), color=GRID_LINE, lw=lw, alpha=alpha, zorder=0)
    for y in np.arange(0.0, 9.5, 0.5):
        lw = 0.6 if math.isclose(float(y) % 2.0, 0.0) else 0.25
        alpha = 0.32 if math.isclose(float(y) % 2.0, 0.0) else 0.14
        ax.axhline(float(y), color=GRID_LINE, lw=lw, alpha=alpha, zorder=0)
    return ax


def draw_header(
    ax: Axes,
    act_label: str,
    title: str,
    subtitle: str,
    progress: float,
) -> None:
    """Draw top title bar and bottom 3b1b timeline progress bar."""
    ax.text(
        0.55,
        8.52,
        act_label,
        color=GOLD_C,
        fontsize=11,
        fontweight="bold",
        family="monospace",
        zorder=10,
    )
    ax.text(
        2.35,
        8.50,
        title,
        color=WHITE,
        fontsize=17,
        fontweight="bold",
        zorder=10,
        path_effects=[pe.withStroke(linewidth=3, foreground=BG_DARK)],
    )
    ax.text(
        0.55,
        8.12,
        subtitle,
        color=GREY_B,
        fontsize=10.5,
        zorder=10,
    )
    ax.plot([0.5, 15.5], [7.92, 7.92], color=BLUE_E, lw=1.2, alpha=0.6, zorder=5)

    ax.plot(
        [0.5, 15.5],
        [0.22, 0.22],
        color="#1e293b",
        lw=4.0,
        solid_capstyle="round",
        zorder=8,
    )
    ax.plot(
        [0.5, 0.5 + 15.0 * max(0.01, min(1.0, progress))],
        [0.22, 0.22],
        color=BLUE_C,
        lw=4.0,
        solid_capstyle="round",
        zorder=9,
    )


def draw_box(
    ax: Axes,
    x: float,
    y: float,
    w: float,
    h: float,
    border_color: str,
    fill_color: str = BG_PANEL,
    alpha: float = 1.0,
    lw: float = 1.8,
    radius: float = 0.14,
    zorder: float = 1.2,
) -> None:
    """Draw a rounded Manim-style box with subtle outer glow."""
    if alpha <= 0.01:
        return
    glow = FancyBboxPatch(
        (x, y),
        w,
        h,
        boxstyle=f"round,pad=0.02,rounding_size={radius}",
        facecolor="none",
        edgecolor=border_color,
        linewidth=lw + 2.5,
        alpha=0.18 * alpha,
        zorder=zorder - 0.2,
    )
    ax.add_patch(glow)
    box = FancyBboxPatch(
        (x, y),
        w,
        h,
        boxstyle=f"round,pad=0.02,rounding_size={radius}",
        facecolor=fill_color,
        edgecolor=border_color,
        linewidth=lw,
        alpha=alpha,
        zorder=zorder,
    )
    ax.add_patch(box)


def draw_code_pointer(ax: Axes, ptr: CodePointer, alpha: float) -> None:
    """Draw a code file + line callout badge with curved arrow pointer."""
    if alpha <= 0.01:
        return
    w = max(2.55, 0.088 * max(len(ptr.file_ref), len(ptr.symbol)) + 0.35)
    h = 0.62
    x0 = ptr.bx - w / 2.0
    y0 = ptr.by - h / 2.0

    draw_box(
        ax,
        x0,
        y0,
        w,
        h,
        border_color=ptr.color,
        fill_color=BG_CODE,
        alpha=0.95 * alpha,
        lw=1.4,
        radius=0.10,
        zorder=14.0,
    )
    ax.text(
        ptr.bx,
        ptr.by + 0.11,
        ptr.file_ref,
        color=ptr.color,
        fontsize=8.3,
        fontweight="bold",
        family="monospace",
        ha="center",
        va="center",
        alpha=alpha,
        zorder=15,
    )
    ax.text(
        ptr.bx,
        ptr.by - 0.13,
        ptr.symbol,
        color=WHITE,
        fontsize=8.0,
        family="monospace",
        ha="center",
        va="center",
        alpha=0.92 * alpha,
        zorder=15,
    )

    arrow = FancyArrowPatch(
        (ptr.bx, ptr.by - h / 2.0 if ptr.ty < ptr.by else ptr.by + h / 2.0),
        (ptr.tx, ptr.ty),
        connectionstyle=f"arc3,rad={ptr.rad}",
        arrowstyle="-|>,head_length=6,head_width=3.8",
        color=ptr.color,
        lw=1.35,
        linestyle="--",
        alpha=0.88 * alpha,
        zorder=13,
    )
    ax.add_patch(arrow)
    dot = Circle((ptr.tx, ptr.ty), 0.055, color=ptr.color, alpha=alpha, zorder=15)
    ax.add_patch(dot)


def draw_curved_arrow(
    ax: Axes,
    p0: tuple[float, float],
    p1: tuple[float, float],
    color: str,
    rad: float = 0.0,
    alpha: float = 1.0,
    lw: float = 2.0,
    pulse_pos: float | None = None,
) -> None:
    """Draw a Manim-style arrow and optional travelling signal pulse."""
    if alpha <= 0.01:
        return
    arr = FancyArrowPatch(
        p0,
        p1,
        connectionstyle=f"arc3,rad={rad}",
        arrowstyle="-|>,head_length=7,head_width=4.5",
        color=color,
        lw=lw,
        alpha=alpha,
        zorder=6,
    )
    ax.add_patch(arr)
    if pulse_pos is not None:
        u = pulse_pos % 1.0
        mx = (1.0 - u) * p0[0] + u * p1[0]
        my = (1.0 - u) * p0[1] + u * p1[1]
        dx = p1[0] - p0[0]
        dy = p1[1] - p0[1]
        nx, ny = -dy, dx
        mx += nx * rad * 0.45 * 4.0 * u * (1.0 - u)
        my += ny * rad * 0.45 * 4.0 * u * (1.0 - u)
        ax.add_patch(Circle((mx, my), 0.11, color=color, alpha=0.35 * alpha, zorder=11))
        ax.add_patch(Circle((mx, my), 0.06, color=WHITE, alpha=0.95 * alpha, zorder=12))


# ---------------------------------------------------------------------------
# ACT I: Full Architecture & Code Map (0.0 .. 1.0 local time)
# ---------------------------------------------------------------------------
def render_act1_architecture(
    ax: Axes,
    t: float,
    global_progress: float,
    ts_label: str,
) -> None:
    """Render Act I: closed-loop crate architecture and code pointers."""
    draw_header(
        ax,
        f"ACT I  [{ts_label}]",
        "How Portunus Fits Together: Self-Play RL in Pure Rust + Burn",
        "Six workspace crates form a closed AlphaZero-style loop orchestrated by train_rl()",
        global_progress,
    )

    nodes = [
        (
            "1. SimEnv & Kernel",
            "Deterministic Discrete-Event Sim\nCloneable State + Counter RNG",
            2.3,
            5.55,
            BLUE_C,
        ),
        (
            "2. Featurizer & Space",
            "SeatObs (Realistic InfoSet)\nx_t in R^132  |  ActionMask -> 35",
            6.5,
            5.55,
            TEAL_C,
        ),
        (
            "3. PUCT MCTS Search",
            "24 Rollouts over 6.0s Horizon\nMin-Max Q + Rate Normalization",
            10.8,
            5.55,
            GOLD_C,
        ),
        (
            "4. Replay Trajectories",
            "SelfPlayCollector (gamma = 0.85)\n(x_t, mask_t, pi_MCTS, G_t)",
            10.8,
            2.25,
            ORANGE_C,
        ),
        (
            "5. Burn PolicyValueNet",
            "Autodiff<Flex> + AdamW\nPolicy CE Loss + Value MSE Loss",
            6.5,
            2.25,
            PURPLE_C,
        ),
        (
            "6. Distill & Lua Addon",
            "BurnDistiller -> StudentNet (32u)\n99.1% Match -> model.lua",
            2.3,
            2.25,
            GREEN_C,
        ),
    ]

    w, h = 3.1, 1.35
    for idx, (title, body, cx, cy, col) in enumerate(nodes):
        a = smoothstep(idx * 0.08, idx * 0.08 + 0.18, t)
        draw_box(ax, cx - w / 2, cy - h / 2, w, h, border_color=col, alpha=a, lw=2.1)
        ax.text(
            cx,
            cy + 0.34,
            title,
            color=col,
            fontsize=11.5,
            fontweight="bold",
            ha="center",
            va="center",
            alpha=a,
            zorder=8,
        )
        ax.text(
            cx,
            cy - 0.18,
            body,
            color=WHITE,
            fontsize=8.7,
            ha="center",
            va="center",
            alpha=0.9 * a,
            zorder=8,
        )

    edges = [
        ((3.88, 5.55), (4.92, 5.55), BLUE_C, 0.0),
        ((8.08, 5.55), (9.22, 5.55), TEAL_C, 0.0),
        ((10.8, 4.85), (10.8, 2.95), GOLD_C, 0.0),
        ((9.22, 2.25), (8.08, 2.25), ORANGE_C, 0.0),
        ((6.5, 2.95), (9.4, 4.88), PURPLE_C, -0.15),
        ((6.5, 2.95), (2.3, 4.85), PURPLE_C, 0.15),
        ((4.92, 2.25), (3.88, 2.25), GREEN_C, 0.0),
    ]
    for i, (p0, p1, col, rad) in enumerate(edges):
        ea = smoothstep(0.18 + i * 0.06, 0.35 + i * 0.06, t)
        draw_curved_arrow(
            ax,
            p0,
            p1,
            col,
            rad=rad,
            alpha=ea,
            lw=2.1,
            pulse_pos=(t * 4.0 + i * 0.17),
        )

    ax.text(
        8.35,
        3.95,
        r"$\theta_k \longrightarrow \mathrm{Priors}\;P_\theta(s,a),\;V_\theta(s)$",
        color=PURPLE_C,
        fontsize=11,
        ha="center",
        va="center",
        alpha=smoothstep(0.35, 0.55, t),
        zorder=10,
    )

    orc_a = smoothstep(0.25, 0.45, t)
    draw_box(
        ax,
        12.75,
        1.55,
        2.8,
        4.75,
        border_color=GOLD_C,
        fill_color="#111823",
        alpha=orc_a,
    )
    ax.text(
        14.15,
        5.92,
        "RL Loop Driver",
        color=GOLD_C,
        fontsize=11.5,
        fontweight="bold",
        ha="center",
        alpha=orc_a,
    )
    ax.text(
        14.15,
        5.52,
        "crates/run/src/train.rs:149",
        color=BLUE_C,
        fontsize=8.2,
        family="monospace",
        ha="center",
        alpha=orc_a,
    )
    steps_txt = (
        "pub fn train_rl(\n"
        "  bundle: &Bundle,\n"
        "  runner: &LocalRunner,\n"
        "  spec: TrainSpec,\n"
        ") -> Result<TrainReport>\n\n"
        "1. Eval SimC script APL\n"
        "2. Init PolicyValueNet\n"
        "3. Eval Iter 0 (untrained)\n"
        "4. For iter in 1..=N:\n"
        "   - MCTS self-play\n"
        "   - BurnTrainer AdamW\n"
        "   - Eval ModelPolicy\n"
        "5. BurnDistiller -> Lua"
    )
    ax.text(
        12.95,
        3.62,
        steps_txt,
        color=WHITE,
        fontsize=8.0,
        family="monospace",
        va="center",
        alpha=0.92 * orc_a,
    )

    pointers = [
        CodePointer(
            "crates/env/src/reward.rs:36",
            "pub struct DenseReward",
            2.3,
            7.12,
            2.3,
            6.25,
            BLUE_C,
            0.12,
        ),
        CodePointer(
            "crates/run/src/observe.rs:676",
            "SeatObsFeaturizer::featurize",
            6.5,
            7.12,
            6.5,
            6.25,
            TEAL_C,
            -0.10,
        ),
        CodePointer(
            "crates/policy/src/lib.rs:632",
            "Mcts::search (PUCT + MinMax Q)",
            10.8,
            7.12,
            10.8,
            6.25,
            GOLD_C,
            0.12,
        ),
        CodePointer(
            "crates/learn/src/lib.rs:125",
            "SelfPlayCollector::collect",
            10.8,
            0.82,
            10.8,
            1.55,
            ORANGE_C,
            -0.12,
        ),
        CodePointer(
            "crates/learn/src/lib.rs:260",
            "BurnTrainer::train (Autodiff)",
            6.5,
            0.82,
            6.5,
            1.55,
            PURPLE_C,
            0.10,
        ),
        CodePointer(
            "crates/export/src/lib.rs:157",
            "BurnDistiller::distill -> Lua",
            2.3,
            0.82,
            2.3,
            1.55,
            GREEN_C,
            -0.12,
        ),
    ]
    for idx, ptr in enumerate(pointers):
        pa = smoothstep(0.30 + idx * 0.07, 0.48 + idx * 0.07, t)
        draw_code_pointer(ax, ptr, pa)


# ---------------------------------------------------------------------------
# ACT II: State Featurization & Two-Headed Burn Network
# ---------------------------------------------------------------------------
def render_act2_network(
    ax: Axes,
    t: float,
    global_progress: float,
    ts_label: str,
) -> None:
    """Render Act II: SeatObs featurization and Burn PolicyValueNet forward pass."""
    draw_header(
        ax,
        f"ACT II [{ts_label}]",
        "From Combat State to Burn Tensors: SeatObs -> PolicyValueNet",
        "Realistic player observation (132 floats) feeds a two-headed Burn 0.22 MLP on the Flex backend",
        global_progress,
    )

    a_obs = smoothstep(0.0, 0.20, t)
    draw_box(ax, 0.6, 1.65, 3.6, 5.2, border_color=BLUE_C, alpha=a_obs)
    ax.text(
        2.4,
        6.50,
        "SeatObs (Realistic InfoSet)",
        color=BLUE_C,
        fontsize=11,
        fontweight="bold",
        ha="center",
        alpha=a_obs,
    )

    obs_rows = [
        ("Maelstrom", "65 / 100", 0.65, BLUE_C),
        ("GCD Remaining", "0.00 s (Ready)", 1.0, GREEN_C),
        ("Flame Shock DoT", "4.2 s (< Pandemic)", 0.35, ORANGE_C),
        ("Stormkeeper Buff", "2 stacks (11.4 s)", 0.85, GOLD_C),
        ("Voltaic Blaze CD", "0.0 s (READY)", 1.0, GREEN_C),
        ("Lava Burst Charges", "1.92 / 2.0", 0.96, RED_C),
        ("Target HP / Dist", "74.2%  |  28.0 yd", 0.74, TEAL_C),
    ]
    for i, (label, val_str, frac, col) in enumerate(obs_rows):
        ry = 5.92 - i * 0.62
        ra = smoothstep(0.05 + i * 0.04, 0.22 + i * 0.04, t)
        ax.text(0.82, ry + 0.12, label, color=GREY_B, fontsize=8.5, alpha=ra)
        ax.text(
            3.98,
            ry + 0.12,
            val_str,
            color=WHITE,
            fontsize=8.3,
            family="monospace",
            ha="right",
            alpha=ra,
        )
        ax.plot(
            [0.82, 3.98],
            [ry - 0.08, ry - 0.08],
            color="#1e293b",
            lw=5.0,
            solid_capstyle="round",
            alpha=ra,
        )
        bar_len = (3.98 - 0.82) * frac * ease_out_cubic(ra)
        ax.plot(
            [0.82, 0.82 + max(0.02, bar_len)],
            [ry - 0.08, ry - 0.08],
            color=col,
            lw=5.0,
            solid_capstyle="round",
            alpha=ra,
        )

    net_a = smoothstep(0.18, 0.42, t)
    layer_x = [5.3, 7.2, 9.0, 10.9]
    input_y = np.linspace(2.2, 6.3, 9)
    hid1_y = np.linspace(2.4, 6.1, 7)
    pol_y = np.linspace(4.55, 6.45, 5)
    val_y = np.linspace(2.05, 3.65, 4)
    out_pol_y = np.linspace(4.45, 6.55, 6)
    out_val_y = [2.85]

    for iy in input_y:
        for hy in hid1_y:
            ax.plot(
                [layer_x[0], layer_x[1]],
                [float(iy), float(hy)],
                color=BLUE_D,
                lw=0.55,
                alpha=0.22 * net_a,
                zorder=3,
            )
    for hy in hid1_y:
        for py in pol_y:
            ax.plot(
                [layer_x[1], layer_x[2]],
                [float(hy), float(py)],
                color=PURPLE_C,
                lw=0.65,
                alpha=0.25 * net_a,
                zorder=3,
            )
        for vy in val_y:
            ax.plot(
                [layer_x[1], layer_x[2]],
                [float(hy), float(vy)],
                color=TEAL_C,
                lw=0.65,
                alpha=0.25 * net_a,
                zorder=3,
            )
    for py in pol_y:
        for oy in out_pol_y:
            ax.plot(
                [layer_x[2], layer_x[3]],
                [float(py), float(oy)],
                color=GOLD_C,
                lw=0.75,
                alpha=0.32 * net_a,
                zorder=3,
            )
    for vy in val_y:
        ax.plot(
            [layer_x[2], layer_x[3]],
            [float(vy), out_val_y[0]],
            color=GREEN_C,
            lw=0.9,
            alpha=0.42 * net_a,
            zorder=3,
        )

    wave_x = 5.0 + (t * 14.0) % 7.0
    for x_pos, ys, col in [
        (layer_x[0], input_y, BLUE_C),
        (layer_x[1], hid1_y, PURPLE_C),
        (layer_x[2], pol_y, PURPLE_C),
        (layer_x[2], val_y, TEAL_C),
        (layer_x[3], out_pol_y, GOLD_C),
        (layer_x[3], np.array(out_val_y), GREEN_C),
    ]:
        glow_boost = math.exp(-((x_pos - wave_x) ** 2) / 0.65)
        for y_pos in ys:
            rad_c = 0.12 + 0.035 * glow_boost
            ax.add_patch(
                Circle(
                    (x_pos, float(y_pos)),
                    rad_c + 0.05,
                    color=col,
                    alpha=(0.25 + 0.45 * glow_boost) * net_a,
                    zorder=6,
                )
            )
            ax.add_patch(
                Circle(
                    (x_pos, float(y_pos)),
                    rad_c,
                    facecolor=BG_PANEL if glow_boost < 0.4 else col,
                    edgecolor=col,
                    lw=1.6,
                    alpha=net_a,
                    zorder=7,
                )
            )

    ax.text(
        layer_x[0],
        6.72,
        r"$\mathbf{x}_t \in \mathbb{R}^{132}$",
        color=BLUE_C,
        fontsize=11,
        ha="center",
        alpha=net_a,
    )
    ax.text(
        layer_x[1],
        6.50,
        "Shared/Tower\nLinear(132, 64)\n+ ReLU",
        color=PURPLE_C,
        fontsize=8.2,
        ha="center",
        alpha=net_a,
    )
    ax.text(
        layer_x[2],
        6.75,
        "Policy Tower\nLinear(64, 64)",
        color=PURPLE_C,
        fontsize=8.2,
        ha="center",
        alpha=net_a,
    )
    ax.text(
        layer_x[2],
        1.62,
        "Value Tower\nLinear(64, 64)",
        color=TEAL_C,
        fontsize=8.2,
        ha="center",
        alpha=net_a,
    )

    out_a = smoothstep(0.35, 0.60, t)
    draw_box(ax, 11.75, 3.85, 3.65, 3.05, border_color=GOLD_C, alpha=out_a)
    ax.text(
        13.58,
        6.58,
        r"Legal-Masked Priors $P_\theta(s_t, a)$",
        color=GOLD_C,
        fontsize=10.5,
        fontweight="bold",
        ha="center",
        alpha=out_a,
    )
    priors = [
        ("Voltaic Blaze", 0.44, GREEN_C, True),
        ("Lava Burst", 0.26, GOLD_C, True),
        ("Lightning Bolt", 0.18, BLUE_C, True),
        ("Earth Shock", 0.09, PURPLE_C, True),
        ("Frost Shock", 0.03, RED_C, True),
        ("Ascendance (On CD)", 0.00, GREY_C, False),
    ]
    for i, (sp_name, prob, col, legal) in enumerate(priors):
        row_y = 6.12 - i * 0.36
        ax.text(
            11.92,
            row_y,
            sp_name,
            color=WHITE if legal else GREY_C,
            fontsize=8.2,
            ha="left",
            va="center",
            alpha=out_a,
        )
        bar_w = 1.65 * prob * ease_out_cubic(out_a)
        ax.plot(
            [13.55, 15.20],
            [row_y, row_y],
            color="#1e293b",
            lw=6.0,
            solid_capstyle="round",
            alpha=out_a,
        )
        if legal and prob > 0:
            ax.plot(
                [13.55, 13.55 + max(0.03, bar_w)],
                [row_y, row_y],
                color=col,
                lw=6.0,
                solid_capstyle="round",
                alpha=out_a,
            )
            ax.text(
                15.26,
                row_y,
                f"{int(prob * 100)}%",
                color=col,
                fontsize=7.8,
                family="monospace",
                va="center",
                alpha=out_a,
            )
        else:
            ax.text(
                14.35,
                row_y,
                "MASKED (-inf)",
                color=RED_C,
                fontsize=7.2,
                family="monospace",
                ha="center",
                va="center",
                alpha=out_a,
            )

    draw_box(ax, 11.75, 1.65, 3.65, 1.85, border_color=GREEN_C, alpha=out_a)
    ax.text(
        13.58,
        3.18,
        r"Scalar Value Head $v_\theta(s_t)$",
        color=GREEN_C,
        fontsize=10.5,
        fontweight="bold",
        ha="center",
        alpha=out_a,
    )
    ax.text(
        13.58,
        2.52,
        r"$v_\theta(s_t) = +184.6\;\mathrm{reward\;rate}$",
        color=WHITE,
        fontsize=11.5,
        ha="center",
        alpha=out_a,
    )
    ax.text(
        13.58,
        1.98,
        "Expected discounted 6s damage return",
        color=GREY_B,
        fontsize=8.3,
        ha="center",
        alpha=out_a,
    )

    ptrs = [
        CodePointer(
            "crates/run/src/observe.rs:707",
            "SeatObsFeaturizer::featurize(&obs)",
            2.4,
            0.92,
            2.4,
            1.65,
            BLUE_C,
            -0.10,
        ),
        CodePointer(
            "crates/policy/src/lib.rs:206",
            "#[derive(Module)] PolicyValueNet",
            8.1,
            7.35,
            8.1,
            6.55,
            PURPLE_C,
            0.10,
        ),
        CodePointer(
            "crates/env/src/action_space.rs:59",
            "SpecActionSpace::legal(&mask)",
            8.1,
            0.92,
            11.75,
            4.35,
            GOLD_C,
            0.18,
        ),
    ]
    for i, p in enumerate(ptrs):
        draw_code_pointer(ax, p, smoothstep(0.32 + i * 0.10, 0.52 + i * 0.10, t))


# ---------------------------------------------------------------------------
# ACT III: PUCT MCTS Search & Horizon Rate Normalization
# ---------------------------------------------------------------------------
def render_act3_mcts(
    ax: Axes,
    t: float,
    global_progress: float,
    ts_label: str,
) -> None:
    """Render Act III: PUCT MCTS search tree and Q-sharpened policy targets."""
    draw_header(
        ax,
        f"ACT III [{ts_label}]",
        "Lookahead Tree Search: PUCT MCTS Over Cloned Combat Worlds",
        "Mcts::search forks SimEnv 24 times, normalizes Q in [0,1], and scales returns by DPS rate (H / dt)",
        global_progress,
    )

    eq_a = smoothstep(0.0, 0.22, t)
    draw_box(ax, 0.55, 5.45, 7.35, 2.20, border_color=GOLD_C, alpha=eq_a)
    ax.text(
        4.22,
        7.28,
        r"$\mathrm{PUCT\;Selection\;with\;Min-Max\;Normalized\;} \tilde{Q}(s,a) \in [0, 1]:$",
        color=GOLD_C,
        fontsize=11.0,
        ha="center",
        alpha=eq_a,
    )
    ax.text(
        4.22,
        6.55,
        r"$a^* = \arg\max_{a} \left[ \tilde{Q}(s,a) + c_{\mathrm{puct}}\,P_\theta(s,a)\,\frac{\sqrt{\sum_b N(s,b)}}{1 + N(s,a)} \right]$",
        color=WHITE,
        fontsize=13.2,
        ha="center",
        alpha=eq_a,
    )
    ax.text(
        4.22,
        5.78,
        r"$\tilde{Q}(s,a) = \frac{Q(s,a) - Q_{\min}}{Q_{\max} - Q_{\min}}, \qquad R_{\mathrm{horizon}} = R_{\mathrm{raw}} \cdot \frac{H}{\Delta t_{\mathrm{elapsed}}}$",
        color=BLUE_C,
        fontsize=10.8,
        ha="center",
        alpha=eq_a,
    )

    tree_a = smoothstep(0.12, 0.35, t)
    root_pt = (4.2, 4.45)
    children = [
        ("Frost Shock", (1.45, 2.75), RED_C, 1, 0.08),
        ("Flame Shock", (3.25, 2.75), ORANGE_C, 2, 0.38),
        ("Lightning Bolt", (5.15, 2.75), BLUE_C, 5, 0.67),
        ("Voltaic Blaze", (7.15, 2.75), GREEN_C, 16, 1.00),
    ]

    sim_count = min(24, max(1, round(t * 28)))
    ax.text(
        1.55,
        4.65,
        f"Simulations: {sim_count} / 24\nenv.clone() forks",
        color=BLUE_C,
        fontsize=9.5,
        family="monospace",
        alpha=tree_a,
    )

    for idx, (label, cpt, col, max_n, qn) in enumerate(children):
        cur_n = max(1, round(max_n * ease_out_cubic(smoothstep(0.15, 0.85, t))))
        lw = 1.2 + 0.25 * cur_n
        draw_curved_arrow(
            ax,
            (root_pt[0], root_pt[1] - 0.28),
            (cpt[0], cpt[1] + 0.36),
            col,
            rad=0.08 * (idx - 1.5),
            alpha=tree_a,
            lw=lw,
            pulse_pos=(t * 5.0 + idx * 0.23) if idx >= 2 else None,
        )
        draw_box(
            ax,
            cpt[0] - 0.78,
            cpt[1] - 0.36,
            1.56,
            0.72,
            border_color=col,
            alpha=tree_a,
            lw=2.4 if idx == 3 else 1.5,
        )
        ax.text(
            cpt[0],
            cpt[1] + 0.12,
            label,
            color=col,
            fontsize=8.8,
            fontweight="bold",
            ha="center",
            alpha=tree_a,
        )
        ax.text(
            cpt[0],
            cpt[1] - 0.14,
            f"N={cur_n} | Q~={qn:.2f}",
            color=WHITE,
            fontsize=7.6,
            family="monospace",
            ha="center",
            alpha=tree_a,
        )

    gc_a = smoothstep(0.32, 0.58, t)
    grandchildren = [
        ("Lava Burst\n(100% Crit!)", (5.85, 1.25), GOLD_C),
        ("Tempest +\nOverload", (7.55, 1.25), GREEN_C),
    ]
    for g_label, gpt, gcol in grandchildren:
        draw_curved_arrow(
            ax,
            (7.15, 2.38),
            (gpt[0], gpt[1] + 0.35),
            gcol,
            rad=0.05,
            alpha=gc_a,
            lw=2.0,
            pulse_pos=t * 4.0,
        )
        draw_box(
            ax, gpt[0] - 0.72, gpt[1] - 0.34, 1.44, 0.68, border_color=gcol, alpha=gc_a
        )
        ax.text(
            gpt[0],
            gpt[1],
            g_label,
            color=WHITE,
            fontsize=8.0,
            ha="center",
            va="center",
            alpha=gc_a,
        )

    draw_box(
        ax,
        root_pt[0] - 1.05,
        root_pt[1] - 0.30,
        2.10,
        0.62,
        border_color=BLUE_C,
        alpha=tree_a,
        lw=2.2,
    )
    ax.text(
        root_pt[0],
        root_pt[1],
        "Root State s_t\n(SimEnv Clone)",
        color=WHITE,
        fontsize=8.8,
        fontweight="bold",
        ha="center",
        va="center",
        alpha=tree_a,
    )

    pol_a = smoothstep(0.40, 0.68, t)
    draw_box(ax, 8.75, 2.15, 6.65, 5.50, border_color=GREEN_C, alpha=pol_a)
    ax.text(
        12.08,
        7.25,
        r"Q-Sharpened Policy Target $\pi_{\mathrm{MCTS}}(a \mid s_t)$",
        color=GREEN_C,
        fontsize=11.5,
        fontweight="bold",
        ha="center",
        alpha=pol_a,
    )
    ax.text(
        12.08,
        6.72,
        r"$\pi_{\mathrm{target}}(a) \propto N(s,a) \cdot \exp(2.5\,\tilde{Q}(s,a))$",
        color=GOLD_C,
        fontsize=11.5,
        ha="center",
        alpha=pol_a,
    )

    bars = [
        ("Voltaic Blaze", 0.25, 0.76, GREEN_C),
        ("Lightning Bolt", 0.25, 0.14, BLUE_C),
        ("Flame Shock", 0.25, 0.06, ORANGE_C),
        ("Earth Shock", 0.15, 0.03, PURPLE_C),
        ("Frost Shock", 0.10, 0.01, RED_C),
    ]
    for i, (name, prior_p, target_p, col) in enumerate(bars):
        by = 5.95 - i * 0.72
        ax.text(
            9.0,
            by + 0.12,
            name,
            color=WHITE,
            fontsize=9.0,
            fontweight="bold",
            alpha=pol_a,
        )
        ax.plot(
            [11.15, 11.15 + 3.4 * prior_p],
            [by + 0.14, by + 0.14],
            color=GREY_C,
            lw=5.0,
            solid_capstyle="round",
            alpha=0.65 * pol_a,
        )
        ax.text(
            11.25 + 3.4 * prior_p,
            by + 0.14,
            f"Prior {int(prior_p * 100)}%",
            color=GREY_B,
            fontsize=7.5,
            va="center",
            alpha=pol_a,
        )
        cur_t = target_p * ease_out_cubic(pol_a)
        ax.plot(
            [11.15, 11.15 + max(0.04, 3.4 * cur_t)],
            [by - 0.12, by - 0.12],
            color=col,
            lw=7.5,
            solid_capstyle="round",
            alpha=pol_a,
        )
        ax.text(
            11.25 + 3.4 * cur_t,
            by - 0.12,
            f"MCTS Target {int(target_p * 100)}%",
            color=col,
            fontsize=8.2,
            fontweight="bold",
            va="center",
            alpha=pol_a,
        )

    ptrs = [
        CodePointer(
            "crates/policy/src/lib.rs:647",
            "let norm_q = (q - min_q)/(max_q - min_q)",
            2.45,
            0.82,
            3.25,
            2.38,
            GOLD_C,
            -0.15,
        ),
        CodePointer(
            "crates/policy/src/lib.rs:765",
            "w = visits * (2.5 * nq).exp()",
            12.1,
            1.15,
            12.1,
            2.15,
            GREEN_C,
            0.12,
        ),
    ]
    for i, p in enumerate(ptrs):
        draw_code_pointer(ax, p, smoothstep(0.45 + i * 0.12, 0.65 + i * 0.12, t))


# ---------------------------------------------------------------------------
# ACT IV: Hillclimbing the Single-Target DPS Landscape (Iter 0 -> Iter 6)
# ---------------------------------------------------------------------------
def render_act4_hillclimb(
    ax: Axes,
    t: float,
    global_progress: float,
    ts_label: str,
) -> None:
    """Render Act IV: single-target DPS hillclimbing and rotational discovery."""
    draw_header(
        ax,
        f"ACT IV  [{ts_label}]",
        "Hillclimbing Single-Target DPS: Elemental Shaman [Stormbringer]",
        "From 1,558 DPS (untrained Frost Shock spam) to 45,797 DPS (+27.7% above the hand-written SimC APL)",
        global_progress,
    )

    draw_box(ax, 0.6, 1.85, 8.2, 5.75, border_color=BLUE_C, alpha=1.0)
    ax.text(
        4.7,
        7.25,
        "Single-Target DPS vs RL Iteration (n = 10 Held-Out Seeds)",
        color=WHITE,
        fontsize=11.0,
        fontweight="bold",
        ha="center",
    )

    iters = [0, 1, 2, 3, 4, 5, 6]
    dps_vals = [1558.4, 39943.7, 42944.5, 43411.2, 42860.8, 45473.7, 45797.4]
    simc_dps = 35853.7

    cx0, cx1 = 1.55, 8.35
    cy0, cy1 = 2.55, 6.65
    max_y = 50000.0

    def to_xy(it_val: float, dps_val: float) -> tuple[float, float]:
        px = cx0 + (it_val / 6.0) * (cx1 - cx0)
        py = cy0 + (dps_val / max_y) * (cy1 - cy0)
        return px, py

    for y_val in [0, 10000, 20000, 30000, 40000, 50000]:
        _, gy = to_xy(0, y_val)
        ax.plot([cx0, cx1], [gy, gy], color="#1e293b", lw=0.8, zorder=5)
        ax.text(
            cx0 - 0.12,
            gy,
            f"{y_val // 1000}k",
            color=GREY_B,
            fontsize=8.0,
            family="monospace",
            ha="right",
            va="center",
        )

    for it_i in iters:
        gx, _ = to_xy(it_i, 0)
        ax.text(
            gx,
            cy0 - 0.25,
            f"Iter {it_i}",
            color=GREY_B,
            fontsize=8.2,
            family="monospace",
            ha="center",
        )

    _, simc_y = to_xy(0, simc_dps)
    ax.plot(
        [cx0, cx1], [simc_y, simc_y], color=GOLD_C, lw=1.8, linestyle="--", zorder=7
    )
    ax.text(
        cx1 - 0.1,
        simc_y + 0.14,
        "Hand-Written SimC APL: 35,854 DPS (100%)",
        color=GOLD_C,
        fontsize=8.5,
        fontweight="bold",
        ha="right",
        zorder=9,
    )

    prog_iter = min(6.0, max(0.0, t * 7.2))
    xs_plot: list[float] = []
    ys_plot: list[float] = []
    for i in range(len(iters)):
        if i <= int(prog_iter):
            px, py = to_xy(iters[i], dps_vals[i])
            xs_plot.append(px)
            ys_plot.append(py)
        elif i == int(prog_iter) + 1:
            frac = prog_iter - int(prog_iter)
            cur_it = iters[i - 1] + frac
            cur_dps = dps_vals[i - 1] + frac * (dps_vals[i] - dps_vals[i - 1])
            px, py = to_xy(cur_it, cur_dps)
            xs_plot.append(px)
            ys_plot.append(py)

    if len(xs_plot) >= 2:
        poly_pts = [
            (xs_plot[0], cy0),
            *zip(xs_plot, ys_plot, strict=True),
            (xs_plot[-1], cy0),
        ]
        ax.add_patch(
            Polygon(poly_pts, closed=True, facecolor=GREEN_C, alpha=0.16, zorder=6)
        )
        ax.plot(xs_plot, ys_plot, color=GREEN_C, lw=3.0, zorder=8)

    for i in range(min(len(iters), int(prog_iter) + 1)):
        px, py = to_xy(iters[i], dps_vals[i])
        col = RED_C if i == 0 else GREEN_C
        ax.add_patch(Circle((px, py), 0.11, color=col, zorder=10))
        pct = 100.0 * dps_vals[i] / simc_dps
        lbl = f"{int(dps_vals[i]):,}\n({pct:.0f}%)"
        y_off = 0.34 if i % 2 == 1 or i == 6 else -0.38
        if i == 0:
            y_off = 0.35
        ax.text(
            px,
            py + y_off,
            lbl,
            color=WHITE if i > 0 else RED_C,
            fontsize=7.6,
            fontweight="bold",
            family="monospace",
            ha="center",
            va="center",
            zorder=11,
        )

    draw_box(ax, 9.15, 4.75, 6.25, 2.85, border_color=PURPLE_C, alpha=1.0)
    ax.text(
        12.28,
        7.25,
        "Burn Autodiff<Flex> + AdamW Objective",
        color=PURPLE_C,
        fontsize=10.8,
        fontweight="bold",
        ha="center",
    )
    ax.text(
        12.28,
        6.50,
        r"$\mathcal{L}(\theta) = -\sum_{a \in \mathrm{legal}} \pi_a \log \mathrm{softmax}(z_a) + \frac{1}{2}(v_\theta(s) - G_t)^2$",
        color=WHITE,
        fontsize=11.0,
        ha="center",
    )
    ax.text(
        12.28,
        5.82,
        "Policy Loss:  1.2561 (Iter 1)  --->  0.6007 (Iter 6)\nValue MSE:   3858.6 (Iter 1)  --->  1208.2 (Iter 6)",
        color=TEAL_C,
        fontsize=8.6,
        family="monospace",
        ha="center",
    )
    ax.text(
        12.28,
        5.12,
        "grads = GradientsParams::from_grads(total_loss.backward(), &net)",
        color=GOLD_C,
        fontsize=8.0,
        family="monospace",
        ha="center",
    )

    draw_box(ax, 9.15, 1.85, 6.25, 2.65, border_color=GREEN_C, alpha=1.0)
    ax.text(
        12.28,
        4.18,
        "How RL Beat the Script (+9,944 DPS / -6.3s Kill Time)",
        color=GREEN_C,
        fontsize=10.2,
        fontweight="bold",
        ha="center",
    )
    shifts = [
        ("Voltaic Blaze", "3.5 -> 6.8 casts", "+9,821 DPS (35.5% share)", GREEN_C),
        (
            "Lightning Bolt + OL",
            "11.6 casts (Stormkeeper)",
            "10,231 DPS (22.9% share)",
            BLUE_C,
        ),
        ("Primal Fire Elem", "Summoned on pull", "7,046 DPS (15.8% share)", GOLD_C),
        ("Tempest + OL", "3.2 casts (Hero proc)", "3,105 DPS (6.9% share)", PURPLE_C),
        ("Frost Shock", "418.8 (Iter 0) -> 0.6", "-1,077 DPS (Pruned!)", RED_C),
    ]
    for idx, (ab, c_str, d_str, col) in enumerate(shifts):
        sy = 3.75 - idx * 0.36
        ax.text(9.35, sy, ab, color=col, fontsize=8.3, fontweight="bold")
        ax.text(11.45, sy, c_str, color=WHITE, fontsize=8.0, family="monospace")
        ax.text(
            15.22, sy, d_str, color=col, fontsize=8.0, family="monospace", ha="right"
        )

    ptrs = [
        CodePointer(
            "crates/learn/src/lib.rs:260",
            "BurnTrainer::train(&replay_buffer)",
            12.28,
            0.92,
            12.28,
            4.75,
            PURPLE_C,
            0.18,
        ),
        CodePointer(
            "crates/run/src/train.rs:285",
            "eval_policy(ModelPolicy, temp=0.0)",
            4.7,
            0.92,
            6.1,
            2.55,
            GREEN_C,
            -0.12,
        ),
    ]
    for p in ptrs:
        draw_code_pointer(ax, p, smoothstep(0.25, 0.50, t))


# ---------------------------------------------------------------------------
# ACT V: Knowledge Distillation -> Compact BurnStudent & WoW Lua Addon
# ---------------------------------------------------------------------------
def render_act5_distillation(
    ax: Axes,
    t: float,
    global_progress: float,
    ts_label: str,
) -> None:
    """Render Act V: BurnDistiller teacher-student compression and Lua emission."""
    draw_header(
        ax,
        f"ACT V   [{ts_label}]",
        "Knowledge Distillation & Zero-Dependency Export to model.lua",
        "BurnDistiller compresses the trained Teacher into a 32-unit StudentNet (99.1% agreement) for in-game inference",
        global_progress,
    )

    a1 = smoothstep(0.0, 0.25, t)
    draw_box(ax, 0.65, 2.25, 4.1, 5.1, border_color=PURPLE_C, alpha=a1)
    ax.text(
        2.7,
        6.95,
        "Teacher: PolicyValueNet",
        color=PURPLE_C,
        fontsize=11.5,
        fontweight="bold",
        ha="center",
        alpha=a1,
    )
    ax.text(
        2.7,
        6.52,
        "crates/policy/src/lib.rs:206",
        color=GREY_B,
        fontsize=8.3,
        family="monospace",
        ha="center",
        alpha=a1,
    )
    ax.text(
        2.7,
        4.65,
        "Two-Headed Burn MLP\n"
        "-------------------\n"
        "Input:  132 features\n"
        "Policy: 132 -> 64 -> 64 -> 35\n"
        "Value:  132 -> 64 -> 64 -> 1\n\n"
        "Trained via 6 RL Iterations\n"
        "45,797.4 DPS (127.7% vs SimC)",
        color=WHITE,
        fontsize=9.2,
        family="monospace",
        ha="center",
        va="center",
        alpha=a1,
    )

    a2 = smoothstep(0.18, 0.45, t)
    draw_curved_arrow(
        ax,
        (4.82, 4.8),
        (5.82, 4.8),
        TEAL_C,
        rad=0.0,
        alpha=a2,
        lw=2.6,
        pulse_pos=t * 4.0,
    )
    draw_box(ax, 5.9, 2.25, 4.1, 5.1, border_color=TEAL_C, alpha=a2)
    ax.text(
        7.95,
        6.95,
        "Student: StudentNet",
        color=TEAL_C,
        fontsize=11.5,
        fontweight="bold",
        ha="center",
        alpha=a2,
    )
    ax.text(
        7.95,
        6.52,
        "crates/export/src/lib.rs:60",
        color=GREY_B,
        fontsize=8.3,
        family="monospace",
        ha="center",
        alpha=a2,
    )
    ax.text(
        7.95,
        4.65,
        "Single-Tower Burn MLP\n"
        "---------------------\n"
        "Input:  132 features\n"
        "Hidden: Linear(132, 32) + ReLU\n"
        "Output: Linear(32, 35)\n\n"
        "Teacher-Student Agreement:\n"
        "  Stormbringer: 99.1%\n"
        "  Arms Slayer:  100.0%",
        color=WHITE,
        fontsize=9.2,
        family="monospace",
        ha="center",
        va="center",
        alpha=a2,
    )

    a3 = smoothstep(0.35, 0.65, t)
    draw_curved_arrow(
        ax,
        (10.08, 4.8),
        (11.08, 4.8),
        GREEN_C,
        rad=0.0,
        alpha=a3,
        lw=2.6,
        pulse_pos=t * 4.0 + 0.4,
    )
    draw_box(
        ax, 11.15, 2.25, 4.25, 5.1, border_color=GREEN_C, fill_color=BG_CODE, alpha=a3
    )
    ax.text(
        13.28,
        6.95,
        "PortunusRotation/model.lua",
        color=GREEN_C,
        fontsize=11.2,
        fontweight="bold",
        family="monospace",
        ha="center",
        alpha=a3,
    )
    ax.text(
        13.28,
        6.52,
        "57,624 bytes | Interface 120100",
        color=GOLD_C,
        fontsize=8.3,
        family="monospace",
        ha="center",
        alpha=a3,
    )
    lua_code = (
        "-- Generated by portunus-export\n"
        "PortunusRotation = {\n"
        '  schema = "flat:spec_262",\n'
        "  input_width = 132,\n"
        "  hidden_width = 32,\n"
        "  output_width = 35,\n"
        "  l1_weight = { ... },\n"
        "  l1_bias   = { ... },\n"
        "  l2_weight = { ... },\n"
        "  l2_bias   = { ... },\n"
        "}\n"
        "function PortunusRotation:Eval(x)\n"
        "  -- Pure Lua ReLU MLP forward\n"
        "  return best_action\n"
        "end"
    )
    ax.text(
        11.38,
        4.45,
        lua_code,
        color=WHITE,
        fontsize=8.2,
        family="monospace",
        va="center",
        alpha=0.94 * a3,
    )

    ptrs = [
        CodePointer(
            "crates/export/src/lib.rs:157",
            "BurnDistiller::distill(&teacher)",
            5.35,
            1.15,
            5.35,
            4.8,
            TEAL_C,
            -0.12,
        ),
        CodePointer(
            "crates/export/src/lib.rs:242",
            "LuaAddonEmitter::emit(&student)",
            10.6,
            1.15,
            10.6,
            4.8,
            GREEN_C,
            0.12,
        ),
    ]
    for p in ptrs:
        draw_code_pointer(ax, p, smoothstep(0.40, 0.65, t))


def synthesize_voiceover(
    tmp_dir: Path,
    voice: str = "Samantha",
    rate_wpm: int = 182,
    pad_seconds: float = 0.55,
) -> tuple[list[float], Path]:
    """Synthesize per-act TTS audio with macOS `say`, pad with silence, and concat."""
    act_wavs: list[Path] = []
    act_durations: list[float] = []

    for idx, act in enumerate(VOICEOVER_ACTS):
        raw_aiff = tmp_dir / f"act_{idx}_raw.aiff"
        padded_wav = tmp_dir / f"act_{idx}_padded.wav"
        subprocess.run(
            [
                "say",
                "-v",
                voice,
                "-r",
                str(rate_wpm),
                "-o",
                str(raw_aiff),
                act.narration,
            ],
            check=True,
        )
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-i",
                str(raw_aiff),
                "-af",
                f"apad=pad_dur={pad_seconds:.2f}",
                "-ar",
                "44100",
                "-ac",
                "1",
                str(padded_wav),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        probe = subprocess.run(
            [
                "ffprobe",
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
                str(padded_wav),
            ],
            check=True,
            capture_output=True,
            text=True,
        )
        dur = float(probe.stdout.strip())
        act_durations.append(dur)
        act_wavs.append(padded_wav)

    concat_txt = tmp_dir / "audio_concat.txt"
    concat_txt.write_text(
        "\n".join(f"file '{w.as_posix()}'" for w in act_wavs) + "\n",
        encoding="utf-8",
    )
    full_audio = tmp_dir / "voiceover_full.m4a"
    subprocess.run(
        [
            "ffmpeg",
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            str(concat_txt),
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            str(full_audio),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return act_durations, full_audio


def write_voiceover_markdown(out_dir: Path, act_durations: list[float]) -> None:
    """Write the timestamped voiceover script to docs/video/voiceover_script.md."""
    lines = [
        "# Portunus Burn RL — 3Blue1Brown Narrative Voiceover Script",
        "",
        "This script is synthesized via TTS and muxed directly into `docs/video/portunus_rl_3b1b.mp4`.",
        "",
    ]
    elapsed = 0.0
    for act, dur in zip(VOICEOVER_ACTS, act_durations, strict=True):
        start_ts = format_mmss(elapsed)
        end_ts = format_mmss(elapsed + dur)
        lines.append(f"## {act.act_roman} `[{start_ts} - {end_ts}]` — {act.title}")
        lines.append("")
        lines.append(f"> {act.narration}")
        lines.append("")
        elapsed += dur

    (out_dir / "voiceover_script.md").write_text("\n".join(lines), encoding="utf-8")


@dataclass(frozen=True)
class FrameTask:
    """Parameters for rendering a single video frame in a worker process."""

    global_frame: int
    act_idx: int
    t_local: float
    global_progress: float
    ts_label: str
    frame_path: Path
    still_path: Path | None


def render_single_frame(task: FrameTask) -> None:
    """Render one frame (and optional act still) to disk."""
    render_fns: tuple[Callable[[Axes, float, float, str], None], ...] = (
        render_act1_architecture,
        render_act2_network,
        render_act3_mcts,
        render_act4_hillclimb,
        render_act5_distillation,
    )
    fig = plt.figure(figsize=(16, 9), dpi=120)
    ax = setup_canvas(fig)
    render_fns[task.act_idx](ax, task.t_local, task.global_progress, task.ts_label)
    fig.savefig(task.frame_path, facecolor=BG_DARK)
    if task.still_path is not None:
        fig.savefig(task.still_path, facecolor=BG_DARK)
    plt.close(fig)


def main() -> None:
    """Synthesize TTS voiceover, render synced 3b1b video frames, and mux MP4 + audio."""
    out_dir = Path(__file__).resolve().parent
    out_dir.mkdir(parents=True, exist_ok=True)

    fps = 8

    with tempfile.TemporaryDirectory(prefix="portunus_frames_") as tmp_str:
        tmp_dir = Path(tmp_str)
        act_durations, full_audio = synthesize_voiceover(tmp_dir)
        write_voiceover_markdown(out_dir, act_durations)

        act_frame_counts = [max(24, round(d * fps)) for d in act_durations]
        total_frames = sum(act_frame_counts)

        tasks: list[FrameTask] = []
        global_frame = 0
        elapsed_sec = 0.0

        for act_idx, (act, n_frames) in enumerate(
            zip(VOICEOVER_ACTS, act_frame_counts, strict=True)
        ):
            ts_label = format_mmss(elapsed_sec)
            for local_frame in range(n_frames):
                t_local = local_frame / float(max(1, n_frames - 1))
                global_progress = (global_frame + 1) / float(total_frames)
                frame_path = tmp_dir / f"frame_{global_frame:04d}.png"
                still_path = (
                    (out_dir / act.still_filename)
                    if local_frame == n_frames - 3
                    else None
                )
                tasks.append(
                    FrameTask(
                        global_frame=global_frame,
                        act_idx=act_idx,
                        t_local=t_local,
                        global_progress=global_progress,
                        ts_label=ts_label,
                        frame_path=frame_path,
                        still_path=still_path,
                    )
                )
                global_frame += 1
            elapsed_sec += act_durations[act_idx]

        max_workers = min(8, os.cpu_count() or 4)
        with ProcessPoolExecutor(max_workers=max_workers) as pool:
            list(pool.map(render_single_frame, tasks, chunksize=8))

        mp4_path = out_dir / "portunus_rl_3b1b.mp4"
        gif_path = out_dir / "portunus_rl_3b1b.gif"
        voiceover_m4a = out_dir / "voiceover.m4a"
        shutil.copyfile(full_audio, voiceover_m4a)

        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-framerate",
                str(fps),
                "-i",
                str(tmp_dir / "frame_%04d.png"),
                "-i",
                str(full_audio),
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-crf",
                "18",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-shortest",
                str(mp4_path),
            ],
            check=True,
        )

        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-i",
                str(mp4_path),
                "-vf",
                "fps=3,scale=960:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=128[p];[s1][p]paletteuse=dither=bayer",
                str(gif_path),
            ],
            check=True,
        )

        tmp_video_dir = Path("/tmp/portunus_video")
        if tmp_video_dir.exists():
            shutil.copyfile(mp4_path, tmp_video_dir / "portunus_rl_3b1b.mp4")

        total_dur = sum(act_durations)
        print(
            f"Rendered narrated MP4 ({total_dur:.1f}s): {mp4_path} ({mp4_path.stat().st_size} bytes)"
        )
        print(f"Rendered GIF: {gif_path} ({gif_path.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
