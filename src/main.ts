import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import "./styles.css";
import { useSettingsStore } from "./stores/settings";

const app = createApp(App);
app.use(createPinia());

// restore persisted theme before first paint to avoid a light/dark flash
useSettingsStore()
  .load()
  .finally(() => app.mount("#app"));
