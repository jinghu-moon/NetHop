<script setup lang="ts">
import TerritoryFlag from "./TerritoryFlag.vue";
import { latencyView, type NodeLatencyState } from "@/model/node-view";
import type { NodeDto } from "@/model/dto";

const props = defineProps<{
  readonly node: NodeDto;
  readonly probeState?: NodeLatencyState | undefined;
}>();
defineEmits<{ select: [node: NodeDto] }>();

function shownLatency() {
  const state = props.probeState;
  if (state === "measuring" || state === "timeout" || state === "unavailable" || state === "protocol_error") {
    return latencyView(undefined, state);
  }
  return latencyView(props.node.latencyMs, props.node.alive === false ? "unavailable" : undefined);
}
</script>

<template>
  <button
    type="button"
    class="node-card"
    :data-requested="node.isRequested"
    :data-active="node.isActive"
    :disabled="node.isRequested"
    @click="$emit('select', node)"
  >
    <div class="node-card-copy">
      <strong :title="node.name">{{ node.name }}</strong>
      <span>{{ node.protocol }}</span>
    </div>
    <div class="node-card-footer">
      <TerritoryFlag :code="node.displayTerritoryCode" />
      <span class="node-card-latency" :data-state="shownLatency().state">{{ shownLatency().text }}</span>
    </div>
  </button>
</template>
