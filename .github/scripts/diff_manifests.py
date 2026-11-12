#!/usr/bin/env python3
"""Summarise the differences between two WPT MANIFEST.json files as Markdown."""

import json
import sys

LIMIT = 50


def flatten(manifest):
    entries = {}
    for key, value in manifest.items():
        if key != "items":
            entries[("<top-level>", key)] = value
    for item_type, tree in manifest.get("items", {}).items():
        stack = [((), tree)]
        while stack:
            prefix, node = stack.pop()
            for name, child in node.items():
                path = prefix + (name,)
                if isinstance(child, dict):
                    stack.append((path, child))
                else:
                    entries[(item_type, "/".join(path))] = child
    return entries


def section(title, keys, describe):
    if not keys:
        return
    print(f"\n### {title} ({len(keys)})\n")
    for key in keys[:LIMIT]:
        print(f"- {describe(key)}")
    if len(keys) > LIMIT:
        print(f"- ... and {len(keys) - LIMIT} more")


def main():
    with open(sys.argv[1]) as f:
        python = flatten(json.load(f))
    with open(sys.argv[2]) as f:
        rust = flatten(json.load(f))

    only_python = sorted(python.keys() - rust.keys())
    only_rust = sorted(rust.keys() - python.keys())
    changed = sorted(k for k in python.keys() & rust.keys() if python[k] != rust[k])

    print("## MANIFEST.json differences (Python vs Rust)")
    if not (only_python or only_rust or changed):
        print("\nThe manifests contain the same entries but are not byte-identical (key order or formatting).")
        return

    def name(key):
        return f"`{key[0]}`: `{key[1]}`"

    section("Only in Python manifest", only_python, name)
    section("Only in Rust manifest", only_rust, name)
    section(
        "Entries that differ",
        changed,
        lambda k: f"{name(k)}\n  - Python: `{json.dumps(python[k])}`\n  - Rust: `{json.dumps(rust[k])}`",
    )


if __name__ == "__main__":
    main()
