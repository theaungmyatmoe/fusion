# Vendored sqlite-vec 0.1.7-alpha.2 (Fusion)

Upstream: https://github.com/asg017/sqlite-vec / crates.io `sqlite-vec`.

## Why vendored

crates.io `0.1.7-alpha.2` fails to compile on **musl** (Linux static / Termux)
because `sqlite-vec.c` redefines `uint*_t` using BSD-only `u_int*_t`.

crates.io `0.1.10-alpha.4` fixes that but ships an incomplete package
(missing `sqlite-vec-diskann.c` while enabling DISKANN by default).

This tree is 0.1.7-alpha.2 with only the musl typedef block removed
(same fix as upstream PR #199 / main).

Do not bump blindly without re-checking musl + `sqlite3_vec_init` ABI.
