<script setup lang="ts">
import TerritoryFlag from "./TerritoryFlag.vue";
import type { ActiveNodeView } from "@/model/node-view";

defineProps<{ readonly value: ActiveNodeView }>();
</script>

<template>
  <section class="active-node-summary" :data-kind="value.kind">
    <template v-if="value.kind === 'node'">
      <TerritoryFlag :code="value.node.displayTerritoryCode" size="large" />
      <div class="active-node-copy">
        <strong :title="value.node.name">{{ value.node.name }}</strong>
        <span>{{ value.node.protocol.toUpperCase() }} · {{ value.sourceLabel }}</span>
      </div>
      <div class="active-node-status">
        <strong :data-state="value.latency.state">{{ value.latency.text }}</strong>
        <span>{{ value.modeLabel }}</span>
      </div>
    </template>
    <template v-else>
      <TerritoryFlag size="large" />
      <div class="active-node-copy">
        <strong>{{ value.title }}</strong>
        <span>{{ value.detail }}</span>
      </div>
    </template>
  </section>
</template>
