#!/usr/bin/env python3
"""What changes when XeeonOS moves to a newer Arch snapshot.

    tools/snapshot-report.py OLD NEW      (dates as YYYY/MM/DD)

Prints, in Markdown, for the pull request of .github/workflows/snapshot.yml:
  - the Arch Linux news published between the two snapshots (they announce
    the changes that need care: manual steps, replaced packages, ...);
  - the packages of XeeonOS whose version changes (the packages listed in
    mkosi.conf.d/*/mkosi.conf and everything they depend on), with the
    important ones and the new major versions first.

Arch supports only whole upgrades, so a single package cannot be held back:
if something looks risky, the snapshot pull request is simply not merged
(next week's run updates it with a newer date).

The exit code is 0 when nothing needs attention, 10 when there is news or a
new major version of an important package: the workflow then opens the pull
request as a draft.
"""

import email.utils
import io
import re
import subprocess
import sys
import tarfile
import urllib.request
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path

ARCHIVE = "https://archive.archlinux.org/repos"
NEWS = "https://archlinux.org/feeds/news/"
REPOS = ["core", "extra"]
# Read first: the parts of the system that can stop it from starting.
IMPORTANT = {
    "linux", "linux-firmware", "systemd", "glibc", "btrfs-progs",
    "cryptsetup", "networkmanager", "nftables", "fwupd", "util-linux",
    "dbus", "openssl", "sudo", "intel-ucode", "amd-ucode", "mesa",
}


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "xeeonos-snapshot-report"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def read_db(date):
    """{name: (version, [depends], [provides])} for one snapshot."""
    pkgs = {}
    for repo in REPOS:
        data = fetch(f"{ARCHIVE}/{date}/{repo}/os/x86_64/{repo}.db")
        if data[:4] == b"\x28\xb5\x2f\xfd":  # zstd, which tarfile may not read
            data = subprocess.run(["zstd", "-dc"], input=data, capture_output=True,
                                  check=True).stdout
        with tarfile.open(fileobj=io.BytesIO(data)) as tar:
            for member in tar:
                if not member.name.endswith("/desc"):
                    continue
                fields, key = {}, None
                for line in tar.extractfile(member).read().decode().splitlines():
                    if line.startswith("%") and line.endswith("%"):
                        key = line.strip("%")
                        fields[key] = []
                    elif line and key:
                        fields[key].append(line)
                name = fields["NAME"][0]
                pkgs[name] = (
                    fields["VERSION"][0],
                    fields.get("DEPENDS", []),
                    fields.get("PROVIDES", []),
                )
    return pkgs


def bare(dep):
    """'glibc>=2.40' -> 'glibc'"""
    return re.split(r"[<>=:]", dep, maxsplit=1)[0].strip()


def wanted():
    """The package names in the Packages= settings of the image modules."""
    names = []
    for conf in sorted(Path("mkosi.conf.d").glob("*/mkosi.conf")):
        inside = False
        for line in conf.read_text().splitlines():
            if re.match(r"^Packages=", line):
                inside = True
                names += line.split("=", 1)[1].split()
            elif inside and line.startswith((" ", "\t")):
                text = line.strip()
                if text and not text.startswith("#"):
                    names += text.split()
            else:
                inside = False
    return names


def closure(pkgs, roots):
    """The roots and everything they depend on."""
    providers = {}
    for name, (_, _, provides) in pkgs.items():
        for p in provides:
            providers.setdefault(bare(p), name)
    seen, todo = set(), list(roots)
    while todo:
        name = bare(todo.pop())
        name = name if name in pkgs else providers.get(name)
        if name and name not in seen:
            seen.add(name)
            todo += pkgs[name][1]
    return seen


def major(name, version):
    """'2:1.2.3-1' -> '2:1'; for the kernel, the series: '6.18.1-1' -> ':6.18'"""
    epoch, _, rest = version.rpartition(":")
    parts = re.split(r"[.+_-]", rest)
    return f"{epoch}:" + ".".join(parts[: 2 if name == "linux" else 1])


def news(old, new):
    """[(date, title, link)] published after the old snapshot."""
    start = datetime.strptime(old, "%Y/%m/%d").replace(tzinfo=timezone.utc)
    items = []
    for item in ET.fromstring(fetch(NEWS)).iter("item"):
        when = email.utils.parsedate_to_datetime(item.findtext("pubDate"))
        if when >= start:
            items.append((when.date().isoformat(), item.findtext("title"), item.findtext("link")))
    return items


def main():
    old, new = sys.argv[1], sys.argv[2]
    before, after = read_db(old), read_db(new)
    ours = closure(after, wanted()) | closure(before, wanted())
    changes = []
    for name in sorted(ours):
        a = before.get(name, ("(new)",))[0]
        b = after.get(name, ("(removed)",))[0]
        if a != b:
            big = a[0] != "(" and b[0] != "(" and major(name, a) != major(name, b)
            changes.append((name in IMPORTANT, big, name, a, b))
    try:
        items = news(old, new)
        news_error = None
    except Exception as e:  # the report still helps without the news
        items, news_error = [], str(e)

    attention = bool(items) or any(imp and big for imp, big, *_ in changes)
    out = [f"## Arch news since {old}", ""]
    if news_error:
        out.append(f"Could not read the news ({news_error}): check {NEWS} by hand.")
    elif items:
        out.append("Read these before merging: they may need a change in XeeonOS.")
        out.append("")
        out += [f"- {d}: [{t}]({link})" for d, t, link in items]
    else:
        out.append("None.")
    out += ["", f"## Packages of XeeonOS that change ({len(changes)} of {len(ours)})", ""]
    if changes:
        out += ["| | Package | From | To |", "|---|---|---|---|"]
        for imp, big, name, a, b in sorted(changes, key=lambda c: (not c[0], not c[1], c[2])):
            notes = (["important"] if imp else []) + (["new major version"] if big else [])
            mark = ("⚠️ " if imp and big else "") + ", ".join(notes)
            out.append(f"| {mark} | {name} | {a} | {b} |")
    else:
        out.append("None.")
    out += ["", "Arch supports only whole upgrades, so one package cannot be held "
            "back. If something here looks risky, leave this pull request open: "
            "next week's run moves it to a newer snapshot."]
    print("\n".join(out))
    return 10 if attention else 0


if __name__ == "__main__":
    sys.exit(main())
