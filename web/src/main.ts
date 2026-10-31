import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { useWalletStore } from "./stores/wallet";

const app = createApp(App);
app.use(createPinia());
app.mount("#app");

// Eager-connect after mount; safe if no wallet is installed.
const wallet = useWalletStore();
wallet.tryEagerConnect();
