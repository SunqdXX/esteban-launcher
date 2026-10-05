import colorsys
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SKIP_DIRS = {"node_modules", "target", "dist", "gen", ".git", "icons", "fonts"}
UI_EXT = {".css", ".ts", ".tsx", ".html"}
NAMED_WARM = {"orange", "darkorange", "coral", "tomato", "gold", "goldenrod", "darkgoldenrod", "peru", "chocolate", "sandybrown", "orangered"}
HEX = re.compile(r"#([0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b")
COMMENT_RULES = {
    ".rs": re.compile(r"^\s*(//|/\*|\*)"),
    ".ts": re.compile(r"^\s*(//|/\*|\*)"),
    ".tsx": re.compile(r"^\s*(//|/\*|\*|\{/\*)"),
    ".js": re.compile(r"^\s*(//|/\*|\*)"),
    ".css": re.compile(r"/\*"),
    ".toml": re.compile(r"^\s*#"),
    ".yml": re.compile(r"^\s*#"),
    ".yaml": re.compile(r"^\s*#"),
    ".py": re.compile(r"^\s*#"),
    ".sh": re.compile(r"^\s*#(?!!)"),
}


OFFLINE = re.compile(r"dev-offline|dev_offline|OfflinePlayer:")
SELF = pathlib.Path(__file__).resolve()


def files():
    listed = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        cwd=ROOT,
        capture_output=True,
        check=True,
    ).stdout.decode().split("\0")
    for name in listed:
        path = ROOT / name
        if name and path.is_file() and not any(part in SKIP_DIRS for part in pathlib.Path(name).parts):
            yield path


def is_warm(hex_value):
    h = hex_value if len(hex_value) == 6 else "".join(c * 2 for c in hex_value)
    r, g, b = (int(h[i:i + 2], 16) / 255 for i in (0, 2, 4))
    hue, light, sat = colorsys.rgb_to_hls(r, g, b)
    return 15 <= hue * 360 <= 50 and sat > 0.35 and 0.2 < light < 0.9


def main():
    problems = []
    for path in files():
        rel = path.relative_to(ROOT)
        text = path.read_text(encoding="utf-8", errors="ignore")
        if path.suffix in UI_EXT and rel.parts[0] == "app":
            for match in HEX.finditer(text):
                if is_warm(match.group(1)):
                    problems.append(f"{rel}: orange or amber color #{match.group(1)}")
            for word in NAMED_WARM:
                if re.search(rf":\s*{word}\b", text, re.IGNORECASE):
                    problems.append(f"{rel}: orange or amber named color {word}")
        rule = COMMENT_RULES.get(path.suffix)
        if rule:
            for number, line in enumerate(text.splitlines(), 1):
                if rule.search(line):
                    problems.append(f"{rel}:{number}: comment line")
        if path.resolve() != SELF and OFFLINE.search(text):
            problems.append(f"{rel}: offline test sessions are not part of this repo")
    for problem in problems:
        print(problem)
    if problems:
        sys.exit(1)
    print("policy ok")


main()
