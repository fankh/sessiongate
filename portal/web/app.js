"use strict";

const message = document.querySelector("#message");
const targets = document.querySelector("#targets");
const credentials = () => ({
  "Authorization": "Bearer " + document.querySelector("#token").value,
  "Content-Type": "application/json"
});

document.querySelector("#load").addEventListener("click", async () => {
  message.textContent = "";
  targets.replaceChildren();
  try {
    const response = await fetch("/api/v1/rdp/targets", { headers: credentials() });
    if (!response.ok) throw new Error("Target request failed (" + response.status + ")");
    for (const target of await response.json()) renderTarget(target);
  } catch (error) {
    message.textContent = error.message;
  }
});

function renderTarget(target) {
  const row = document.createElement("div");
  row.className = "target";
  const info = document.createElement("div");
  const title = document.createElement("h2");
  title.textContent = target.name;
  const controls = document.createElement("div");
  controls.className = "controls";
  const entries = [
    ["Clipboard out", target.policy.clipboard_to_browser],
    ["Clipboard in", target.policy.clipboard_to_remote],
    ["Upload", target.policy.upload], ["Download", target.policy.download],
    ["Printing", target.policy.printing], ["Audio", target.policy.audio_output],
    ["Microphone", target.policy.microphone], ["Recording", target.policy.recording]
  ];
  for (const entry of entries) {
    const tag = document.createElement("span");
    tag.className = "tag " + (entry[1] ? "" : "denied");
    tag.textContent = entry[0] + ": " + (entry[1] ? "allowed" : "blocked");
    controls.append(tag);
  }
  info.append(title, controls);
  const launch = document.createElement("button");
  launch.textContent = "Connect securely";
  launch.addEventListener("click", async () => {
    launch.disabled = true;
    message.textContent = "";
    try {
      const response = await fetch("/api/v1/rdp/sessions", {
        method: "POST",
        headers: credentials(),
        body: JSON.stringify({
          target_id: target.id,
          rdp_username: document.querySelector("#rdp-user").value,
          rdp_password: document.querySelector("#rdp-password").value
        })
      });
      if (!response.ok) throw new Error(await response.text());
      const session = await response.json();
      document.querySelector("#rdp-password").value = "";
      window.location.assign("/rdp.html#" + encodeURIComponent(session.guacamole_url));
    } catch (error) {
      message.textContent = error.message;
      launch.disabled = false;
    }
  });
  row.append(info, launch);
  targets.append(row);
}
