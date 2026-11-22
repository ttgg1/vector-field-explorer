// WebGPU-unavailable panel helper (tickets 54 and 236).
//
// The Rust wasm entry point (src/main.rs) owns the authoritative check:
// it reads navigator.gpu and awaits requestAdapter() before eframe's
// WebRunner can start. This classic script only toggles the static panel,
// picks the browser-specific support link, installs the Retry and preview-back
// handlers, and reveals a static exercise preview chosen from the page query.
// It never starts the app, never substitutes a renderer, and never decides
// availability on its own. Retry is inert only if this script fails to load;
// the Rust gate still reveals the panel, with the generic recovery view, in
// that case.
//
// Recognized exercise links (documented in docs/CODEBASE.md):
//   ?exercise=ring | ?exercise=shell | ?exercise=plane | ?exercise=wire
// An optional `preview=1` is accepted as an explicit marker; it does not change
// which exercise is selected. Unknown ids fall back to the generic view.
(function () {
  "use strict";

  // Keep this list in sync with the [data-exercise] blocks in index.html.
  var EXERCISES = ["ring", "shell", "plane", "wire"];

  var BROWSERS = {
    edge: {
      name: "Edge",
      label: "Edge WebGPU help",
      url: "https://learn.microsoft.com/en-us/microsoft-edge/web-platform/webgpu",
    },
    opera: {
      name: "Opera",
      label: "Opera support",
      url: "https://help.opera.com/",
    },
    firefox: {
      name: "Firefox",
      label: "Mozilla support",
      url: "https://support.mozilla.org/en-US/",
    },
    chrome: {
      name: "Chrome",
      label: "Chrome WebGPU help",
      url: "https://developer.chrome.com/docs/web-platform/webgpu",
    },
    safari: {
      name: "Safari",
      label: "Safari support",
      url: "https://developer.apple.com/safari/",
    },
    other: {
      name: "your browser",
      label: "your browser's WebGPU support",
      url: "https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API#browser_compatibility",
    },
  };

  function detectBrowser() {
    var ua = (navigator && navigator.userAgent) || "";
    if (/Edg\//.test(ua)) {
      return BROWSERS.edge;
    }
    if (/OPR\//.test(ua)) {
      return BROWSERS.opera;
    }
    if (/Firefox\//.test(ua)) {
      return BROWSERS.firefox;
    }
    if (/Chrome\//.test(ua) || /Chromium\//.test(ua)) {
      return BROWSERS.chrome;
    }
    if (/Safari\//.test(ua)) {
      return BROWSERS.safari;
    }
    return BROWSERS.other;
  }

  function updateBrowserHelp() {
    var link = document.getElementById("webgpu-browser-help");
    if (!link) {
      return;
    }
    var browser = detectBrowser();
    link.href = browser.url;
    link.textContent = browser.label;
  }

  function parseExercise(search) {
    var query = String(search || "");
    if (query.charAt(0) === "?") {
      query = query.slice(1);
    }
    var match = /(?:^|&)exercise=([^&]*)/.exec(query);
    if (!match) {
      return null;
    }
    var id = match[1].replace(/\+/g, " ");
    try {
      id = decodeURIComponent(id);
    } catch (error) {
      return null;
    }
    id = id.trim().toLowerCase();
    return EXERCISES.indexOf(id) === -1 ? null : id;
  }

  function exerciseNodes() {
    return document.querySelectorAll("[data-exercise]");
  }

  function exerciseNode(id) {
    var nodes = exerciseNodes();
    for (var i = 0; i < nodes.length; i += 1) {
      if (nodes[i].getAttribute("data-exercise") === id) {
        return nodes[i];
      }
    }
    return null;
  }

  function hideExerciseNodes() {
    var nodes = exerciseNodes();
    for (var i = 0; i < nodes.length; i += 1) {
      nodes[i].hidden = true;
    }
  }

  function showGeneric() {
    var preview = document.getElementById("webgpu-preview");
    var generic = document.getElementById("webgpu-generic");
    if (preview) {
      preview.hidden = true;
    }
    if (generic) {
      generic.hidden = false;
    }
    hideExerciseNodes();
  }

  function showPreview(id) {
    var preview = document.getElementById("webgpu-preview");
    var generic = document.getElementById("webgpu-generic");
    var node = exerciseNode(id);
    if (!preview || !node) {
      return false;
    }
    hideExerciseNodes();
    node.hidden = false;
    preview.hidden = false;
    if (generic) {
      generic.hidden = true;
    }
    return true;
  }

  function show(reason) {
    var panel = document.getElementById("webgpu-unsupported");
    if (!panel) {
      return false;
    }
    updateBrowserHelp();
    panel.hidden = false;
    if (document.body) {
      document.body.classList.add("webgpu-unsupported");
    }
    var reasonLine = document.getElementById("webgpu-reason");
    var code = document.getElementById("webgpu-reason-code");
    if (reasonLine) {
      reasonLine.hidden = false;
    }
    if (code) {
      code.textContent =
        String(reason || "") || "WebGPU is unavailable in this browser.";
    }
    var exercise = parseExercise(
      window.location && window.location.search ? window.location.search : ""
    );
    if (!exercise || !showPreview(exercise)) {
      showGeneric();
    }
    return true;
  }

  function hide() {
    var panel = document.getElementById("webgpu-unsupported");
    if (panel) {
      panel.hidden = true;
    }
    if (document.body) {
      document.body.classList.remove("webgpu-unsupported");
    }
  }

  function reloadPage() {
    window.location.reload();
  }

  function init() {
    updateBrowserHelp();
    var retries = document.querySelectorAll("[data-webgpu-retry]");
    for (var i = 0; i < retries.length; i += 1) {
      retries[i].addEventListener("click", reloadPage);
    }
    var back = document.getElementById("webgpu-preview-back");
    if (back) {
      back.addEventListener("click", showGeneric);
    }
  }

  window.vfeWebGpu = {
    show: show,
    hide: hide,
    browser: detectBrowser,
    parseExercise: parseExercise,
    showPreview: showPreview,
    showGeneric: showGeneric,
  };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
