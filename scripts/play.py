#!/usr/bin/env python3
"""Play the development fixture through the authenticated normal-rule controller."""
import argparse
import collections
import json
import math
import pathlib
import sys
import urllib.error
import urllib.request
import urllib.parse
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[1]
LIVE = set()


def request(path, body=None):
    data = None if body is None else json.dumps(body).encode()
    try:
        with urllib.request.urlopen(urllib.request.Request(BASE + "/dev/" + path, data, HEADERS), timeout=130) as response:
            result = json.load(response)
            if path == 'join':
                LIVE.add(result['player'])
            elif path == 'leave':
                LIVE.remove(body['player'])
            return result
    except urllib.error.HTTPError as error:
        raise RuntimeError(error.read().decode()) from error


def observe(player):
    return request("observe", {"player": player})


def action(player, command):
    return request("action", {"player": player, "action": command})


def report(label, state):
    char = state["character"]
    print(json.dumps({"event": label, "player": state["player"], "position": state["pos"], "character": char}), flush=True)


def navigate(player, destination):
    scene = request("map")
    blocked = {tuple(p) for p in scene["blocked"]}
    state = observe(player)
    start = (int(state["pos"]["x"]), int(state["pos"]["y"]))
    destination = tuple(destination)
    parents = {start: None}
    queue = collections.deque([start])
    while queue and destination not in parents:
        x, y = queue.popleft()
        for point in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= point[0] < scene["width"] and 0 <= point[1] < scene["height"] and point not in blocked and point not in parents:
                parents[point] = (x, y)
                queue.append(point)
    if destination not in parents:
        raise RuntimeError(f"No walkable route to {destination}")
    route = []
    point = destination
    while parents[point] is not None:
        route.append(point)
        point = parents[point]
    route.reverse()
    steps = [{"at_ms": i * 300, "action": {"command": "move", "x": x + .5, "y": y + .5}} for i, (x, y) in enumerate(route)]
    steps.append({"at_ms": len(route) * 300 + 100, "action": {"command": "stop"}})
    frames = request("script", {"player": player, "steps": steps})
    state = frames[-1]
    assert math.hypot(state["pos"]["x"] - destination[0] - .5, state["pos"]["y"] - destination[1] - .5) < .08, state["pos"]
    report("arrived", state)
    return state


def wait(player, milliseconds):
    return request("script", {"player": player, "steps": [{"at_ms": milliseconds}]})[-1]


def play():
    identity = "self-play-" + uuid.uuid4().hex
    player = request("join", {"guid": identity, "world": -1})["player"]
    report("joined", observe(player))
    companion = request("join", {"guid": "companion-" + uuid.uuid4().hex, "world": -1})["player"]
    assert any(p["id"] == companion for p in observe(player)["players"])
    request("leave", {"player": companion})
    state = navigate(player, (12, 137))
    action(player, {'command': 'move', 'x': 12.2, 'y': 137.8})
    wait(player, 500)
    state = wait(player, 9000)
    assert state["character"]["hp"] < 100, "standing in the tutorial gun lane must cause damage"
    report("damaged", state)
    state = action(player, {"command": "use", "slot": 4, "x": state["pos"]["x"], "y": state["pos"]["y"]})
    assert state["character"]["hp"] == 100 and state["character"]["items"][4] == -1
    report("potion consumed", state)
    navigate(player, (12, 103))
    request('script', {'player': player, 'steps': [
        {'at_ms': 0, 'action': {'command': 'shoot', 'angle': -math.pi / 2}},
        {'at_ms': 3000, 'action': {'command': 'stop'}},
    ]})
    navigate(player, (12, 66))
    state = action(player, {"command": "use", "slot": 1, "x": 12.5, "y": 62.5})
    assert state["character"]["mp"] <= 70
    state = wait(player, 1000)
    assert not any(e["kind"] == 0x6b1 for e in state["entities"]), "spell must defeat the 60 HP chicken god"
    report("spell combat", state)
    navigate(player, (12, 45))
    request("script", {"player": player, "steps": [
        {"at_ms": 0, "action": {"command": "shoot", "angle": -math.pi / 2}},
        {"at_ms": 3000, "action": {"command": "stop"}},
    ]})
    navigate(player, (12, 35))
    state = observe(player)
    bag = next(e for e in state["entities"] if e["kind"] == 0x806)
    state = action(player, {"command": "swap", "from": {"object": bag["id"], "slot": 0, "kind": 0xa4f}, "to": {"object": player, "slot": 3, "kind": -1}})
    assert state["character"]["items"][3] == 0xa4f
    report("ring equipped", state)
    character = state["character"]["id"]
    request("leave", {"player": player})
    player = request("join", {"guid": identity, "world": -1})["player"]
    state = observe(player)
    assert state["character"]["id"] == character and state["character"]["items"][3] == 0xa4f
    report("durable reconnect", state)
    action(player, {'command': 'swap', 'from': {'object': player, 'slot': 3, 'kind': 0xa4f}, 'to': {'object': player, 'slot': 5, 'kind': -1}})
    navigate(player, (12, 137))
    action(player, {'command': 'move', 'x': 12.2, 'y': 137.8})
    state = wait(player, 115000)
    assert not state["character"]["alive"], "stationary player must eventually die in the crossfire"
    report("permanent death", state)
    request("leave", {"player": player})
    player = request("join", {"guid": identity, "world": -1})["player"]
    state = observe(player)
    assert state["character"]["id"] != character and state["character"]["items"][3] == -1
    report("replacement character", state)
    request("leave", {"player": player})
    print("self-play passed", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:18923")
    parser.add_argument("--state", type=pathlib.Path, default=ROOT / ".local")
    args = parser.parse_args()
    origin = urllib.parse.urlsplit(args.url)
    if origin.scheme not in {"http", "https"} or origin.hostname not in {"127.0.0.1", "localhost", "::1"} or origin.path not in {"", "/"} or origin.query or origin.fragment or origin.username is not None:
        parser.error("controller URL must be a loopback origin")
    BASE = args.url.rstrip("/")
    HEADERS = {"Authorization": "Bearer " + (args.state / "control.token").read_text().strip(), "Content-Type": "application/json"}
    try:
        play()
    finally:
        for player in list(LIVE):
            request("leave", {"player": player})
