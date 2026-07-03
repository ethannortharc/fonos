# desktop-sprite — panda companion simulator

Standalone demo for the fonos desktop-companion concept (issue #25): a cute
panda in a transparent always-on-top window that lives on your desktop —
sitting on a bamboo pole, blinking, nibbling a bamboo shoot, waving when
clicked, and curling into a ball to roll (wheel-accurate rotation) to a new
spot along the bottom of the screen.

No fonos backend involved — the point is to prove the window + character +
behavior layers are independent renderers. In the real feature, the behavior
brain here (`main.swift`'s wander/nibble schedule) is replaced by the STS
pipeline's TurnEvent stream (#24): listening / thinking / speaking states.

## Run

```
./run.sh          # compiles main.swift and launches the panda
```

- Click the panda → it waves (and loves you back)
- Right-click → quits
- It wanders on its own every few seconds

## Files

- `panda.html` — the character: pure SVG/CSS/JS, two forms (sit / ball),
  state machine driven from native via `setState()` / `doNibble()` / `doWave()`
- `main.swift` — transparent borderless floating NSWindow + WKWebView +
  the behavior brain (window-position animation along the screen bottom)
