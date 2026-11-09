// WebGPU-unavailable panel helper (ticket 54).
//
// The Rust wasm entry point (src/main.rs) owns the authoritative check:
// it reads navigator.gpu and awaits requestAdapter() before eframe's
// WebRunner can start. This classic script only toggles the static panel,
// picks the browser-specific support link, and installs the Retry click
// handler, so it never starts the app, never substitutes a renderer, and never
// decides availability on its own. Retry is inert only if this script fails to
// load; the Rust gate still reveals the panel directly in that case.
(function () {
  "use strict";

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

  function init() {
    updateBrowserHelp();
    var retry = document.getElementById("webgpu-retry");
    if (retry) {
      retry.addEventListener("click", function () {
        window.location.reload();
      });
    }
  }

  window.vfeWebGpu = {
    show: show,
    hide: hide,
    browser: detectBrowser,
  };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
