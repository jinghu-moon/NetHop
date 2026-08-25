import { createApp } from "vue";

import App from "./App.vue";
import { router } from "./router";
import "./styles/fonts.css";
import "./styles/theme-light.css";
import "./styles/theme-dark.css";
import "./styles/foundations.css";
import "./styles/layout.css";
import "./styles/shared.css";
import "./styles/pages/overview.css";
import "./styles/pages/subscriptions.css";
import "./styles/pages/visuals.css";
import "./styles/pages/settings.css";
import "./styles/pages/applications.css";
import "./styles/pages/nodes.css";
import "./styles/pages/operations.css";

createApp(App).use(router).mount("#app");
