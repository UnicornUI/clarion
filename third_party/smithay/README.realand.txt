# Clarion Smithay patch record

Upstream: https://github.com/Smithay/smithay
Revision: `2923f2248508307687de4e1d84e4d208b937ad42` (2026-10-18)
Version: 0.7.0
License: MIT (upstream `LICENSE.txt` retained)

All tracked files were copied from the pinned Cargo checkout. The local copy
lives outside cargo vendor's output directory so release packaging cannot
delete it. Nix sources include this directory. `Cargo.toml` at the repository
root applies this local dependency through `[patch]`; the shared Cargo
checkout is untouched.

The pin contains the upstream `Rectangle::subtract_rects_many_in_place` fix
([Smithay@2923f2248](https://github.com/Smithay/smithay/commit/2923f2248508307687de4e1d84e4d208b937ad42),
closing upstream issue #2195), so no backport is needed. The vendored tree is
that revision plus `clarion-smithay.patch` and the Linux-relevant upstream
backports recorded in `upstream-backports.patch`.

## Patches

| Patch | Files | Upstream |
| --- | --- | --- |
| `clarion-smithay.patch` | eight files, see below | Local downstream work, not submitted |
| `upstream-backports.patch` | `src/` changes listed below | Backports from upstream `65aa86fa` |

| Area | Files | Change |
| --- | --- | --- |
| HDR / color signaling | `backend/drm/surface/{atomic,mod}.rs`, `backend/drm/compositor/mod.rs`, `backend/renderer/utils/wayland.rs` | Stage connector and CRTC color properties in the same atomic commit as the framebuffer, expose `has_pending_frames`, and invalidate imported textures when a surface commits a new color description. |
| Deferred render retry | `backend/renderer/mod.rs`, `backend/drm/compositor/mod.rs` | `Renderer::is_render_deferred` lets a retry keep output allocations instead of resetting the swapchain on every asynchronous target preparation. |
| Buffer ages | `backend/allocator/swapchain.rs` | `reset_buffer_ages` clears age in place instead of replacing shared slots, which released buffers that were still held. |
| Seat handoff | `backend/session/libseat.rs`, `backend/session/libseat/pause.rs` | Acknowledge `SeatEvent::Disable` only after the pause callback released HDR signaling, without holding the seat borrow across the callback, with a one-second warning and error logging. |

These are Clarion product requirements (HDR, seat switching, deferred retry),
so they stay a downstream fork. The winit backend follows upstream as-is:
earlier local beta-adapter changes are gone because the pinned revision
already carries them.

## Upstream backports

The following Linux-relevant upstream commits from the niri-edge pin
`65aa86fa` are applied on top of the Clarion patch set. They are recorded in
`upstream-backports.patch` (the `CHANGELOG.md` entry from `76118ad7` is also
present in the vendored tree).

| Upstream commit | Area | Change |
| --- | --- | --- |
| `65aa86fa` | input/tablet | Send the old focus its frame before `proximity_in`, so clients owning both targets do not lose the new focus. |
| `f581531d` | GLES renderer | Fix the y-inverted texture matrix translation (`v` → `1 - v`). |
| `52a47d10` | wayland/drm_syncobj | Destroy imported timeline handles on drop, fixing syncobj leaks across timeline/device replacement. |
| `76118ad7` | libinput | Process already-queued events before polling, so device add/remove events are not delayed until the fd becomes readable. |
| `85de6d6e` | input/keyboard | Require `Send` on `KeyboardGrab` and drop the unsafe `Send` impl for `KbdInternal`. |
| `8fd72710`, `f2e37dba`, `bca0e022`, `cdf10648`, `ee5f7873`, `945f3309`, `6b37edc0` | input/keyboard, virtual-keyboard | Reference-count `KeymapFile`, expose its id, add `keymap_file()` / `set_keymap_from_keymap_file()`, and make the virtual keyboard use it. |

Not backported because they are platform/dependency-specific or unused by
Clarion: OpenBSD build fixes (`590862c7`, `c7a26ade`), the winit/x11rb beta
update (`905ecda4`), the `egl` `repr(C)` cleanup (`263e57a2`), and the
Xwayland host-environment spawn variant (`261a56a5`, Clarion uses
`xwayland-satellite`).

Damage-tracker regression coverage lives in Clarion's
`src/render_helpers/damage_region.rs`: subtract a region from itself, and
compare seeded subtraction against an independent per-pixel coverage oracle.
The upstream regression test for the loop fix is in `src/utils/geometry.rs`
and can be run from the vendored workspace with:

```sh
cd third_party/smithay
cargo test --lib --no-default-features rectangle_subtract_rects_many_checks_all_rects
```

The default feature set also builds the winit backend against whatever beta
the standalone workspace resolves, which can differ from the pinned workspace
lock; `--no-default-features` keeps that unrelated failure out of the way.

## Replaying

```sh
base=$(mktemp -d)
git -C ~/.cargo/git/checkouts/smithay-*/2923f22 archive HEAD | tar x -C "$base"
cd "$base"
git apply third_party/smithay/clarion-smithay.patch
```

## Upgrading

1. Move both `smithay` and `smithay-drm-extras` workspace pins to the new
   revision, then run `cargo fetch` to refresh `Cargo.lock`.
2. Drop in the new pinned sources and replay `clarion-smithay.patch`. If a hunk
   fails, port the change by hand, refresh the patch, and re-verify that it
   reproduces the vendored tree.
3. Run `cargo check --locked`, the damage-region tests, and a session that
   changes output scale/rotation, swaps seats, and toggles HDR.
4. If a change lands upstream, delete it from the patch. When the patch is
   empty, remove the vendored directory and the `[patch]` entry.
