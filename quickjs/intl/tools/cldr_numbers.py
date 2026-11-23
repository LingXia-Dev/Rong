#!/usr/bin/env python3
"""Generates src/cldr_numbers.rs: the CLDR number symbols and patterns that
ICU4X's data does not carry yet (exponent separator, NaN, infinity, the
approximately sign and pattern, the range pattern, the percent sign as its
own symbol), per locale and numbering system.

CLDR_VERSION must be the CLDR the pinned ICU4X release was tested with, so
the table and the app's data blob agree; the script fails if they differ.
Only entries that change a lookup are kept: a locale whose values differ
from the root, or equal the root while an ancestor differs (so the lookup's
fallback walk stops there).

    python3 quickjs/intl/tools/cldr_numbers.py          # rewrite the table
    python3 quickjs/intl/tools/cldr_numbers.py --check  # fail if it is stale
"""

import io
import json
import re
import sys
import tarfile
import unicodedata
import xml.etree.ElementTree as ET
import urllib.request
from pathlib import Path

CLDR_VERSION = "49.0.0"
# The same release in CLDR's XML: root's symbols for numbering systems no
# locale defines, which the JSON leaves out.
CLDR_XML_TAG = "release-49"

CRATE = Path(__file__).resolve().parent.parent
OUT = CRATE / "src" / "cldr_numbers.rs"
ROOT_FIELDS = ("decimal", "group", "minus_sign", "plus_sign")
FIELDS = ("exponential", "nan", "infinity", "approximately_sign", "approximately", "range", "percent_sign")


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "rong-intl-generator"})
    with urllib.request.urlopen(request, timeout=120) as response:
        return response.read()


def icu4x_version():
    text = (CRATE / "Cargo.toml").read_text()
    found = re.search(r'^icu = \{ version = "=([0-9.]+)"', text, re.M)
    if not found:
        sys.exit("cannot find the pinned icu version in Cargo.toml")
    return found.group(1)


def icu4x_cldr_version(version):
    url = (
        "https://raw.githubusercontent.com/unicode-org/icu4x/"
        f"icu%40{version}/provider/source/src/lib.rs"
    )
    found = re.search(r'TESTED_CLDR_TAG: &\'static str = "([0-9.]+)"', fetch(url).decode())
    if not found:
        sys.exit(f"cannot find TESTED_CLDR_TAG for ICU4X {version}")
    return found.group(1)


def package(name):
    url = f"https://registry.npmjs.org/{name}/-/{name}-{CLDR_VERSION}.tgz"
    return tarfile.open(fileobj=io.BytesIO(fetch(url)), mode="r:gz")


def load():
    numbers = {}
    with package("cldr-numbers-full") as tar:
        for member in tar.getmembers():
            parts = member.name.split("/")
            if len(parts) == 4 and parts[1] == "main" and parts[3] == "numbers.json":
                data = json.load(tar.extractfile(member))
                locale = parts[2]
                numbers[locale] = data["main"][locale]["numbers"]
    with package("cldr-core") as tar:
        member = tar.getmember("package/supplemental/parentLocales.json")
        parents = json.load(tar.extractfile(member))["supplemental"]["parentLocales"]["parentLocale"]
    return numbers, parents


def root_symbols():
    """Root's own symbols per numbering system (not the ones that alias the
    locale's latn symbols), from CLDR's XML."""
    url = f"https://raw.githubusercontent.com/unicode-org/cldr/{CLDR_XML_TAG}/common/main/root.xml"
    root = ET.fromstring(fetch(url))
    found = {}
    for symbols in root.findall("./numbers/symbols"):
        if symbols.find("alias") is None:
            found[symbols.get("numberSystem")] = {c.tag: c.text for c in symbols}
    return found


def values(numbers, locale, system):
    symbols = numbers[locale].get(f"symbols-numberSystem-{system}")
    patterns = numbers[locale].get(f"miscPatterns-numberSystem-{system}")
    if symbols is None or patterns is None:
        return None
    return (
        symbols["exponential"],
        symbols["nan"],
        symbols["infinity"],
        symbols["approximatelySign"],
        patterns["approximately"],
        patterns["range"],
        symbols["percentSign"],
    )


def ancestors(locale, parents):
    """Every locale a fallback walk from `locale` may visit: CLDR's parent
    locales and plain truncation, both."""
    seen, todo = set(), [locale]
    while todo:
        current = todo.pop()
        for parent in (parents.get(current), current.rsplit("-", 1)[0] if "-" in current else None):
            if parent and parent not in ("root", "und") and parent not in seen:
                seen.add(parent)
                todo.append(parent)
    return seen


