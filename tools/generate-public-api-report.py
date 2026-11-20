#!/usr/bin/env python3
"""Render cargo-public-api output as a static, deterministic HTML report."""

import argparse
from html import escape
from pathlib import Path


def module_tree(paths: list[str]) -> str:
    tree: dict[str, dict] = {}
    for path in paths:
        node = tree
        for part in path.split("::"):
            node = node.setdefault(part, {})

    def render(nodes: dict[str, dict]) -> str:
        items = []
        for name in sorted(nodes):
            items.append(f"<li><code>{escape(name)}</code>{render(nodes[name])}</li>")
        return f"<ul>{''.join(items)}</ul>" if items else ""

    return render(tree)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="cargo-public-api stdout text file")
    parser.add_argument("output", type=Path, help="HTML report output path")
    args = parser.parse_args()

    api_text = args.input.read_text(encoding="utf-8")
    lines = api_text.splitlines()
    module_paths = [
        line.removeprefix("pub mod ")
        for line in lines
        if line.startswith("pub mod ")
    ]
    public_lines = [line for line in lines if line.startswith("pub ")]
    counts = {
        "Public modules": len(module_paths),
        "Public API declarations": len(public_lines),
        "Functions": sum(line.startswith("pub fn ") for line in lines),
        "Structs": sum(line.startswith("pub struct ") for line in lines),
        "Enums": sum(line.startswith("pub enum ") for line in lines),
        "Traits": sum(line.startswith("pub trait ") for line in lines),
    }

    count_cards = "".join(
        f'<article><strong>{count}</strong><span>{escape(label)}</span></article>'
        for label, count in counts.items()
    )
    report = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Azure DevTools — Rust Public API Report</title>
  <style>
    :root {{ color-scheme: light; --ink:#172033; --muted:#607089; --line:#dce4ef; --blue:#2457c5; }}
    * {{ box-sizing:border-box; }}
    body {{ margin:0; color:var(--ink); background:#f5f7fb; font:15px/1.55 system-ui,sans-serif; }}
    header {{ padding:36px max(24px,calc((100% - 1040px)/2)); background:#172033; color:#fff; }}
    header h1 {{ margin:0 0 6px; font-size:32px; }}
    header p {{ margin:0; color:#d5deed; }}
    main {{ max-width:1040px; margin:28px auto; padding:0 24px; }}
    section {{ margin:16px 0; padding:22px; background:white; border:1px solid var(--line); border-radius:12px; }}
    h2 {{ margin:0 0 14px; font-size:21px; }}
    .counts {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(140px,1fr)); gap:10px; }}
    article {{ border:1px solid var(--line); border-radius:9px; padding:12px; }}
    article strong {{ display:block; font-size:24px; color:var(--blue); }}
    article span {{ color:var(--muted); font-size:13px; }}
    pre {{ overflow:auto; max-height:72vh; padding:16px; border-radius:8px; background:#101827; color:#e4ecf7; font:12px/1.5 ui-monospace,monospace; }}
    code {{ overflow-wrap:anywhere; }}
    .modules ul {{ padding-left:22px; }}
    .modules li {{ margin:4px 0; }}
    a {{ color:var(--blue); }}
    .note {{ color:var(--muted); }}
  </style>
</head>
<body>
<header>
  <h1>Azure DevTools — Rust Public API</h1>
  <p>Static analysis output from <code>cargo-public-api 0.52.0</code> · generated from Rust rustdoc JSON</p>
</header>
<main>
  <section>
    <h2>Surface summary</h2>
    <div class="counts">{count_cards}</div>
  </section>
  <section class="modules">
    <h2>Public module hierarchy</h2>
    <p class="note">Tree derived mechanically from the <code>pub mod</code> paths in the analyzer output.</p>
    {module_tree(module_paths)}
  </section>
  <section>
    <h2>Complete analyzer listing</h2>
    <p><a href="{escape(args.input.name)}">Download the original cargo-public-api output</a> ({len(lines)} lines).</p>
    <pre>{escape(api_text)}</pre>
  </section>
  <section>
    <h2>Provenance and limits</h2>
    <p>Extractor: <code>cargo-public-api 0.52.0</code>. Command: <code>cargo public-api -sss --color never</code>. The listing is analyzer output; counts and module tree are computed from those lines by this script.</p>
    <p class="note">This extraction required a temporary compatibility rename of the existing removed <code>lazy_type_alias</code> feature gate for the current nightly rustdoc run. The source feature gate was restored afterward; the rename does not introduce or remove public declarations.</p>
  </section>
</main>
</body>
</html>
"""
    args.output.write_text(report, encoding="utf-8")


if __name__ == "__main__":
    main()
