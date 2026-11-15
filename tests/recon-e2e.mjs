// Real local integration check: requires Rust :8081, Next :3000 and run_recon_demo.sh.
// No request interception or fixtures. Artifacts stay under ignored ros2/generated/.
import { chromium } from "../web/node_modules/@playwright/test/index.mjs";
import { mkdir, writeFile } from "node:fs/promises";
import assert from "node:assert/strict";

const origin = process.env.RECON_WEB_URL || "http://127.0.0.1:3000";
const output = new URL("../ros2/generated/recon/verification/", import.meta.url);
await mkdir(output, { recursive:true });
const browser = await chromium.launch({ channel:"chrome", headless:true });
try {
  const page = await browser.newPage({ viewport:{ width:1440, height:1000 } });
  await page.addInitScript(() => localStorage.setItem("emberwatch-locale", "en"));
  const errors = [];
  page.on("pageerror", e => errors.push(e.message));
  await page.goto(origin + "/recon");
  const assign = page.getByRole("button", { name:"Assign simulated reconnaissance" });
  await assign.waitFor();
  await page.waitForFunction(() => ![...document.querySelectorAll("button")].find(b => b.textContent.includes("Assign simulated"))?.disabled);
  await assign.click();
  const states = new Set(["assigned"]);
  let task;
  const deadline = Date.now() + 90000;
  while (Date.now() < deadline) {
    const response = await page.request.get(origin + "/api/recon-backend/api/recon/tasks?run_id=synthetic-portugal");
    assert.equal(response.status(), 200);
    const tasks = (await response.json()).tasks.sort((a,b) => a.assigned_at.localeCompare(b.assigned_at));
    task = tasks.at(-1);
    const state = task?.observation?.state || "assigned";
    states.add(state);
    if (state === "failed") throw new Error(task.observation.note);
    if (state === "observing" && task.frame) break;
    await page.waitForTimeout(500);
  }
  assert.equal(task?.observation?.state, "observing", "actual simulator must reach observation station");
  await page.getByTestId("mission-state").filter({ hasText:"Observing" }).waitFor();
  const image = page.getByRole("img", { name:"Frame rendered by the simulated drone camera" });
  await image.waitFor();
  await page.waitForFunction(() => { const im = document.querySelector("main img"); return im?.complete && im.naturalWidth === 640; });
  const sceneDetail = await image.evaluate(im => {
    const canvas = document.createElement("canvas"); canvas.width = 640; canvas.height = 480;
    const ctx = canvas.getContext("2d"); ctx.drawImage(im, 0, 0);
    const pixels = ctx.getImageData(0, 0, 640, 480).data;
    let min=255, max=0;
    for (let i=0; i<pixels.length; i+=4) { min=Math.min(min,pixels[i]); max=Math.max(max,pixels[i]); }
    return max-min;
  });
  assert(sceneDetail > 20, "camera must show scene detail, not a uniform occluded frame");
  // Fetch the exact frame revision; retry only if the next simulator frame replaced it.
  let jpeg;
  for (let i=0; i<5; i++) {
    const response = await page.request.get(origin + "/api/recon-backend" + task.frame.image_url);
    if (response.ok()) { jpeg = await response.body(); break; }
    const tasks = (await (await page.request.get(origin + "/api/recon-backend/api/recon/tasks?run_id=synthetic-portugal")).json()).tasks;
    task = tasks.find(t => t.id === task.id);
  }
  assert(jpeg?.length > 1000);
  assert.equal(task.frame.origin, "gazebo_camera_render");
  assert.equal(task.observation.drone_position_lon_lat, null);
  assert.equal(task.incident.first_event_at, null);
  assert.equal(task.reports[0].origin_group, "synthetic-origin-1");
  await writeFile(new URL("observing.jpg", output), jpeg);
  await writeFile(new URL("observing-task.json", output), JSON.stringify(task, null, 2));
  await page.screenshot({ path:new URL("observing-desktop.png", output).pathname, fullPage:true });
  await page.setViewportSize({ width:390, height:844 });
  await page.screenshot({ path:new URL("observing-mobile.png", output).pathname, fullPage:true });
  await page.setViewportSize({ width:1440, height:1000 });
  await page.getByRole("button", { name:"Request return" }).click();
  const returnDeadline = Date.now() + 60000;
  while (Date.now() < returnDeadline) {
    const tasks = (await (await page.request.get(origin + "/api/recon-backend/api/recon/tasks?run_id=synthetic-portugal")).json()).tasks;
    task = tasks.find(t => t.id === task.id);
    const state = task.observation.state;
    states.add(state);
    if (state === "complete") break;
    assert.notEqual(state, "failed");
    await page.waitForTimeout(500);
  }
  assert.equal(task.observation.state, "complete");
  for (const state of ["assigned", "travelling", "observing", "returning", "complete"]) assert(states.has(state), state);
  assert.deepEqual(errors, []);
  const report = { checked_at:new Date().toISOString(), states:[...states], image_bytes:jpeg.length,
    image_origin:task.frame.origin, scene_detail_range:sceneDetail, source:task.reports[0].source_url, operator_return:true,
    simulation:true, renderer:"Gazebo ogre2 mounted camera", flight:"existing kinematic stand-in; no PX4 dynamics", browser_errors:errors };
  await writeFile(new URL("result.json", output), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
} finally { await browser.close(); }