def generate():
    numbers, parents = load()
    systems = sorted({k.split("-", 2)[2] for n in numbers.values() for k in n if k.startswith("symbols-numberSystem-")})
    root = {s: values(numbers, "und", s) for s in systems}
    entries = {}
    for system in systems:
        base = root[system] or root["latn"]
        for locale in sorted(numbers):
            if locale == "und":
                continue
            mine = values(numbers, locale, system)
            if mine is None:
                continue
            differs = mine != base
            above = any(
                values(numbers, a, system) not in (None, base) for a in ancestors(locale, parents) if a in numbers
            )
            if differs or above:
                entries[f"{locale.replace('_', '-')}/{system}"] = mine
        if root[system] is not None:
            entries[f"und/{system}"] = root[system]

    # Root's own symbols for systems the JSON's root lacks (arab, arabext);
    # their patterns alias latn's.
    xml_root = root_symbols()
    latn = root["latn"]
    extra = {}
    for system, sym in sorted(xml_root.items()):
        if system == "latn" or root.get(system) is not None:
            continue
        entries[f"und/{system}"] = (
            sym["exponential"], sym["nan"], sym["infinity"], sym["approximatelySign"],
            latn[4], latn[5], sym["percentSign"],
        )
        extra[system] = (sym["decimal"], sym["group"], sym["minusSign"], sym["plusSign"])

    table = sorted(set(entries.values()))
    index = {v: i for i, v in enumerate(table)}
    def rust(text):
        # Invisible and format characters (zero-width spaces, bidi marks) as
        # escapes, so the source shows them.
        out = ""
        for ch in text:
            if ch in '"\\':
                out += "\\" + ch
            elif unicodedata.category(ch) in ("Cc", "Cf", "Zl", "Zp") or (
                unicodedata.category(ch) == "Zs" and ch != " "
            ):
                out += "\\u{%x}" % ord(ch)
            else:
                out += ch
        return '"' + out + '"'

    lines = [
        "// @generated by tools/cldr_numbers.py from CLDR " + CLDR_VERSION + "; do not edit.",
        "//! CLDR number symbols and patterns ICU4X's data does not carry yet.",
        "#![cfg_attr(rustfmt, rustfmt::skip)]",
        "",
        f'pub(crate) const CLDR_VERSION: &str = "{CLDR_VERSION}";',
        "",
        "/// One locale's values, by name so a change of order cannot swap them.",
        "pub(crate) struct Values {",
        *[f"    pub {f}: &'static str," for f in FIELDS],
        "}",
        "",
        "/// Root's own symbols for a numbering system.",
        "pub(crate) struct RootSymbols {",
        *[f"    pub {f}: &'static str," for f in ROOT_FIELDS],
        "}",
        "",
        "pub(crate) static VALUES: &[Values] = &[",
    ]
    for v in table:
        lines.append(
            "    Values { " + ", ".join(f"{f}: {rust(x)}" for f, x in zip(FIELDS, v, strict=True)) + " },"
        )
    lines += ["];", "", "/// `locale/numberingSystem`, sorted; the index into [`VALUES`].",
              "pub(crate) static ENTRIES: &[(&str, u16)] = &["]
    for key in sorted(entries):
        lines.append(f"    ({rust(key)}, {index[entries[key]]}),")
    lines += [
        "];",
        "",
        "/// Root's decimal, group, minus and plus signs for the numbering systems",
        "/// root defines on its own; a locale without symbols for one uses these.",
        "pub(crate) static ROOT_SYMBOLS: &[(&str, RootSymbols)] = &[",
    ]
    for system, sym in sorted(extra.items()):
        lines.append(
            f"    ({rust(system)}, RootSymbols {{ "
            + ", ".join(f"{f}: {rust(x)}" for f, x in zip(ROOT_FIELDS, sym, strict=True))
            + " }),"
        )
    lines += ["];", ""]
    return "\n".join(lines)


def main():
    pinned = icu4x_version()
    tested = icu4x_cldr_version(pinned)
    if tested != CLDR_VERSION:
        sys.exit(f"ICU4X {pinned} uses CLDR {tested}; this generator is at {CLDR_VERSION}")
    text = generate()
    if "--check" in sys.argv:
        if OUT.read_text() != text:
            sys.exit(f"{OUT.name} is stale: run tools/cldr_numbers.py")
        print(f"{OUT.name} matches CLDR {CLDR_VERSION} (ICU4X {pinned})")
    else:
        OUT.write_text(text)
        print(f"wrote {OUT.name}: CLDR {CLDR_VERSION} for ICU4X {pinned}")


if __name__ == "__main__":
    main()
