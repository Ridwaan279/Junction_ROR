// Actual backend/UI failure checks. Start without a recon simulator, then start
// run_recon_demo.sh --force-no-route --no-camera while this waits for failure.
import { chromium } from "../web/node_modules/@playwright/test/index.mjs";
import { writeFile } from "node:fs/promises";
import assert from "node:assert/strict";
const base=process.env.RECON_WEB_URL || "http://127.0.0.1:3000";
const folder=new URL("../ros2/generated/recon/verification/",import.meta.url);
const browser=await chromium.launch({channel:"chrome",headless:true});
try {
  const page=await browser.newPage({viewport:{width:1440,height:1000}});
  await page.addInitScript(()=>localStorage.setItem("emberwatch-locale","en"));
  await page.goto(base+"/recon");
  await page.getByText("Stale frame — this is not the current view.").waitFor({timeout:25000});
  await page.getByText("Telemetry missing or stale").waitFor();
  await page.screenshot({path:new URL("stale.png",folder).pathname,fullPage:true});
  await page.getByRole("button",{name:"Assign simulated reconnaissance"}).click();
  await page.getByText("Waiting for an actual frame from the Gazebo renderer.",{exact:true}).waitFor();
  assert.equal(await page.locator("main img").count(),0);
  await page.screenshot({path:new URL("no-camera.png",folder).pathname,fullPage:true});
  console.log("STALE_AND_NO_CAMERA_PASS; waiting for --force-no-route --no-camera simulator");
  await page.getByTestId("mission-state").filter({hasText:"Failed"}).waitFor({timeout:60000});
  await page.getByText("no_route: demonstration corridor blocked").waitFor();
  assert.equal(await page.locator("main img").count(),0);
  await page.screenshot({path:new URL("no-route.png",folder).pathname,fullPage:true});
  await writeFile(new URL("failures.json",folder),JSON.stringify({checked_at:new Date().toISOString(),stale_frame:true,stale_telemetry:true,no_camera_no_mock:true,no_route:true,request_interception:false},null,2));
  console.log("NO_ROUTE_AND_NO_MOCK_PASS");
}finally{await browser.close();}
