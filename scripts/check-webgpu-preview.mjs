#!/usr/bin/env node
// Deterministic source and runtime check for the WebGPU-unavailable page
// (ticket 236). It does not need a browser or WebGPU:
//
// 1. parse index.html/web/style.css by string structure and assert the
//    recovery-action order, the <details> diagnostics, the hidden defaults,
//    and the four static exercise previews;
// 2. load web/webgpu.js in a tiny fake DOM and assert that a capable browser
//    stays untouched, that show() picks the recognized exercise from the
//    query, that unknown ids fall back to the generic view, and that Retry and
//    the preview back button work.
//
// The helper is deliberately not allowed to decide WebGPU availability, so the
// script also rejects navigator.gpu/requestAdapter/getContext/WebGL tokens in
// its executable lines.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => fs.readFileSync(path.join(root, relative), "utf8");

const html = read("index.html");
const css = read("web/style.css");
const helper = read("web/webgpu.js");
const exercises = ["ring", "shell", "plane", "wire"];

const must = (condition, message) => assert.ok(condition, message);

const panelStart = html.indexOf('<main id="webgpu-unsupported"');
const panelEnd = html.indexOf("</main>", panelStart);
must(panelStart !== -1 && panelEnd > panelStart, "index.html has the unsupported panel");
const panel = html.slice(panelStart, panelEnd);
const openingTag = panel.slice(0, panel.indexOf(">"));
must(/\bhidden\b/.test(openingTag), "the unsupported panel is hidden by default");

const at = (needle, from = 0) => {
  const index = panel.indexOf(needle, from);
  must(index !== -1, `the panel is missing ${needle}`);
  return index;
};

// Recovery order: detected problem, then Retry + Download together, then the
// generic view, then the preview, then the expandable diagnostics.
const reason = at('id="webgpu-reason"');
const actions = at('class="webgpu-actions"');
const retry = at('id="webgpu-retry"');
const download = at('id="webgpu-download"');
const generic = at('id="webgpu-generic"');
const preview = at('id="webgpu-preview"');
const details = at('<details id="webgpu-diagnostics">');
must(
  reason < actions && actions < retry && retry < download,
  "Retry and Download must follow the reason inside the recovery-action group",
);
must(
  download < generic && generic < preview && preview < details,
  "actions, generic recovery, preview and diagnostics are out of order",
);

const actionsEnd = panel.indexOf("</div>", actions);
must(actionsEnd > actions, "the recovery-action group is closed");
const actionGroup = panel.slice(actions, actionsEnd);
must(
  actionGroup.includes('id="webgpu-retry"') &&
    actionGroup.includes('id="webgpu-download"') &&
    actionGroup.includes(">Download desktop app<"),
  "Retry and Download desktop app must be adjacent in one group",
);

// Diagnostics are a native, keyboard-focusable disclosure.
const detailsBlock = panel.slice(details, panel.indexOf("</details>", details) + "</details>".length);
must(detailsBlock.includes("<summary>Browser diagnostics and compatibility</summary>"), "diagnostics have a summary");
for (const token of [
  "Firefox",
  "Chrome",
  "Edge",
  "Linux",
  "about:config",
  "dom.webgpu.enabled",
  "chrome://flags/#enable-unsafe-webgpu",
  "edge://flags/#enable-unsafe-webgpu",
  "--enable-features=Vulkan",
  'id="webgpu-browser-help"',
  "https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API",
  "https://caniuse.com/webgpu",
]) {
  must(detailsBlock.includes(token), `the expandable diagnostics are missing ${token}`);
}
must(
  css.includes("#webgpu-diagnostics summary") &&
    css.includes("#webgpu-unsupported summary:focus-visible"),
  "the summary must be styled and expose a focus-visible outline",
);
must(
  css.includes("#webgpu-unsupported button:focus-visible") &&
    css.includes("#webgpu-unsupported a.webgpu-action"),
  "buttons and the desktop download need focus-visible styling",
);

