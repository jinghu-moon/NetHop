<script setup lang="ts">
import { IconRefresh } from "@tabler/icons-vue";
import { Button as TButton } from "tdesign-mobile-vue";

const reload = (): void => {
  window.location.reload();
};
</script>

<template>
  <main class="foundation-shell">
    <header class="foundation-header">
      <div>
        <h1>NetHop</h1>
        <p>控制服务尚未连接</p>
      </div>
      <TButton shape="square" theme="default" variant="outline" @click="reload">
        <IconRefresh :size="20" />
      </TButton>
    </header>
  </main>
</template>
