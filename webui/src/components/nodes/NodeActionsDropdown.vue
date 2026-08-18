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
import { Button as TButton } from "tdesign-mobile-vue";
import AnchoredDropdown from "@/components/AnchoredDropdown.vue";
import type { NodeSort } from "@/model/node-view";

const props = defineProps<{
  sort: NodeSort;
  hasDelayResults: boolean;
  hasSelectedNode: boolean;
}>();
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
  <AnchoredDropdown class="node-actions-dropdown" menu-label="节点操作" menu-class="node-actions-menu" menu-width="200px" :offset="6">
    <template #trigger="{ open, toggle }">
      <TButton
        class="node-actions-trigger"
        size="small"
        shape="square"
        variant="outline"
        theme="default"
        title="更多操作"
        aria-haspopup="menu"
        :aria-expanded="open"
        :data-open="open"
        @click="toggle"
      >
        <IconDotsVertical :size="19" />
      </TButton>
    </template>

    <template #default="{ activePanel, pushPanel, popPanel, close }">
      <template v-if="activePanel === 'root'">
        <section class="anchored-dropdown__section">
          <div class="anchored-dropdown__options">
            <button class="anchored-dropdown__option" type="button" role="menuitem" @click="runAction('refresh', close)">
              <span class="anchored-dropdown__option-content">
                <IconRefresh class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" />
                <span>刷新节点列表</span>
              </span>
            </button>
            <button class="anchored-dropdown__option" type="button" role="menuitem" aria-haspopup="menu" @click="pushPanel('sort')">
              <span class="anchored-dropdown__option-content">
                <IconArrowsSort class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" />
                <span>排序方式</span>
              </span>
              <span class="anchored-dropdown__option-trailing">
                <span>{{ sortLabels[sort] }}</span>
                <IconChevronRight :size="15" aria-hidden="true" />
              </span>
            </button>
          </div>
        </section>
        <section class="anchored-dropdown__section anchored-dropdown__section--divided">
          <div class="anchored-dropdown__options">
            <button class="anchored-dropdown__option" type="button" role="menuitem" :disabled="!hasDelayResults" @click="runAction('clearDelays', close)">
              <span class="anchored-dropdown__option-content">
                <IconX class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" />
                <span>清除测速结果</span>
              </span>
            </button>
            <button class="anchored-dropdown__option" type="button" role="menuitem" :disabled="!hasSelectedNode" @click="runAction('export', close)">
              <span class="anchored-dropdown__option-content">
                <IconCopy class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" />
                <span>导出当前节点</span>
              </span>
            </button>
            <button class="anchored-dropdown__option" type="button" role="menuitem" :disabled="!hasSelectedNode" @click="runAction('edit', close)">
              <span class="anchored-dropdown__option-content">
                <IconEdit class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" />
                <span>编辑当前节点</span>
              </span>
            </button>
            <button class="anchored-dropdown__option" type="button" role="menuitem" data-tone="danger" :disabled="!hasSelectedNode" @click="runAction('exclude', close)">
              <span class="anchored-dropdown__option-content">
                <IconTrash class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" />
                <span>排除当前节点</span>
              </span>
            </button>
          </div>
        </section>
      </template>

      <section v-else-if="activePanel === 'sort'" class="anchored-dropdown__section">
        <div class="anchored-dropdown__options" role="group" aria-label="节点排序方式">
          <button class="anchored-dropdown__option" type="button" role="menuitem" @click="popPanel">
            <span class="anchored-dropdown__option-content">
              <IconArrowLeft class="anchored-dropdown__option-leading-icon" :size="18" aria-hidden="true" />
              <span>排序方式</span>
            </span>
          </button>
          <button
            v-for="option in sortOptions"
            :key="option.value"
            class="anchored-dropdown__option"
            type="button"
            role="menuitemradio"
            :aria-checked="option.value === sort"
            :data-selected="option.value === sort"
            @click="selectSort(option.value, close)"
          >
            <span>{{ option.label }}</span>
            <IconCheck class="anchored-dropdown__option-icon" :size="17" aria-hidden="true" :data-visible="option.value === sort" />
          </button>
        </div>
      </section>
    </template>
  </AnchoredDropdown>
</template>

<style scoped>
.node-actions-dropdown {
  flex: 0 0 auto;
}

.node-actions-trigger[data-open="true"] {
  border-color: var(--focus-ring);
  color: var(--nh-info);
}
</style>
