#!/usr/bin/env python3
"""Fetch only the Fluent Color SVGs used by these apps, at a pinned revision."""
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import subprocess
from urllib.parse import quote
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
REVISION = "82ecf7cbdb43684403c7125efa160be04a33dca5"
UPSTREAM = "https://raw.githubusercontent.com/microsoft/fluentui-system-icons/" + REVISION
ICONS = {
    "home": "Home", "settings": "Settings", "chart_multiple": "Chart Multiple",
    "apps": "Apps", "text_bullet_list_square": "Text Bullet List Square",
    "chat": "Chat", "image": "Image", "puzzle_piece": "Puzzle Piece", "design_ideas": "Design Ideas",
    "calendar": "Calendar",
}
APPLICATIONS = {
    "backend-settings": ["home", "settings", "chart_multiple", "apps", "puzzle_piece"],
    "gallery": ["apps", "home", "settings", "design_ideas", "text_bullet_list_square", "puzzle_piece", "chat", "image", "calendar"],
}

def fetch(path):
    request = Request(UPSTREAM + "/" + quote(path, safe="/"), headers={"User-Agent": "Plico-assets"})
    with urlopen(request, timeout=30) as response:
        return response.read()

def main():
    with ThreadPoolExecutor(max_workers=5) as pool:
        def download(name):
            return name, fetch(f"assets/{ICONS[name]}/SVG/ic_fluent_{name}_24_color.svg")
        assets = dict(pool.map(download, ICONS))
    license_text = fetch("LICENSE")
    for app, names in APPLICATIONS.items():
        directory = ROOT / "apps" / app / "assets/fluent"
        directory.mkdir(parents=True, exist_ok=True)
        for name in names:
            (directory / (name + ".svg")).write_bytes(assets[name])
        (directory / "LICENSE").write_bytes(license_text)
        (directory / "README.md").write_text(
            "# Fluent System Icons — Color\n\n"
            "Source: https://github.com/microsoft/fluentui-system-icons\n\n"
            f"Revision: `{REVISION}`. License: MIT (adjacent LICENSE).\n\n"
            "Only the used 24px Color assets are included, without modification.\n"
            "Original paths: `assets/<group>/SVG/ic_fluent_<name>_24_color.svg`.\n"
            "Regenerate with `python3 scripts/update-fluent-icons.py`; normal builds are offline.\n"
        )
        module = "//! Selected, unmodified Fluent Color assets; see ../assets/fluent/README.md.\nuse plico_core::SvgAsset;\n"
        for name in names:
            module += f'pub const {name.upper()}: SvgAsset = plico_core::svg_asset!("../assets/fluent/{name}.svg");\n'
        module_path = ROOT / "apps" / app / "src/color_icons.rs"
        module_path.write_text(module)
        subprocess.run(["rustfmt", "--edition", "2024", str(module_path)], check=True)
    print(f"Fetched {len(assets)} selected Fluent Color icons at {REVISION}.")

if __name__ == "__main__":
    main()
