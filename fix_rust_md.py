with open(".agent/learning/rust.md", "r") as f:
    content = f.read()

# The first bad entry
bad_str = """## 2026-08-31 - [Optimize market clearing loop by batching city tax updates]
**Learning:** In heavily nested loops like the market clearing tick loop, repeatedly calling functions that perform map lookups ( calling ) for every matched trade incurs significant overhead due to repeated hashing and  allocation overhead. In this case,  is called on every matched trade but always targets the exact same  (which is invariant inside the  loop).
**Action:** Hoist the accumulation of values into a local variable (e.g. ) inside the loop, and apply it in a single batched function call () after the loop completes to avoid O(N) map lookups, replacing them with a single O(1) update.
"""

content = content.replace(bad_str, "")

with open(".agent/learning/rust.md", "w") as f:
    f.write(content)
