// Pointer Lock bridge for the fly camera (ticket 65).
//
// The wasm build cannot request pointer lock through eframe, so this classic
// script exposes a small global that the Rust side calls through
// `js_sys::Reflect`. It owns the browser's pointerlockchange state and
// accumulates raw `movementX/Y` while locked; `takeDelta()` drains the
// accumulator exactly once per frame.
//
// Best-effort by contract: a refused request, a missing API, or a dropped lock
// never throws into the app and never blocks fly mode (drag-look remains).
(function () {
  "use strict";

  var canvas = null;
  var locked = false;
  var wasLocked = false;
  var dx = 0;
  var dy = 0;

  function onPointerLockChange() {
    var element = document.pointerLockElement;
    locked = !!(canvas && element === canvas);
    if (locked) {
      wasLocked = true;
    }
  }

  function onPointerMove(event) {
    if (!locked) {
      return;
    }
    dx += event.movementX || 0;
    dy += event.movementY || 0;
  }

  function request(target) {
    canvas = target || document.getElementById("vfe-canvas");
    if (!canvas || typeof canvas.requestPointerLock !== "function") {
      return false;
    }
    try {
      var result = canvas.requestPointerLock();
      // Newer browsers return a promise; swallow a refusal so the app stays in
      // drag-look instead of surfacing an unhandled rejection.
      if (result && typeof result.catch === "function") {
        result.catch(function () {});
      }
      return true;
    } catch (error) {
      return false;
    }
  }

  function release() {
    dx = 0;
    dy = 0;
    locked = false;
    // An app-initiated release is not a lost lock; a later re-entry must not
    // mistake a refused request for a drop.
    wasLocked = false;
    if (document.pointerLockElement && typeof document.exitPointerLock === "function") {
      try {
        document.exitPointerLock();
      } catch (error) {
        // Already unlocked or unsupported: nothing to do.
      }
    }
  }

  function takeDelta() {
    var delta = [dx, dy];
    dx = 0;
    dy = 0;
    return delta;
  }

  function isLocked() {
    return locked;
  }

  // True once after a successful lock is later dropped (typically browser
  // Esc). The initial "never locked" failure is not a loss.
  function lockLost() {
    var lost = wasLocked && !locked;
    if (lost) {
      wasLocked = false;
    }
    return lost;
  }

  document.addEventListener("pointerlockchange", onPointerLockChange);
  if (window.PointerEvent) {
    document.addEventListener("pointermove", onPointerMove);
  } else {
    // Older engines without Pointer Events still deliver raw mouse motion.
    document.addEventListener("mousemove", onPointerMove);
  }

  window.vfePointerLock = {
    request: request,
    release: release,
    takeDelta: takeDelta,
    isLocked: isLocked,
    lockLost: lockLost
  };
})();
