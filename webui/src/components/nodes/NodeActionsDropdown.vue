<script setup lang="ts">
import {
  IconArrowLeft,
  IconArrowsSort,
  IconCheck,
  IconChevronRight,
  IconCopy,
  IconDotsVertical,
  IconEdit,
  IconRefresh,
  IconTrash,
  IconX,
} from "@tabler/icons-vue";
import { ref } from "vue";
import Dropdown from "@/components/ui/overlay/Dropdown.vue";
import IconButton from "@/components/ui/primitives/IconButton.vue";
import MenuItem from "@/components/ui/menu/MenuItem.vue";
import MenuList from "@/components/ui/menu/MenuList.vue";
import MenuSection from "@/components/ui/menu/MenuSection.vue";
import MenuItemRadio from "@/components/ui/menu/MenuItemRadio.vue";
import type { NodeSort } from "@/model/node-view";

const props = defineProps<{
  sort: NodeSort;
  hasDelayResults: boolean;
  hasSelectedNode: boolean;
}>();
const open = ref(false);
const emit = defineEmits<{
  refresh: [];
  sortChange: [value: NodeSort];
  clearDelays: [];
  export: [];
  edit: [];
  exclude: [];
}>();

const sortOptions: readonly { readonly value: NodeSort; readonly label: string }[] = [
  { value: "default", label: "默认顺序" },
  { value: "name", label: "名称" },
  { value: "latency-asc", label: "延迟：低到高" },
  { value: "latency-desc", label: "延迟：高到低" },
];
const sortLabels: Readonly<Record<NodeSort, string>> = {
  default: "默认",
  name: "名称",
  "latency-asc": "延迟升序",
  "latency-desc": "延迟降序",
};

type NodeMenuAction = "refresh" | "clearDelays" | "export" | "edit" | "exclude";

function runAction(action: NodeMenuAction, close: () => void): void {
  if (action === "refresh") emit("refresh");
  else if (action === "clearDelays") emit("clearDelays");
  else if (action === "export") emit("export");
  else if (action === "edit") emit("edit");
  else emit("exclude");
  close();
}

function selectSort(value: NodeSort, close: () => void): void {
  emit("sortChange", value);
  close();
}
</script>

<template>
  <Dropdown v-model:open="open" class="node-actions-dropdown" panel-class="node-actions-menu" panel-width="200px" placement="bottom-end" :close-on-select="false">
    <template #trigger="{ open }">
      <IconButton
        class="node-actions-trigger"
        size="s"
        variant="outline"
        aria-label="更多操作"
        title="更多操作"
        aria-haspopup="menu"
        :aria-expanded="open"
        :data-open="open"
      >
        <IconDotsVertical :size="19" />
      </IconButton>
    </template>

    <template #default="{ activePanel, pushPanel, popPanel, close }">
      <template v-if="activePanel === 'root'">
        <MenuList aria-label="节点操作">
          <MenuSection>
            <MenuItem class="anchored-dropdown__option" @click="runAction('refresh', close)">
              <template #prefix><IconRefresh class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" /></template>
              刷新节点列表
            </MenuItem>
            <MenuItem class="anchored-dropdown__option" aria-haspopup="menu" @click="pushPanel('sort')">
              <template #prefix><IconArrowsSort class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" /></template>
              <template #suffix><span class="anchored-dropdown__option-trailing"><span>{{ sortLabels[sort] }}</span><IconChevronRight :size="15" aria-hidden="true" /></span></template>
              排序方式
            </MenuItem>
          </MenuSection>
          <MenuSection divided>
            <MenuItem class="anchored-dropdown__option" :disabled="!hasDelayResults" @click="runAction('clearDelays', close)">
              <template #prefix><IconX class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" /></template>
              清除测速结果
            </MenuItem>
            <MenuItem class="anchored-dropdown__option" :disabled="!hasSelectedNode" @click="runAction('export', close)">
              <template #prefix><IconCopy class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" /></template>
              导出当前节点
            </MenuItem>
            <MenuItem class="anchored-dropdown__option" :disabled="!hasSelectedNode" @click="runAction('edit', close)">
              <template #prefix><IconEdit class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" /></template>
              编辑当前节点
            </MenuItem>
            <MenuItem class="anchored-dropdown__option" danger :disabled="!hasSelectedNode" @click="runAction('exclude', close)">
              <template #prefix><IconTrash class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" /></template>
              排除当前节点
            </MenuItem>
          </MenuSection>
        </MenuList>
      </template>

      <section v-else-if="activePanel === 'sort'" class="anchored-dropdown__section">
        <div class="anchored-dropdown__options" role="group" aria-label="节点排序方式">
          <MenuItem class="anchored-dropdown__option" @click="popPanel">
            <template #prefix><IconArrowLeft class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" /></template>
            排序方式
          </MenuItem>
          <MenuItemRadio
            v-for="option in sortOptions"
            :key="option.value"
            class="anchored-dropdown__option"
            :selected="option.value === sort"
            @click="selectSort(option.value, close)"
          >
            <template #suffix><IconCheck class="anchored-dropdown__option-icon" :size="17" aria-hidden="true" :data-visible="option.value === sort" /></template>
            {{ option.label }}
          </MenuItemRadio>
        </div>
      </section>
    </template>
  </Dropdown>
</template>

<style scoped>
.node-actions-dropdown {
  flex: 0 0 auto;
}

.node-actions-trigger[data-open="true"] {
  border-color: var(--focus-ring);
  color: var(--nh-info);
}
:global(.node-actions-menu) { width: 200px; }
</style>
