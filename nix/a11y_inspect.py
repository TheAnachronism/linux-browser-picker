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
        "DISABLED",
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

def named_nodes(name: str):
    nodes = [node for node in iter_nodes() if node_name(node) == name]
    return [node for node in nodes if "focusable" in node_states(node)] + [
        node for node in nodes if "focusable" not in node_states(node)
    ]

def scroll_into_view(node: Atspi.Accessible) -> None:
    component = component_of(node)
    if component is None:
        return
    try:
        component.scroll_to(Atspi.ScrollType.ANYWHERE)
    except Exception:
        try:
            component.scroll_to_point(Atspi.CoordType.WINDOW, 0, 0)
        except Exception:
            return

def action_of(node: Atspi.Accessible):
    try:
        action = node.get_action()
    except Exception:
        action = None
    if action is None:
        try:
            action = Atspi.Action(node)
        except Exception:
            action = None
    return action

def pointer_nodes(name: str):
    nodes = named_nodes(name)
    preferred_roles = (
        "toggle button",
        "entry",
        "text",
        "push button",
        "check box",
        "combo box",
    )
    ordered = []
    for role in preferred_roles:
        ordered.extend(node for node in nodes if node_role(node) == role)
    ordered.extend(node for node in nodes if node not in ordered)
    return ordered

def pointer_named(name: str) -> None:
    for node in pointer_nodes(name):
        scroll_into_view(node)
        component = component_of(node)
        if component is None:
            continue
        try:
            extents = component.get_extents(Atspi.CoordType.SCREEN)
        except Exception:
            continue
        if getattr(extents, "width", 0) <= 0 or getattr(extents, "height", 0) <= 0:
            continue
        x = extents.x + max(extents.width // 2, 1)
        y = extents.y + max(extents.height // 2, 1)
        subprocess.run(["xdotool", "mousemove", "--sync", str(x), str(y)], check=False)
        subprocess.run(["xdotool", "click", "1"], check=False)
        time.sleep(0.15)
        return
    grab_named(name)

def activate_named(name: str) -> None:
    for node in pointer_nodes(name):
        scroll_into_view(node)
        action = action_of(node)
        if action is None:
            continue
        try:
            if action.get_n_actions() > 0:
                action.do_action(0)
                time.sleep(0.2)
                return
        except Exception:
            continue
    pointer_named(name)

def grab_named(name: str) -> None:
    if named_is_focused(name):
        return
    for node in named_nodes(name):
        scroll_into_view(node)
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

def require_named_state(name: str, enabled: bool) -> None:
    for node in pointer_nodes(name):
        states = node_states(node)
        is_enabled = "enabled" in states or "sensitive" in states
        if enabled and not is_enabled:
            raise SystemExit(f"{name!r} is disabled: {sorted(states)}\n{tree_text()}")
        if not enabled and is_enabled:
            raise SystemExit(f"{name!r} is enabled: {sorted(states)}\n{tree_text()}")
        return
    raise SystemExit(f"missing {name!r}\n{tree_text()}")

def require_named_binding(
    name: str,
    role: str | None,
    enabled: bool | None,
    selected: bool,
    focused: bool,
    shortcut: str | None,
) -> None:
    haystack = tree_text()
    for node in iter_nodes():
        if node_name(node) != name:
            continue
        if role is not None and node_role(node) != role:
            continue
        states = node_states(node)
        is_enabled = "enabled" in states or "sensitive" in states
        if enabled is True and not is_enabled:
            continue
        if enabled is False and is_enabled:
            continue
        if selected and "selected" not in states:
            continue
        if focused and "focused" not in states:
            continue
        if shortcut is not None and node_shortcuts(node) != shortcut:
            continue
        return
    raise SystemExit(
        "no accessible bound "
        f"name={name!r} role={role!r} enabled={enabled!r} "
        f"selected={selected} focused={focused} shortcut={shortcut!r}\n{haystack}"
    )


def require_named_checked(name: str) -> None:
    for node in pointer_nodes(name):
        states = node_states(node)
        if "checked" not in states:
            raise SystemExit(f"{name!r} is unchecked: {sorted(states)}\n{tree_text()}")
        return
    raise SystemExit(f"missing {name!r}\n{tree_text()}")

def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "command",
        choices=(
            "dump",
            "wait",
            "assert",
            "focus",
            "pointer",
            "activate",
            "enabled",
            "disabled",
            "checked",
            "node",
        ),
    )
    parser.add_argument("values", nargs="*")
    parser.add_argument("--timeout", type=float, default=15.0)
    parser.add_argument("--role")
    parser.add_argument("--shortcut")
    parser.add_argument("--selected", action="store_true")
    parser.add_argument("--focused", action="store_true")
    parser.add_argument("--enabled", dest="node_enabled", action="store_true")
    parser.add_argument("--disabled", dest="node_disabled", action="store_true")
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
    if args.command == "pointer":
        if not args.values:
            raise SystemExit("pointer requires a name")
        pointer_named(args.values[0])
        return
    if args.command == "activate":
        if not args.values:
            raise SystemExit("activate requires a name")
        activate_named(args.values[0])
        return
    if args.command in {"enabled", "disabled"}:
        if not args.values:
            raise SystemExit(f"{args.command} requires a name")
        require_named_state(args.values[0], args.command == "enabled")
        return
    if args.command == "checked":
        if not args.values:
            raise SystemExit("checked requires a name")
        require_named_checked(args.values[0])
        return
    if args.command == "node":
        if not args.values:
            raise SystemExit("node requires a name")
        if args.node_enabled and args.node_disabled:
            raise SystemExit("node cannot be both enabled and disabled")
        enabled = True if args.node_enabled else False if args.node_disabled else None
        require_named_binding(
            args.values[0],
            args.role,
            enabled,
            args.selected,
            args.focused,
            args.shortcut,
        )
        return
    assert_names(args.values)


if __name__ == "__main__":
    main()
