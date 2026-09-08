import { createApp } from "vue";
import { createPinia } from "pinia";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { setSettingsReadOnly } from "./stores/viewer";
import App from "./App.vue";
import ConvertWindow from "./ConvertWindow.vue";

import "./styles/modern-normalize.css";
import "./styles/global.css";

// 主窗口与独立转换窗口共用同一份前端产物，按窗口 label 选择根组件。
// 浏览器里（pnpm dev）没有 Tauri 上下文，回退为主窗口。
let label = "main";
try {
  label = getCurrentWebviewWindow().label;
} catch {}

// 独立转换窗口只读设置：不回写 settings.json，避免与主窗口的保存互相覆盖
const isConvert = label === "convert";
if (isConvert) {
  setSettingsReadOnly();
}

const app = createApp(isConvert ? ConvertWindow : App);
app.use(createPinia());
app.mount("#app");
