"use strict";

const workspace = document.querySelector("#workspace");
const frame = document.querySelector("#rdp");
const fullscreen = document.querySelector("#fullscreen");
const notice = document.querySelector("#notice");
const sessionState = document.querySelector("#session-state");

function showNotice(text) {
  notice.textContent = text;
  notice.classList.add("visible");
  window.setTimeout(() => notice.classList.remove("visible"), 3000);
}

function sessionUrl() {
  if (!location.hash.startsWith("#")) return null;
  try {
    const url = new URL(decodeURIComponent(location.hash.slice(1)), location.origin);
    if (url.origin !== location.origin || !url.pathname.startsWith("/guacamole/")) return null;
    return url.href;
  } catch (_) {
    return null;
  }
}

const url = sessionUrl();
if (url) {
  frame.src = url;
  history.replaceState(null, "", "/rdp.html");
  frame.addEventListener("load", () => {
    sessionState.textContent = "Connected through secure gateway";
    frame.focus();
  });
} else {
  sessionState.textContent = "No active session";
  showNotice("Invalid or expired remote desktop launch.");
  fullscreen.disabled = true;
}

fullscreen.addEventListener("click", async () => {
  try {
    if (!document.fullscreenElement) {
      await workspace.requestFullscreen({ navigationUI: "hide" });
      if (navigator.keyboard?.lock) await navigator.keyboard.lock();
      frame.focus();
      showNotice("Full screen enabled. Remote keyboard capture is active.");
    } else {
      if (navigator.keyboard?.unlock) navigator.keyboard.unlock();
      await document.exitFullscreen();
    }
  } catch (error) {
    showNotice("Full screen or keyboard capture was denied by the browser.");
  }
});

document.addEventListener("fullscreenchange", () => {
  const active = Boolean(document.fullscreenElement);
  fullscreen.textContent = active ? "Exit full screen" : "Full screen";
  if (!active && navigator.keyboard?.unlock) navigator.keyboard.unlock();
  if (active) frame.focus();
});
