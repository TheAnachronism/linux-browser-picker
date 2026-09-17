#!/usr/bin/env python3
"""Dump and assert the live AT-SPI tree for Browser Picker."""

from __future__ import annotations

import argparse
import subprocess
import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi


def desktop() -> Atspi.Accessible:
    Atspi.init()
    return Atspi.get_desktop(0)


def child_count(node: Atspi.Accessible) -> int:
    try:
        return int(node.get_child_count())
    except Exception:
        return 0


def child_at(node: Atspi.Accessible, index: int) -> Atspi.Accessible | None:
    try:
        return node.get_child_at_index(index)
    except Exception:
        return None


def node_name(node: Atspi.Accessible) -> str:
    try:
        return node.get_name() or ""
    except Exception:
        return ""


def node_role(node: Atspi.Accessible) -> str:
    try:
        return node.get_role_name() or ""
    except Exception:
        return ""


def node_description(node: Atspi.Accessible) -> str:
    try:
        return node.get_description() or ""
    except Exception:
        return ""


def node_states(node: Atspi.Accessible) -> set[str]:
    found: set[str] = set()
    try:
        states = node.get_state_set()
    except Exception:
        return found
    for name in (
        "SELECTED",
        "FOCUSED",
        "FOCUSABLE",
        "SENSITIVE",
        "ENABLED",
        "SHOWING",
        "VISIBLE",
        "CHECKED",
        "EXPANDABLE",
        "EXPANDED",
        "INVALID",
        "REQUIRED",
        "SELECTABLE",
        "ACTIVE",
        "MODAL",
        "EDITABLE",
    ):
        state = getattr(Atspi.StateType, name, None)
        if state is None:
            continue
        try:
            if states.contains(state):
                found.add(name.lower())
        except Exception:
            continue
    return found


def node_attributes(node: Atspi.Accessible) -> dict[str, str]:
    try:
        return dict(node.get_attributes() or {})
    except Exception:
        return {}


def node_shortcuts(node: Atspi.Accessible) -> str:
    attributes = node_attributes(node)
    for key in ("keyshortcuts", "shortcut", "accelerator"):
        value = attributes.get(key)
        if value:
            return value
    return ""


def walk(node: Atspi.Accessible, depth: int = 0):
    if depth > 40:
        return
    yield node, depth
    children = [child_at(node, index) for index in range(child_count(node))]
    for child in children:
        if child is not None:
            yield from walk(child, depth + 1)


def format_node(node: Atspi.Accessible, depth: int) -> str:
    states = ",".join(f"state:{name}" for name in sorted(node_states(node)))
    shortcuts = node_shortcuts(node)
    attr_text = " ".join(
        f"attr:{key}={value!r}" for key, value in sorted(node_attributes(node).items())
    )
    return (
        f"{'  ' * depth}name={node_name(node)!r} role={node_role(node)!r} "
        f"desc={node_description(node)!r} {states} shortcuts={shortcuts!r} {attr_text}"
    )


def dump_tree() -> str:
    lines = []
    root = desktop()
    for index in range(child_count(root)):
        app = child_at(root, index)
        if app is None:
            continue
        for node, depth in walk(app):
            try:
                lines.append(format_node(node, depth))
            except Exception as error:
                lines.append(f"{'  ' * depth}error={error!r}")
    return "\n".join(lines)


def tree_text() -> str:
    return dump_tree()


def require(haystack: str, needle: str) -> None:
    if needle not in haystack:
        raise SystemExit(f"accessibility tree missing {needle!r}\n{haystack}")


def assert_names(required: list[str]) -> None:
    haystack = tree_text()
    for needle in required:
        require(haystack, needle)


def wait_for(needle: str, timeout: float) -> None:
    deadline = time.time() + timeout
    last = ""
    while time.time() < deadline:
        last = tree_text()
        if needle in last:
            return
        time.sleep(0.15)
    raise SystemExit(f"timed out waiting for {needle!r}\n{last}")


def iter_nodes():
    root = desktop()
    for index in range(child_count(root)):
        app = child_at(root, index)
        if app is None:
            continue
        yield from (node for node, _depth in walk(app))


def named_is_focused(name: str) -> bool:
    return any(
        node_name(node) == name and "focused" in node_states(node) for node in iter_nodes()
    )


def component_of(node: Atspi.Accessible):
    try:
        component = node.get_component()
    except Exception:
        component = None
    if component is None:
        try:
            component = Atspi.Component(node)
        except Exception:
            component = None
    return component


def grab_named(name: str) -> None:
    if named_is_focused(name):
        return
    for node in iter_nodes():
        if node_name(node) != name:
            continue
        component = component_of(node)
        if component is None:
            continue
        try:
            if component.grab_focus():
                return
        except Exception:
            continue
    deadline = time.time() + 8
    while time.time() < deadline:
        if named_is_focused(name):
            return
        subprocess.run(["xdotool", "key", "--clearmodifiers", "Tab"], check=False)
        time.sleep(0.05)
    raise SystemExit(f"could not focus {name!r}\n{tree_text()}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("dump", "wait", "assert", "focus"))
    parser.add_argument("values", nargs="*")
    parser.add_argument("--timeout", type=float, default=15.0)
    args = parser.parse_args()
    if args.command == "dump":
        sys.stdout.write(tree_text() + "\n")
        return
    if args.command == "wait":
        if not args.values:
            raise SystemExit("wait requires a name")
        wait_for(args.values[0], args.timeout)
        return
    if args.command == "focus":
        if not args.values:
            raise SystemExit("focus requires a name")
        grab_named(args.values[0])
        return
    assert_names(args.values)


if __name__ == "__main__":
    main()
