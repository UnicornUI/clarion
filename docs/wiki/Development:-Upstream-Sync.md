# Upstream sync (niri-edge → Clarion)

This page records the state of the manual upstream sync between Clarion and
[niri-edge](https://github.com/123llyrr/niri-edge), so the next sync can start
from facts rather than re-discovering the same gaps.

## Current pins

| Item | Value |
| --- | --- |
| Clarion branch | `dock/folder-pin` |
| Last sync record | `f61c759` (2026-11-17) |
| niri-edge reference | `7c0b45e2` (2026-11-11) |
| Shared niri baseline | `45739bc` |
| Smithay pin | `65aa86fa5236e41775c313408102a5d5c2d1e2ca` (2026-11-04) |
| Smithay local patch | `third_party/smithay/clarion-smithay.patch` |

Clarion's git history was rewritten; niri/niri-edge commit hashes are not
ancestors of Clarion commits. Do **not** try to `git merge` niri-edge directly.
Sync is a manual, code-level port plus this ledger.

## What has already been ported

The following areas were ported and verified in the current branch. Treat them
as done unless new upstream work changes the same code.

### Input and bindings

- Press/release `BoundAction` model, release actions for keys, mouse buttons and
  tablet stylus buttons.
- Pass-through binds, release-only pass-through default, bare modifier / `Mod`
  triggers, press-order-independent modifier-only binds.
- Bind lookup with modified keysym, raw keysym and raw ASCII fallback; XF86 key
  names; NUL-byte key names rejected instead of panicking.
- Hardcoded overview/launcher binds; MRU commit on `ForwardAndHandle`.
- Screenshot UI binds behave like normal binds: modifier-sensitive, no repeat,
  screenshot binds take precedence over configured binds.
- VT switch / power-off hardcoded binds do not repeat; LEDs refresh after keymap
  changes.
- `change-vt` action, GlobalShortcuts `org.gnome.Shell` accelerator methods.
- `InputTime` propagation from keyboard, pointer, scroll, tablet and switch
  events into `do_action`; GNOME activation actions use the real event time.
- `toggle-touchpad` action and `TouchpadToggled` IPC event.
- `touchpad.pinch-sensitivity` + window rule `pinch-sensitivity`.
- `restore-window` action (by id, or most recent on an output's active
  workspace).

### ext-image-copy-capture

- Output, toplevel and pointer-cursor sources; capture sessions, frames, DMABUF
  and SHM buffers.
- `is-window-cast-target`, IPC cast reporting from the second frame, minimized
  window sessions pausing frames and reporting inactive.
- `block-out-from screen-capture` privacy, output transforms, resize/close
  lifecycle and session cleanup.
- Vulkan DMA-BUF constraints using the output's actual renderer; the old
  `with_primary_renderer` GLES-only restriction was removed.

### Window, output and protocol behavior

- Set/unset fullscreen, maximized-to-edges and windowed-fullscreen actions.
- Scrolling layout `tile_pos_in_workspace_view`, including stationary view
  offsets.
- VRR `fullscreen-only` window rule; `fullscreen-backdrop-color`.
- IME popups at full opacity; layer-shell keyboard-focus fix; DnD icon output
  overlap; frame callbacks skipped while monitors are inactive.
- IPC serde defaults for optional action fields.
- a11y disappearance monitor ignores unrelated well-known names.
- `clarion-session` imports the login environment with
  `dbus-update-activation-environment --systemd --all`.

### Smithay

Smithay is pinned to the same revision as niri-edge (`65aa86fa`), so the
Linux-relevant upstream fixes between Clarion's old pin (`2923f224`) and this
one are included. `clarion-smithay.patch` is now only Clarion's local work:
HDR/color properties staged in the same KMS commit, deferred-render retry,
buffer-age reset without releasing held slots, and seat-handoff pause/destroy
ordering. See `third_party/smithay/README.realand.txt`.

## Intentional differences (do not port blindly)

| Area | Clarion | Why |
| --- | --- | --- |
| Minimize / Genie configuration | Driven by the Shell and `SetMinimizeTarget`; no `window-minimize` / `window-minimize-effect` KDL knobs | Clarion's shell owns the minimize presentation and target geometry |
| IPC capabilities | `Capabilities(Vec<String>)` | Clarion exposes capabilities as a string list |
| Animation targets | `SetMinimizeTarget` instead of `SetWindowAnimationTargets` | Same reason as minimize |
| Dynamic cast target | Clarion-specific target naming/management | Existing portal/dynamic-cast design |
| PipeWire SHM fallback / `disable-pipewire-dmabuf` | Not implemented | PipeWire screencast is DMA-BUF-only; ext-image-copy already provides SHM capture. Add a PipeWire SHM path only if a concrete VM/remote/driver requirement appears |

Clarion-only features are not upstream gaps: native Vulkan composition,
HDR/scRGB color management, `clarion-core`, the native Shell/Dock, and the
custom settings/performance tooling.

## Remaining known gaps

1. **`debug { disable-pipewire-dmabuf; }`** — blocked on adding a PipeWire SHM
   screencast path. This is intentionally not a priority unless a real use case
   appears.
2. **New niri-edge commits after `7c0b45e2`** — none in the local reference at
   the time of this record. Re-check on the next sync.

`InputTime` propagation was completed after the first sync record; it is no
longer a gap.

## How to sync next time

### 1. Update and enumerate the reference

```sh
cd ../niri-edge
git fetch --all --tags
git log --oneline 7c0b45e2..HEAD
```

Record the new reference HEAD in this page before touching code. If upstream
rebased or force-pushed, use the previous reference HEAD as the base and compare
code, not hashes.

### 2. Classify commits

For every commit decide one of:

- **already ported** — point to the Clarion code or commit;
- **missing** — no equivalent code;
- **partial** — some pieces exist;
- **N/A** — docs, CI, tests, platform-specific (OpenBSD, X11 host-env spawn),
  or an intentional Clarion divergence.

Verify in code. Useful places:

- `clarion-config/src/binds.rs`, `clarion-config/src/window_rule.rs`,
  `clarion-config/src/input.rs`, `clarion-config/src/debug.rs`
- `clarion-ipc/src/lib.rs`, `clarion-ipc/src/state.rs`
- `src/input/mod.rs`, `src/compositor.rs`
- `src/handlers/`, `src/protocols/`, `src/layout/`, `src/window/`
- `src/tests/`
- `third_party/smithay/` for Smithay changes

Do not trust commit titles alone: several upstream commits were squashed or
re-implemented in Clarion under different names.

### 3. Port with tests

- Add a failing test first for behavior changes (parser, event stream, input,
  capture, layout).
- Run `cargo test --workspace` before committing.
- Keep one feature per commit; update this ledger in the same branch.
- If the port touches a Clarion-only area (Vulkan, Shell, HDR), write a
  Clarion-specific test rather than copying an upstream test verbatim.

### 4. Update the Smithay pin

If niri-edge moves Smithay:

```sh
# In a checkout of Smithay at the new revision:
git archive <new-rev> | tar x -C third_party/smithay
cd third_party/smithay
git apply clarion-smithay.patch   # or port rejected hunks by hand
```

Then:

1. Update the `Revision:` line in
   `third_party/smithay/README.realand.txt`.
2. Regenerate `clarion-smithay.patch` if hunks changed; verify it applies to a
   pristine checkout of the new revision.
3. Update `Cargo.lock` (`cargo update -p smithay` or a normal build).
4. Watch for dependency-coupled API changes. The `2923f224 -> 65aa86fa` move
   needed one adaptation: winit beta.4 changed the window-creation error type,
   so `src/backend/winit/native_window.rs` stores it as `String`.
5. Run the full test matrix and a live session check.

### 5. Commit and record

- Commit per area, with the upstream commit hashes in the body when useful.
- Update the **Current pins** and **Remaining known gaps** sections here.
- If a gap is intentionally skipped, record the reason, not just “not ported”.

## Verification checklist

```sh
cargo check --workspace --locked
cargo test --workspace
cargo test -p clarion-config --test schema
rustfmt +nightly --edition 2021 --check <touched files>
```

Smithay local patch sanity check (from the vendored tree):

```sh
cd third_party/smithay
cargo test --lib --no-default-features rectangle_subtract_rects_many_checks_all_rects
```

Live session smoke checks:

```sh
clarion --version
clarion msg version
wayland-info | grep -E 'ext_image_copy_capture|image_capture_source_manager'
clarion msg casts
wl-mirror -b extcopy-dmabuf <output>
wl-mirror -b extcopy-shm <output>
```

Also verify: `restore-window`, `toggle-touchpad` + event stream, pinch
sensitivity, screenshots, wlr-screencopy/PipeWire casts, HDR/fullscreen toggle,
VT switch and lock/unlock.

## Ledger

| Date | Reference | Smithay | Branch HEAD | Notes |
| --- | --- | --- | --- | --- |
| 2026-11-17 | niri-edge `7c0b45e2` | `65aa86fa` | `f61c759` | 105 upstream commits classified; ext-image-copy, input/binds, InputTime, a11y, touchpad, pinch, restore-window and Smithay pin done. Only `disable-pipewire-dmabuf` remains intentionally open. |