// Both the generic view and every preview block start hidden; only show()
// reveals the panel. The preview must read as a fixed preview, not a sim.
const previewTag = panel.match(/<section\s+id="webgpu-preview"[\s\S]*?>/);
must(previewTag && /\bhidden\b/.test(previewTag[0]), "the preview is hidden by default");
must(
  /<section id="webgpu-generic"[^>]*>/.test(panel) &&
    !/<section id="webgpu-generic"[^>]*\bhidden\b/.test(panel),
  "the generic recovery view is visible by default",
);
for (const id of exercises) {
  must(
    panel.includes(`<div class="webgpu-exercise" data-exercise="${id}" hidden>`),
    `the ${id} preview block must start hidden`,
  );
}
const previewBlock = panel.slice(preview, details);
must(previewBlock.includes("Static preview."), "the preview is labelled as static");
must(previewBlock.includes("not a live simulation"), "the preview disclaims live simulation");
must((previewBlock.match(/<strong>Question:<\/strong>/g) || []).length === exercises.length, "every exercise has a question");
must((previewBlock.match(/<svg/g) || []).length === exercises.length, "every exercise has an inline diagram");
must((previewBlock.match(/role="img"/g) || []).length === exercises.length, "every diagram has an image role");
must(previewBlock.includes('id="webgpu-preview-back"'), "the preview needs a way back to the generic view");
must(previewBlock.includes("data-webgpu-retry"), "the preview must offer Retry");
must(previewBlock.includes('href="download.html"'), "the preview must offer the desktop download");

must(html.includes('<script src="webgpu.js"></script>'), "webgpu.js is loaded as a classic script");
must(html.includes('href="web/webgpu.js"'), "Trunk still copies web/webgpu.js");
must(!html.includes('<script type="module" src="webgpu.js">'), "the helper must stay a classic script");
const noscript = html.slice(html.indexOf("<noscript>"), html.indexOf("</noscript>"));
must(noscript.includes('href="download.html"'), "the no-JavaScript fallback offers the desktop download");

// The helper must not make availability authoritative or add a renderer.
const helperCode = helper.replace(/^\s*\/\/.*$/gm, "");
for (const forbidden of ["navigator.gpu", "requestAdapter", "getContext", "webgl", "WebGL"]) {
  must(!helperCode.includes(forbidden), `web/webgpu.js must not use ${forbidden}`);
}

// --- Fake DOM: run the classic helper exactly as the page would. -----------
const makeElement = (attributes = {}) => ({
  hidden: attributes.hidden === true,
  textContent: "",
  href: "",
  attributes,
  listeners: {},
  getAttribute(name) {
    return Object.prototype.hasOwnProperty.call(this.attributes, name)
      ? this.attributes[name]
      : null;
  },
  addEventListener(type, listener) {
    this.listeners[type] = listener;
  },
  click() {
    if (this.listeners.click) {
      this.listeners.click({ preventDefault() {} });
    }
  },
});

const elements = {
  "webgpu-unsupported": makeElement({ hidden: true }),
  "webgpu-reason": makeElement({ hidden: true }),
  "webgpu-reason-code": makeElement(),
  "webgpu-retry": makeElement({ "data-webgpu-retry": "" }),
  "webgpu-download": makeElement(),
  "webgpu-generic": makeElement(),
  "webgpu-preview": makeElement({ hidden: true }),
  "webgpu-preview-back": makeElement(),
  "webgpu-browser-help": makeElement(),
};
const exerciseNodes = exercises.map((id) =>
  makeElement({ hidden: true, "data-exercise": id }),
);
const retryNodes = [elements["webgpu-retry"], makeElement({ "data-webgpu-retry": "" })];

const bodyClasses = new Set();
const fakeDocument = {
  readyState: "complete",
  body: {
    classList: {
      add(name) {
        bodyClasses.add(name);
      },
      remove(name) {
        bodyClasses.delete(name);
      },
    },
  },
  getElementById(id) {
    return elements[id] || null;
  },
  querySelectorAll(selector) {
    if (selector === "[data-exercise]") {
      return exerciseNodes;
    }
    if (selector === "[data-webgpu-retry]") {
      return retryNodes;
    }
    return [];
  },
  addEventListener() {},
};

let reloads = 0;
const fakeWindow = {
  location: {
    search: "",
    reload() {
      reloads += 1;
    },
  },
};
const fakeNavigator = { userAgent: "Mozilla/5.0 Firefox/999.0" };

