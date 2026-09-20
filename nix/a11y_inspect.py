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


def shortcut_matches(actual: str, expected: str) -> bool:
    if actual == expected:
        return True
    return expected in actual.split()

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
    for candidate in (
        lambda: Atspi.Component(node),
        lambda: node.get_component(),
    ):
        try:
            component = candidate()
        except Exception:
            component = None
        if component is not None:
            return component
    return None

def named_nodes(name: str):
    nodes = [node for node in iter_nodes() if node_name(node) == name]
    return [node for node in nodes if "focusable" in node_states(node)] + [
        node for node in nodes if "focusable" not in node_states(node)
    ]

def window_y(node: Atspi.Accessible) -> int | None:
    component = component_of(node)
    if component is None:
        return None
    try:
        parsed = parse_extents(component.get_extents(Atspi.CoordType.WINDOW))
    except Exception:
        return None
    if parsed is None:
        return None
    return parsed[1]

def scroll_into_view(node: Atspi.Accessible) -> None:
    component = component_of(node)
    if component is None:
        return
    for scroll_type in (
        getattr(Atspi.ScrollType, "TOP", None),
        getattr(Atspi.ScrollType, "ANYWHERE", None),
        getattr(Atspi.ScrollType, "BOTTOM", None),
    ):
        if scroll_type is None:
            continue
        try:
            component.scroll_to(scroll_type)
        except Exception:
            continue
        y = window_y(node)
        if y is not None and 24 < y < 900:
            return
    try:
        component.grab_focus()
    except Exception:
        pass
    for _ in range(10):
        y = window_y(node)
        if y is None:
            break
        if 24 < y < 900:
            return
        key = "Prior" if y <= 24 else "Next"
        subprocess.run(["xdotool", "key", "--clearmodifiers", key], check=False)
        time.sleep(0.12)

def action_of(node: Atspi.Accessible):
    for candidate in (
        lambda: node.get_action(),
        lambda: Atspi.Action(node),
    ):
        try:
            action = candidate()
        except Exception:
            action = None
        if action is None:
            continue
        try:
            if action.get_n_actions() > 0:
                return action
        except Exception:
            continue
    return None

def pointer_nodes(name: str, last: bool = False):
    nodes = named_nodes(name)
    preferred_roles = (
        "toggle button",
        "entry",
        "text",
        "push button",
        "button",
        "check box",
        "combo box",
    )
    ordered = []
    for role in preferred_roles:
        matches = [node for node in nodes if node_role(node) == role]
        ordered.extend(reversed(matches) if last else matches)
    rest = [node for node in nodes if node not in ordered]
    ordered.extend(reversed(rest) if last else rest)
    return ordered

def parse_extents(extents) -> tuple[int, int, int, int] | None:
    if extents is None:
        return None
    if isinstance(extents, (tuple, list)) and len(extents) >= 4:
        x, y, width, height = (int(value) for value in extents[:4])
        return x, y, width, height
    try:
        x = int(extents.x)
        y = int(extents.y)
        width = int(extents.width)
        height = int(extents.height)
    except (AttributeError, TypeError, ValueError):
        return None
    return x, y, width, height

