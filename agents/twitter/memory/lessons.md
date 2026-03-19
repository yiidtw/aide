# Twitter Agent Lessons

## X compose — type_text vs fill
- Chrome DevTools `fill` does NOT work on X's compose box — Post button stays disabled
- X uses React contentEditable, needs real keyboard events to update internal state
- Use `type_text` (chrome-devtools MCP) instead — it simulates keystroke-by-keystroke input
- `fill` sets value directly, bypasses React event handlers → state out of sync → Post disabled
- Always `press_key Control+A` then `Backspace` to clear before typing new content