new Function("window", "document", "navigator", helper)(
  fakeWindow,
  fakeDocument,
  fakeNavigator,
);
const api = fakeWindow.vfeWebGpu;
must(api && typeof api.show === "function", "webgpu.js exposes window.vfeWebGpu.show");

// Capable browser: loading the helper must not reveal anything.
must(elements["webgpu-unsupported"].hidden === true, "a capable browser keeps the panel hidden");
must(elements["webgpu-preview"].hidden === true, "a capable browser keeps the preview hidden");
must(elements["webgpu-generic"].hidden === false, "the generic view is the default content");
must(bodyClasses.size === 0, "the helper does not add the unsupported body class on load");

// Retry (both copies) reloads only when clicked.
elements["webgpu-retry"].click();
must(reloads === 1, "the primary Retry button reloads the page");
retryNodes[1].click();
must(reloads === 2, "the preview Retry button reloads the page");

// Generic visit: no exercise query.
must(api.show("requestAdapter() returned no adapter") === true, "show() reveals the panel");
must(elements["webgpu-unsupported"].hidden === false, "show() unhides the panel");
must(elements["webgpu-reason"].hidden === false, "show() unhides the detected problem");
must(
  elements["webgpu-reason-code"].textContent === "requestAdapter() returned no adapter",
  "show() writes the detected problem",
);
must(bodyClasses.has("webgpu-unsupported"), "show() adds the unsupported body class");
must(elements["webgpu-preview"].hidden === true, "a generic visit does not show a preview");
must(elements["webgpu-generic"].hidden === false, "a generic visit keeps the standard recovery view");
must(exerciseNodes.every((node) => node.hidden), "a generic visit hides every exercise block");

// Recognized exercise link.
fakeWindow.location.search = "?exercise=ring";
api.show("navigator.gpu is unavailable in this browser");
must(elements["webgpu-preview"].hidden === false, "a recognized exercise shows the preview");
must(elements["webgpu-generic"].hidden === true, "the preview replaces the generic view");
must(exerciseNodes[0].hidden === false, "?exercise=ring reveals the ring exercise");
must(exerciseNodes.slice(1).every((node) => node.hidden), "the other exercise blocks stay hidden");

// Unknown or missing ids fall back to the standard recovery.
for (const search of ["?exercise=slinky", "?preview=1", "?exercise=", ""]) {
  fakeWindow.location.search = search;
  api.show("no adapter");
  must(elements["webgpu-preview"].hidden === true, `${search || "(none)"} must not show a preview`);
  must(elements["webgpu-generic"].hidden === false, `${search || "(none)"} must show the generic view`);
  must(exerciseNodes.every((node) => node.hidden), `${search || "(none)"} must hide every exercise`);
}

// Query parsing is case-insensitive, tolerant of an optional preview=1, and
// rejects unknown ids. It is pure and does not touch the DOM.
assert.equal(api.parseExercise("?exercise=ring"), "ring");
assert.equal(api.parseExercise("?exercise=RING"), "ring");
assert.equal(api.parseExercise("?exercise=%73hell"), "shell");
assert.equal(api.parseExercise("?preview=1&exercise=plane"), "plane");
assert.equal(api.parseExercise("?exercise=wire&preview=1"), "wire");
assert.equal(api.parseExercise("?exercise=slinky"), null);
assert.equal(api.parseExercise("?preview=1"), null);
assert.equal(api.parseExercise(""), null);

// The preview's way back restores the generic view in place.
fakeWindow.location.search = "?exercise=shell";
api.show("no adapter");
must(elements["webgpu-preview"].hidden === false, "shell preview is visible before Back");
elements["webgpu-preview-back"].click();
must(elements["webgpu-preview"].hidden === true, "Back hides the preview");
must(elements["webgpu-generic"].hidden === false, "Back restores the generic view");
must(exerciseNodes.every((node) => node.hidden), "Back hides every exercise block");

console.log(
  "webgpu-preview: PASS - action order, <details> diagnostics, hidden defaults, " +
    "4 static exercise previews, and generic/recognized preview selection",
);