def picker_window_id() -> str:
    output = subprocess.run(
        ["xdotool", "search", "--onlyvisible", "--name", "^Browser Picker$"],
        check=False,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if output:
        return output.split()[-1]
    return subprocess.run(
        ["xdotool", "getactivewindow"],
        check=False,
        capture_output=True,
        text=True,
    ).stdout.strip()

def window_origin(window: str) -> tuple[int, int]:
    if not window:
        return 0, 0
    output = subprocess.run(
        ["xdotool", "getwindowgeometry", "--shell", window],
        check=False,
        capture_output=True,
        text=True,
    ).stdout
    x = y = 0
    for line in output.splitlines():
        if line.startswith("X="):
            x = int(line.split("=", 1)[1])
        elif line.startswith("Y="):
            y = int(line.split("=", 1)[1])
    return x, y

def containing_frame_name(node: Atspi.Accessible) -> str:
    frame = ancestor_with_role(node, "frame") or ancestor_with_role(node, "window")
    return node_name(frame) if frame is not None else ""

def node_click_target(node: Atspi.Accessible, window: str) -> tuple[int, int, bool] | None:
    component = component_of(node)
    if component is None:
        return None
    win_x, win_y = window_origin(window)
    screen = None
    window_rel = None
    try:
        screen = parse_extents(component.get_extents(Atspi.CoordType.SCREEN))
    except Exception:
        screen = None
    try:
        window_rel = parse_extents(component.get_extents(Atspi.CoordType.WINDOW))
    except Exception:
        window_rel = None
    in_main = containing_frame_name(node) in {"Browser Picker", "Picker", "Configuration"}
    chosen: tuple[int, int, int, int, bool] | None = None
    if screen is not None and screen[2] > 0 and screen[3] > 0:
        if screen[1] > 24 or not in_main:
            chosen = (*screen, False)
    if chosen is None and in_main and window_rel is not None and window_rel[2] > 0 and window_rel[3] > 0 and window_rel[1] > 24:
        x, y, width, height = window_rel
        chosen = (x + win_x, y + win_y, width, height, True)
    if chosen is None and screen is not None and screen[2] > 0 and screen[3] > 0:
        chosen = (*screen, False)
    if chosen is None:
        sys.stderr.write(
            "no usable click extents role=" + repr(node_role(node))
            + " name=" + repr(node_name(node))
            + " screen=" + repr(screen)
            + " window=" + repr(window_rel) + "\n"
        )
        return None
    x, y, width, height, relative = chosen
    cx = x + max(width // 2, 1)
    cy = y + max(height // 2, 1)
    sys.stderr.write(
        f"click {node_role(node)} origin={x},{y} size={width}x{height} at={cx},{cy} relative={relative}\n"
    )
    return cx, cy, relative

def click_node(node: Atspi.Accessible) -> bool:
    window = picker_window_id()
    if window:
        subprocess.run(["xdotool", "windowfocus", "--sync", window], check=False)
    for attempt in range(4):
        scroll_into_view(node)
        if attempt:
            subprocess.run(
                ["xdotool", "key", "--clearmodifiers", "Next" if attempt % 2 else "Prior"],
                check=False,
            )
            time.sleep(0.2)
        if click_node_once(node, window):
            return True
    return False

def click_node_once(node: Atspi.Accessible, window: str) -> bool:
    target = node_click_target(node, window)
    if target is None:
        return False
    cx, cy, relative = target
    if relative and window:
        win_x, win_y = window_origin(window)
        subprocess.run(
            [
                "xdotool",
                "mousemove",
                "--window",
                window,
                "--sync",
                str(max(cx - win_x, 1)),
                str(max(cy - win_y, 1)),
                "click",
                "1",
            ],
            check=False,
        )
    else:
        subprocess.run(
            ["xdotool", "mousemove", "--sync", str(cx), str(cy), "click", "1"],
            check=False,
        )
    time.sleep(0.2)
    return True

def do_action_node(node: Atspi.Accessible) -> bool:
    action = action_of(node)
    if action is None:
        return False
    try:
        count = action.get_n_actions()
    except Exception:
        return False
    for index in range(count):
        try:
            name = (action.get_action_name(index) or "").lower()
        except Exception:
            name = ""
        if name and name not in {"click", "press", "toggle", "activate"}:
            continue
        try:
            action.do_action(index)
            time.sleep(0.2)
            return True
        except Exception:
            continue
    return False

def pointer_named(name: str, last: bool = False) -> None:
    for node in pointer_nodes(name, last=last):
        scroll_into_view(node)
        component = component_of(node)
        if component is None:
            continue
        try:
            parsed = parse_extents(component.get_extents(Atspi.CoordType.SCREEN))
        except Exception:
            parsed = None
        if parsed is None or parsed[2] <= 0 or parsed[3] <= 0:
            continue
        x, y, width, height = parsed
        cx = x + max(width // 2, 1)
        cy = y + max(height // 2, 1)
        sys.stderr.write(
            f"click {node_role(node)} origin={x},{y} size={width}x{height} at={cx},{cy} screen=True\n"
        )
        subprocess.run(["xdotool", "mousemove", "--sync", str(cx), str(cy), "click", "1"], check=False)
        time.sleep(0.15)
        return
    grab_named(name, last=last)

def atspi_click(node: Atspi.Accessible) -> bool:
    component = component_of(node)
    if component is None:
        return False
    try:
        parsed = parse_extents(component.get_extents(Atspi.CoordType.WINDOW))
    except Exception:
        parsed = None
    width, height = (parsed[2], parsed[3]) if parsed else (5, 5)
    try:
        component.generate_mouse_event(max(width // 2, 1), max(height // 2, 1), "b1c")
        time.sleep(0.2)
        return True
    except Exception:
        return False

def checkbox_toggled(node: Atspi.Accessible, before: bool) -> bool:
    deadline = time.time() + 0.4
    while time.time() < deadline:
        if ("checked" in node_states(node)) != before:
            return True
        time.sleep(0.05)
    return False

def parent_of(node: Atspi.Accessible) -> Atspi.Accessible | None:
    for candidate in (
        lambda: node.get_parent(),
        lambda: Atspi.Accessible.get_parent(node),
    ):
        try:
            parent = candidate()
        except Exception:
            parent = None
        if parent is not None:
            return parent
    return None

def ancestor_with_role(node: Atspi.Accessible, role: str) -> Atspi.Accessible | None:
    current = node
    for _ in range(16):
        current = parent_of(current)
        if current is None:
            return None
        if node_role(current) == role:
            return current
    return None

def send_key(window: str, *keys: str) -> None:
    command = ["xdotool", "key", "--clearmodifiers"]
    if window:
        command.extend(["--window", window])
    command.extend(keys)
    subprocess.run(command, check=False)

def send_sym(symbol: str) -> None:
    kind = getattr(Atspi.KeySynthType, "SYM", None) or getattr(
        Atspi.KeySynthType, "PRESSRELEASE", Atspi.KeySynthType.STRING
    )
    try:
        Atspi.generate_keyboard_event(0, symbol, kind)
    except Exception:
        Atspi.generate_keyboard_event(0, symbol, Atspi.KeySynthType.STRING)
    time.sleep(0.05)

def focus_accessible(node: Atspi.Accessible) -> bool:
    component = component_of(node)
    if component is None:
        return False
    try:
        grabbed = bool(component.grab_focus())
    except Exception:
        grabbed = False
    time.sleep(0.15)
    return grabbed or "focused" in node_states(node)

def press_space_on_checkbox(
    node: Atspi.Accessible,
    before: bool,
    window: str,
    require_focus: bool = True,
) -> bool:
    if require_focus and "focused" not in node_states(node):
        return False
    send_sym("space")
    if checkbox_toggled(node, before):
        return True
    try:
        Atspi.generate_keyboard_event(0, " ", Atspi.KeySynthType.STRING)
    except Exception:
        pass
    if checkbox_toggled(node, before):
        return True
    send_key(window, "space")
    return checkbox_toggled(node, before)

def toggle_checkbox_via_row(node: Atspi.Accessible, before: bool, window: str) -> bool:
    row = ancestor_with_role(node, "list item")
    if row is None:
        return False
    id_field = None
    for descendant, _depth in walk(row):
        if node_role(descendant) == "text" and node_name(descendant) == "Browser Destination ID":
            id_field = descendant
            break
    if id_field is None:
        return False
    grabbed = focus_accessible(id_field)
    sys.stderr.write(f"destination id grab_focus={grabbed} focused={('focused' in node_states(id_field))}\n")
    send_sym("ISO_Left_Tab")
    time.sleep(0.15)
    if press_space_on_checkbox(node, before, window, require_focus=False):
        return True
    send_key(window, "shift+Tab")
    time.sleep(0.15)
    if press_space_on_checkbox(node, before, window, require_focus=False):
        return True
    send_key(window, "ISO_Left_Tab")
    time.sleep(0.15)
    return press_space_on_checkbox(node, before, window, require_focus=False)

def focused_accessibles() -> list[Atspi.Accessible]:
    return [node for node, _depth in walk(desktop()) if "focused" in node_states(node)]

def toggle_checkbox_by_tab(node: Atspi.Accessible, before: bool, window: str) -> bool:
    target_name = node_name(node)
    for step in range(40):
        focused = focused_accessibles()
        names = [(node_name(item), node_role(item)) for item in focused]
        if any(
            node_name(item) == target_name and node_role(item) == "check box"
            for item in focused
        ):
            return press_space_on_checkbox(node, before, window, require_focus=False)
        send_key(window, "Tab")
        time.sleep(0.08)
    return False

def toggle_checkbox(node: Atspi.Accessible) -> bool:
    before = "checked" in node_states(node)
    window = picker_window_id()
    if window:
        subprocess.run(["xdotool", "windowfocus", "--sync", window], check=False)
    if ancestor_with_role(node, "list item") is not None:
        return toggle_checkbox_by_tab(node, before, window)
    do_action_node(node)
    if checkbox_toggled(node, before):
        return True
    if focus_accessible(node) and press_space_on_checkbox(node, before, window):
        return True
    if click_node_once(node, window) and checkbox_toggled(node, before):
        return True
    return False

def activate_named(name: str, last: bool = False) -> None:
    nodes = pointer_nodes(name, last=last)
    for node in nodes:
        scroll_into_view(node)
        if node_role(node) == "check box":
            if toggle_checkbox(node):
                return
            continue
        action = action_of(node)
        if action is not None:
            try:
                if action.get_n_actions() > 0:
                    action.do_action(0)
                    time.sleep(0.2)
                    return
            except Exception:
                pass
    pointer_named(name, last=last)

def tab_to_named(name: str, last: bool = False, role: str | None = None) -> None:
    window = picker_window_id()
    if window:
        subprocess.run(["xdotool", "windowfocus", "--sync", window], check=False)

    def is_match(node: Atspi.Accessible) -> bool:
        return node_name(node) == name and (role is None or node_role(node) == role)

    total = sum(
        1
        for node, _depth in walk(desktop())
        if is_match(node) and "focusable" in node_states(node)
    )
    if total == 0:
        raise SystemExit(f"missing {name!r} role={role!r}\n{tree_text()}")
    want = total if last else 1
    on_name = False
    hits = 0
    for _ in range(80):
        now = any(is_match(item) for item in focused_accessibles())
        if now and not on_name:
            hits += 1
        on_name = now
        if now and hits >= want:
            return
        send_key(window, "Tab")
        time.sleep(0.08)
    raise SystemExit(f"could not tab to {name!r} role={role!r} last={last}\n{tree_text()}")

def text_iface(node: Atspi.Accessible):
    for candidate in (
        lambda: Atspi.Text(node),
        lambda: node.get_text_iface(),
        lambda: node.get_text(),
    ):
        try:
            iface = candidate()
        except Exception:
            iface = None
        if iface is not None:
            return iface
    return None

def set_named_text(name: str, value: str, last: bool = False, role: str | None = "text") -> None:
    tab_to_named(name, last=last, role=role)
    for node in focused_accessibles():
        if node_name(node) != name:
            continue
        if role is not None and node_role(node) != role:
            continue
        iface = text_iface(node)
        if iface is None:
            continue
        try:
            iface.set_text_contents(value)
        except Exception as error:
            raise SystemExit(f"could not set text on {name!r}: {error}\n{tree_text()}") from error
        time.sleep(0.1)
        return
    raise SystemExit(f"could not set text on focused {name!r}\n{tree_text()}")

def choose_combo(name: str, value: str, last: bool = False) -> None:
    combos = [
        node
        for node in pointer_nodes(name, last=last)
        if node_role(node) in {"combo box", "toggle button"}
    ]
    if not combos:
        raise SystemExit(f"missing combo {name!r}\n{tree_text()}")
    node = combos[0]
    window = picker_window_id()
    if window:
        subprocess.run(["xdotool", "windowfocus", "--sync", window], check=False)
    focused_here = any(
        node_name(item) == name and node_role(item) in {"combo box", "toggle button"}
        for item in focused_accessibles()
    )
    if focused_here:
        do_action_node(node)
        send_key(window, "space")
        send_key(window, "alt+Down")
        send_key(window, "F4")
        time.sleep(0.35)
    else:
        scroll_into_view(node)
        if not (do_action_node(node) or click_node(node)):
            raise SystemExit(f"could not open combo {name!r}\n{tree_text()}")
        time.sleep(0.3)
    def popover_open() -> bool:
        for item, _depth in walk(desktop()):
            if "expanded" in node_states(item) and node_role(item) in {"combo box", "toggle button"}:
                return True
            if node_role(item) == "list item" and not node_name(item):
                if descendant_has_name(item, value) or descendant_has_name(item, name):
                    return True
        return False

    deadline = time.time() + 2
    while time.time() < deadline and not popover_open():
        time.sleep(0.1)
    if not popover_open():
        raise SystemExit(f"combo {name!r} did not open\n{tree_text()}")
    options = [
        item
        for item, _depth in walk(desktop())
        if node_role(item) == "list item"
        and not node_name(item)
        and "showing" in node_states(item)
    ]
    target_index = next(
        (index for index, item in enumerate(options) if descendant_has_name(item, value)),
        None,
    )
    if target_index is not None:
        send_key(window, "Home")
        for _index in range(target_index):
            send_key(window, "Down")
    elif name == "Show Picker" and value == "Open automatically":
        send_key(window, "Down")
    elif value == "Open automatically":
        send_key(window, "Home")
    elif name == "Show Picker":
        send_key(window, "End")
    else:
        send_key(window, "End")
    time.sleep(0.2)
    send_key(window, "Return")
    time.sleep(0.3)
    deadline = time.time() + 4
    last_tree = ""
    while time.time() < deadline:
        last_tree = tree_text()
        actual = node_attributes(node).get("valuetext") or node_name(node)
        if actual == value:
            return
        if combo_has_value(name, value, last=last):
            return
        time.sleep(0.15)
    raise SystemExit(f"could not choose {value!r} from {name!r}\n{last_tree}")

def combo_has_value(name: str, value: str, last: bool = False) -> bool:
    combos = [
        item
        for item in iter_nodes()
        if node_role(item) == "combo box"
        and "showing" in node_states(item)
        and (
            node_name(item) == name
            or node_attributes(item).get("valuetext") == name
            or node_attributes(item).get("valuetext") == value
        )
    ]
    if not combos:
        return False
    item = combos[-1] if last else combos[0]
    actual = node_attributes(item).get("valuetext") or node_name(item)
    return actual == value


def descendant_has_name(node: Atspi.Accessible, value: str, depth: int = 0) -> bool:
    if depth > 8:
        return False
    if node_name(node) == value:
        return True
    for index in range(child_count(node)):
        child = child_at(node, index)
        if child is not None and descendant_has_name(child, value, depth + 1):
            return True
    return False

def grab_named(name: str, last: bool = False) -> None:
    if not last and named_is_focused(name):
        return
    candidates = pointer_nodes(name, last=last)
    if not candidates:
        nodes = named_nodes(name)
        candidates = list(reversed(nodes)) if last else nodes
    window = picker_window_id()
    if window:
        subprocess.run(["xdotool", "windowfocus", "--sync", window], check=False)
    for node in candidates:
        scroll_into_view(node)
        if focus_accessible(node) or named_is_focused(name):
            return
    if last:
        raise SystemExit(f"could not focus last {name!r}\n{tree_text()}")
    deadline = time.time() + 8
    while time.time() < deadline:
        if named_is_focused(name):
            return
        send_key(window, "Tab")
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
    description: str | None = None,
    showing: bool | None = None,
    last: bool = False,
) -> None:
    deadline = time.time() + 8
    last_tree = ""
    from_last = last
    while time.time() < deadline:
        haystack = tree_text()
        last_tree = haystack
        found_showing = False
        candidates = [
            node
            for node in iter_nodes()
            if node_name(node) == name and (role is None or node_role(node) == role)
        ]
        if from_last:
            candidates = list(reversed(candidates))
        for node in candidates[:1] if from_last else candidates:
            if description is not None and node_description(node) != description:
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
            if shortcut is not None and not shortcut_matches(node_shortcuts(node), shortcut):
                continue
            is_showing = "showing" in states or "visible" in states
            if showing is False:
                if is_showing:
                    found_showing = True
                    continue
                return
            if showing is True and not is_showing:
                continue
            return
        if showing is False and not found_showing:
            return
        time.sleep(0.15)
    raise SystemExit(
        "no accessible bound "
        f"name={name!r} role={role!r} enabled={enabled!r} "
        f"selected={selected} focused={focused} shortcut={shortcut!r} "
        f"description={description!r} showing={showing!r} last={from_last!r}\n{last_tree}"
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
            "combo",
            "tab-to",
            "set-text",
        ),
    )
    parser.add_argument("values", nargs="*")
    parser.add_argument("--timeout", type=float, default=15.0)
    parser.add_argument("--role")
    parser.add_argument("--shortcut")
    parser.add_argument("--description")
    parser.add_argument("--selected", action="store_true")
    parser.add_argument("--focused", action="store_true")
    parser.add_argument("--showing", dest="node_showing", action="store_true")
    parser.add_argument("--hidden", dest="node_hidden", action="store_true")
    parser.add_argument("--enabled", dest="node_enabled", action="store_true")
    parser.add_argument("--disabled", dest="node_disabled", action="store_true")
    parser.add_argument("--last", action="store_true")
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
        grab_named(args.values[0], last=args.last)
        return
    if args.command == "pointer":
        if not args.values:
            raise SystemExit("pointer requires a name")
        pointer_named(args.values[0])
        return
    if args.command == "activate":
        if not args.values:
            raise SystemExit("activate requires a name")
        activate_named(args.values[0], last=args.last)
        return
    if args.command == "tab-to":
        if not args.values:
            raise SystemExit("tab-to requires a name")
        tab_to_named(args.values[0], last=args.last, role=args.role)
        return
    if args.command == "set-text":
        if len(args.values) != 2:
            raise SystemExit("set-text requires a name and a value")
        set_named_text(args.values[0], args.values[1], last=args.last, role=args.role or "text")
        return
    if args.command == "combo":
        if len(args.values) != 2:
            raise SystemExit("combo requires a current name and a value")
        choose_combo(args.values[0], args.values[1], last=args.last)
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
        if args.node_showing and args.node_hidden:
            raise SystemExit("node cannot be both showing and hidden")
        enabled = True if args.node_enabled else False if args.node_disabled else None
        showing = True if args.node_showing else False if args.node_hidden else None
        require_named_binding(
            args.values[0],
            args.role,
            enabled,
            args.selected,
            args.focused,
            args.shortcut,
            args.description,
            showing,
            args.last,
        )
        return
    assert_names(args.values)


if __name__ == "__main__":
    main()
