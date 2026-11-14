import { defineConfig } from "@playwright/test";
import base from "./playwright.config";
export default defineConfig({
  ...base,
  testMatch: "**/online.spec.ts",
  testIgnore: [],
  timeout: 60000,
  use: {
    ...base.use,
    baseURL: process.env.EMBERWATCH_ONLINE_URL ?? "http://127.0.0.1:3000",
  },
  webServer: undefined,
});
