import { createApp } from "vue";

import App from "./App.vue";
import { router } from "./router";
import "tdesign-mobile-vue/es/style/index.css";
import "./styles/base.css";

createApp(App).use(router).mount("#app");
