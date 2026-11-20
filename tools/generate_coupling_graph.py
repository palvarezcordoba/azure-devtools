#!/usr/bin/env python3
"""Generate a compact module-coupling DOT graph from cargo-modules output."""

import argparse
import re
import subprocess
from pathlib import Path
from xml.sax.saxutils import escape

NODE = re.compile(r'"([^"]+)" \[label="pub mod\|([^"]+)"')
EDGE = re.compile(r'"([^"]+)" -> "([^"]+)" \[label="uses"')


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest-path",
        type=Path,
        default=root / "Cargo.toml",
        help="Rust package manifest to analyze",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=root / "coupling-analyzed.dot",
        help="DOT file to write",
    )
    parser.add_argument(
        "--matrix-output",
        type=Path,
        default=root / "coupling-matrix.svg",
        help="SVG dependency matrix to write",
    )
    args = parser.parse_args()

    raw = subprocess.run(
        [
            "cargo",
            "modules",
            "dependencies",
            "--lib",
            "--no-externs",
            "--no-sysroot",
            "--no-fns",
            "--no-types",
            "--no-traits",
            "--no-owns",
            "--layout",
            "dot",
            "--manifest-path",
            str(args.manifest_path),
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout

    modules = dict(NODE.findall(raw))
    edges = sorted(set(EDGE.findall(raw)))
    if not modules:
        raise RuntimeError("cargo-modules returned no module nodes")
    if any(source not in modules or target not in modules for source, target in edges):
        raise RuntimeError("cargo-modules returned a dependency for an unknown module")

    incoming = {module_id: 0 for module_id in modules}
    outgoing = {module_id: 0 for module_id in modules}
    for source, target in edges:
        outgoing[source] += 1
        incoming[target] += 1

    ids = {module_id: f"module_{index}" for index, module_id in enumerate(sorted(modules))}
    lines = [
        "digraph AzureDevToolsModuleCoupling {",
        '  graph [rankdir=LR, bgcolor="transparent", pad="0.35", nodesep="0.55", ranksep="0.8", fontname="Arial", labelloc="t", label="Azure DevTools — Rust Analyzer Module Coupling", outputorder="edgesfirst"];',
        '  node [shape=box, style="rounded,filled", fontname="Arial", fontsize=11, margin="0.16,0.12", color="#94a3b8", fillcolor="#ffffff", fontcolor="#172033", penwidth=1.4];',
        '  edge [fontname="Arial", fontsize=9, color="#64748b", fontcolor="#475569", penwidth=1.25, arrowsize=0.7];',
        "",
        "  // Node degrees count distinct module-to-module `uses` edges.",
    ]

    for module_id in sorted(modules):
        name = modules[module_id]
        group = "state" if "::state::" in module_id else "tui"
        color, fill = (
            ("#7c3aed", "#f5f3ff") if group == "state" else ("#16a34a", "#f0fdf4")
        )
        label = f"{name}\\nin {incoming[module_id]} | out {outgoing[module_id]}"
        lines.append(
            f'  {ids[module_id]} [label="{label}", color="{color}", fillcolor="{fill}"];'
        )

    lines.append("")
    for source, target in edges:
        lines.append(f'  {ids[source]} -> {ids[target]} [label="uses"];')
    lines.append("}")

    output = args.output.resolve()
    output.write_text("\n".join(lines) + "\n")
    print(f"Wrote {len(modules)} modules and {len(edges)} dependency edges to {output}")
    write_matrix(args.matrix_output.resolve(), modules, edges)


def write_matrix(output: Path, modules: dict[str, str], edges: list[tuple[str, str]]) -> None:
    names = sorted(modules)
    dependencies = set(edges)
    cell = 68
    left = 250
    top = 230
    width = left + cell * len(names) + 24
    height = top + cell * len(names) + 36
    lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title description">',
        "<title id=\"title\">Azure DevTools module dependency matrix</title>",
        '<desc id="description">Rows depend on columns. Blue cells are direct module use dependencies reported by cargo-modules.</desc>',
        '<rect width="100%" height="100%" fill="#ffffff"/>',
        '<g font-family="Arial, sans-serif" fill="#172033">',
        '<text x="24" y="34" font-size="19" font-weight="700">Azure DevTools — Module Coupling Matrix</text>',
        '<text x="24" y="58" font-size="12" fill="#475569">Row module depends on column module · direct Rust module “uses” edges</text>',
        '<rect x="24" y="76" width="14" height="14" rx="2" fill="#2563eb"/>',
        '<text x="45" y="88" font-size="11">direct dependency</text>',
        '<rect x="165" y="76" width="14" height="14" rx="2" fill="#f1f5f9" stroke="#cbd5e1"/>',
        '<text x="186" y="88" font-size="11">no direct dependency</text>',
    ]

    for column, module_id in enumerate(names):
        x = left + column * cell + cell / 2
        short = escape(modules[module_id].removeprefix("azure_vars::"))
        lines.append(
            f'<text x="{x:g}" y="{top - 12}" transform="rotate(-45 {x:g} {top - 12})" text-anchor="end" font-size="11">{short}</text>'
        )

    for row, source in enumerate(names):
        y = top + row * cell
        source_name = escape(modules[source].removeprefix("azure_vars::"))
        lines.append(
            f'<text x="{left - 14}" y="{y + cell / 2 + 4:g}" text-anchor="end" font-size="11">{source_name}</text>'
        )
        for column, target in enumerate(names):
            x = left + column * cell
            present = (source, target) in dependencies
            fill = "#2563eb" if present else "#f1f5f9"
            stroke = "#1d4ed8" if present else "#cbd5e1"
            value = "1" if present else "·"
            text_fill = "#ffffff" if present else "#94a3b8"
            lines.extend(
                [
                    f'<rect x="{x + 3}" y="{y + 3}" width="{cell - 6}" height="{cell - 6}" rx="7" fill="{fill}" stroke="{stroke}"/>',
                    f'<text x="{x + cell / 2:g}" y="{y + cell / 2 + 5:g}" text-anchor="middle" font-size="14" font-weight="700" fill="{text_fill}">{value}</text>',
                ]
            )

    lines.extend(["</g>", "</svg>"])
    output.write_text("\n".join(lines) + "\n")
    print(f"Wrote dependency matrix to {output}")


if __name__ == "__main__":
    main()
