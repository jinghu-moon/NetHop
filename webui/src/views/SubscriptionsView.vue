<script setup lang="ts">
import {
  IconArrowDown,
  IconArrowUp,
  IconDotsVertical,
  IconEdit,
  IconFileImport,
  IconPlus,
  IconRefresh,
  IconTrash,
} from "@tabler/icons-vue";
import {
  ActionSheet as TActionSheet,
  Button as TButton,
  Input as TInput,
  Popup as TPopup,
  Radio as TRadio,
  RadioGroup as TRadioGroup,
  Tag as TTag,
  Textarea as TTextarea,
} from "tdesign-mobile-vue";
import { computed, h, nextTick, onActivated, ref, watch } from "vue";
import { runJson } from "@/bridge/command";
import { useHost } from "@/bridge/context";
import { uploadPrivatePayload } from "@/bridge/private-payload";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import PageState from "@/components/PageState.vue";
import { validatedQuery } from "@/model/client";
import { parseConfig, type SubscriptionDto } from "@/model/dto";
import { presentSubscription, type SubscriptionPresentation } from "@/model/subscription-presentation";
import { createOperationStore } from "@/runtime/operation";
import { uiStores } from "@/runtime/store";
import { useBackDismiss } from "@/shell/useBackDispatcher";

type SourceAction = "update" | "edit" | "move-up" | "move-down" | "delete";

const host = useHost();
const loading = ref(false);
const loadError = ref(false);
const editorOpen = ref(false);
const importOpen = ref(false);
const actionSheetOpen = ref(false);
const editing = ref<SubscriptionDto>();
const activeItem = ref<SubscriptionDto>();
const queuedAction = ref<{ readonly item: SubscriptionDto; readonly action: SourceAction }>();
const pendingDelete = ref<SubscriptionDto>();
const name = ref("");
const url = ref("");
const formError = ref("");
const saving = ref(false);
const importText = ref("");
const importPreview = ref<Readonly<Record<string, unknown>>>();
const updatingIds = ref<ReadonlySet<string>>(new Set());
const updateAllPending = ref(false);
const selectingSourceId = ref<string>();
const selectedSourceId = ref("");
const clockSeconds = ref(Math.floor(Date.now() / 1_000));
const feedbackId = ref<string>();
const operations = createOperationStore();
useBackDismiss(
  () => importOpen.value || editorOpen.value || actionSheetOpen.value,
  () => {
    if (importOpen.value) closeImport();
    else if (editorOpen.value) closeEditor();
    else actionSheetOpen.value = false;
  },
);

const items = computed(() => uiStores.runtime.subscriptionOrder.value
  .map((id) => uiStores.runtime.subscriptionsById.value[id])
  .filter((item): item is SubscriptionDto => Boolean(item)));
const presentations = computed<Readonly<Record<string, SubscriptionPresentation>>>(() => Object.fromEntries(items.value.map((item) => [item.id, presentSubscription(item, clockSeconds.value)])));
const currentFeedback = computed(() => feedbackId.value ? operations.byId[feedbackId.value] : undefined);
const activeIndex = computed(() => activeItem.value ? items.value.findIndex((item) => item.id === activeItem.value?.id) : -1);
const actionKeys = computed<readonly SourceAction[]>(() => activeItem.value
  ? ["update", "edit", "move-up", "move-down", "delete"]
  : []);
const actionItems = computed(() => {
  const item = activeItem.value;
  if (!item) return [];
  return [
    { label: "更新订阅", icon: () => h(IconRefresh, { size: 20 }), disabled: isUpdating(item.id) },
    { label: "编辑", icon: () => h(IconEdit, { size: 20 }) },
    { label: "上移", icon: () => h(IconArrowUp, { size: 20 }), disabled: activeIndex.value <= 0 },
    { label: "下移", icon: () => h(IconArrowDown, { size: 20 }), disabled: activeIndex.value < 0 || activeIndex.value >= items.value.length - 1 },
    { label: "删除", icon: () => h(IconTrash, { size: 20 }), color: "var(--nh-danger)" },
  ];
});

function isUpdating(id: string): boolean { return updatingIds.value.has(id); }
function presentation(item: SubscriptionDto): SubscriptionPresentation { return presentations.value[item.id] ?? presentSubscription(item, clockSeconds.value); }
function setUpdating(id: string, pending: boolean): void {
  const next = new Set(updatingIds.value);
  if (pending) next.add(id); else next.delete(id);
  updatingIds.value = next;
}
function beginFeedback(id: string, key: string): void {
  feedbackId.value = id;
  operations.begin(id, key);
  operations.update(id, "running");
}
function finishFeedback(id: string, success: boolean, message: string): void {
  operations.update(id, success ? "success" : "failure", { message });
}

async function load(): Promise<void> {
  if (loading.value) return;
  loading.value = true;
  loadError.value = false;
  try {
    const config = await validatedQuery(host, { id: "config.get" }, parseConfig);
    uiStores.config.load(config);
    const subscriptions = config.document.subscriptions;
    const sourceItems = subscriptions && typeof subscriptions === "object" && Array.isArray((subscriptions as { sources?: unknown }).sources)
      ? (subscriptions as { sources: readonly unknown[] }).sources
      : [];
    const result = sourceItems.flatMap((raw): SubscriptionDto[] => {
      if (!raw || typeof raw !== "object") return [];
      const source = raw as Record<string, unknown>;
      if (typeof source.source_id !== "string" || typeof source.name !== "string" || typeof source.enabled !== "boolean") return [];
      const status = config.sourceStatus.find((entry) => entry.sourceId === source.source_id);
      return [{ id: source.source_id, name: source.name, enabled: source.enabled, ...(status ? { state: status.health, status } : {}) }];
    });
    clockSeconds.value = Math.floor(Date.now() / 1_000);
    uiStores.runtime.replaceSubscriptions(result);
    if (!result.some((item) => item.id === selectedSourceId.value)) selectedSourceId.value = result.find((item) => item.enabled)?.id ?? result[0]?.id ?? "";
  } catch (error) {
    loadError.value = true;
    throw error;
  } finally {
    loading.value = false;
  }
}

function openEditor(item?: SubscriptionDto): void {
  editorOpen.value = true;
  editing.value = item;
  name.value = item?.name ?? "";
  url.value = "";
  formError.value = "";
}
function closeEditor(): void {
  editorOpen.value = false;
  editing.value = undefined;
  name.value = "";
  url.value = "";
  formError.value = "";
}
async function openContentImport(): Promise<void> {
  closeEditor();
  await nextTick();
  importOpen.value = true;
}
function closeImport(): void {
  importOpen.value = false;
  importText.value = "";
  importPreview.value = undefined;
}

async function save(): Promise<void> {
  const nextName = name.value.trim();
  const nextUrl = url.value.trim();
  if (!nextName) { formError.value = "请输入订阅名称"; return; }
  if ((!editing.value && !nextUrl) || (nextUrl && !/^https:\/\//i.test(nextUrl))) { formError.value = "订阅链接必须使用 HTTPS"; return; }
  const id = "subscription-save";
  const wasEditing = Boolean(editing.value);
  beginFeedback(id, "subscription-config");
  saving.value = true;
  try {
    const digest = uiStores.config.baseDigest.value ?? "0".repeat(64);
    const mutation = editing.value
      ? { type: "update_source", source_id: editing.value.id, name: nextName, ...(nextUrl ? { url: nextUrl } : {}) }
      : { type: "add_source", name: nextName, url: nextUrl };
    await uploadPrivatePayload(host, "subscription", "config-mutate", JSON.stringify({ expected_config_digest: digest, mutation }));
    closeEditor();
    await load();
    finishFeedback(id, true, wasEditing ? "订阅已保存" : "订阅已添加");
  } catch {
    finishFeedback(id, false, "订阅保存失败");
  } finally {
    saving.value = false;
  }
}

async function update(item?: SubscriptionDto): Promise<void> {
  if (item ? isUpdating(item.id) : updateAllPending.value) return;
  const id = item ? `subscription-update-${item.id}` : "subscription-update-all";
  beginFeedback(id, "subscription-update");
  if (item) setUpdating(item.id, true); else updateAllPending.value = true;
  try {
    await runJson(host, { id: "subscription.update", ...(item ? { sourceId: item.id } : {}), wait: true });
    await load();
    finishFeedback(id, true, item ? `“${item.name}”已更新` : "全部订阅已更新");
  } catch {
    finishFeedback(id, false, item ? `“${item.name}”更新失败` : "订阅更新失败");
  } finally {
    if (item) setUpdating(item.id, false); else updateAllPending.value = false;
  }
}

function selectedConfigSnapshot(sourceId: string, digest: string): void {
  const active = uiStores.config.active.value;
  const subscriptions = active?.document.subscriptions;
  if (!active || !subscriptions || typeof subscriptions !== "object") return;
  const sourceItems = (subscriptions as { sources?: unknown }).sources;
  if (!Array.isArray(sourceItems)) return;
  const sources = sourceItems.map((raw) => raw && typeof raw === "object"
    ? { ...(raw as Record<string, unknown>), enabled: (raw as Record<string, unknown>).source_id === sourceId }
    : raw);
  uiStores.config.load({
    ...active,
    observedConfigDigest: digest,
    activeConfigDigest: digest,
    candidateSequence: active.candidateSequence + 1,
    document: { ...active.document, subscriptions: { ...(subscriptions as Record<string, unknown>), sources } },
  });
}

async function selectSource(item: SubscriptionDto): Promise<void> {
  if (item.enabled || selectingSourceId.value) return;
  const previous = selectedSourceId.value;
  selectedSourceId.value = item.id;
  selectingSourceId.value = item.id;
  const id = `subscription-select-${item.id}`;
  beginFeedback(id, "subscription-config");
  try {
    const digest = uiStores.config.baseDigest.value ?? "0".repeat(64);
    const response = await uploadPrivatePayload(host, "subscription", "config-mutate", JSON.stringify({ expected_config_digest: digest, mutation: { type: "select_source", source_id: item.id } }));
    const nextDigest = (response as { result?: { observed_config_digest?: unknown } }).result?.observed_config_digest;
    if (typeof nextDigest !== "string" || !/^[a-f0-9]{64}$/.test(nextDigest)) throw new Error("missing config digest");
    selectedConfigSnapshot(item.id, nextDigest);
    uiStores.runtime.replaceSubscriptions(items.value.map((source) => ({ ...source, enabled: source.id === item.id })));
    finishFeedback(id, true, `已切换到“${item.name}”`);
  } catch {
    selectedSourceId.value = previous;
    finishFeedback(id, false, "订阅切换失败");
  } finally {
    selectingSourceId.value = undefined;
  }
}

async function remove(item: SubscriptionDto): Promise<void> {
  const id = `subscription-remove-${item.id}`;
  beginFeedback(id, "subscription-config");
  try {
    const digest = uiStores.config.baseDigest.value ?? "0".repeat(64);
    await uploadPrivatePayload(host, "subscription", "config-mutate", JSON.stringify({ expected_config_digest: digest, mutation: { type: "remove_source", source_id: item.id } }));
    pendingDelete.value = undefined;
    await load();
    finishFeedback(id, true, `“${item.name}”已删除`);
  } catch {
    finishFeedback(id, false, "订阅删除失败");
  }
}

async function move(item: SubscriptionDto, direction: -1 | 1): Promise<void> {
  const index = items.value.findIndex((value) => value.id === item.id);
  if ((direction < 0 && index <= 0) || (direction > 0 && index >= items.value.length - 1)) return;
  const target = direction < 0 ? items.value[index - 1] : items.value[index + 2];
  const id = `subscription-move-${item.id}`;
  beginFeedback(id, "subscription-config");
  try {
    const mutation = { type: "move_source", source_id: item.id, ...(target ? { before_source_id: target.id } : {}) };
    await uploadPrivatePayload(host, "subscription", "config-mutate", JSON.stringify({ expected_config_digest: uiStores.config.baseDigest.value ?? "0".repeat(64), mutation }));
    await load();
    finishFeedback(id, true, "订阅顺序已更新");
  } catch {
    finishFeedback(id, false, "订阅排序失败");
  }
}

function openActions(item: SubscriptionDto): void {
  selectedSourceId.value = item.id;
  activeItem.value = item;
  actionSheetOpen.value = true;
}
function selectAction(_selected: unknown, index: number): void {
  const item = activeItem.value;
  const action = actionKeys.value[index];
  if (item && action) queuedAction.value = { item, action };
}
function finishActionSheet(): void {
  const queued = queuedAction.value;
  queuedAction.value = undefined;
  activeItem.value = undefined;
  if (!queued) return;
  void nextTick().then(async () => {
    switch (queued.action) {
      case "update": await update(queued.item); break;
      case "edit": openEditor(queued.item); break;
      case "move-up": await move(queued.item, -1); break;
      case "move-down": await move(queued.item, 1); break;
      case "delete": pendingDelete.value = queued.item; break;
    }
  });
}

async function previewImport(): Promise<void> {
  if (!importText.value.trim()) return;
  const id = "subscription-import-preview";
  beginFeedback(id, "subscription-import");
  try {
    const raw = await uploadPrivatePayload(host, "subscription", "subscription-import-preview", JSON.stringify({ expected_config_digest: uiStores.config.baseDigest.value ?? "0".repeat(64), document: { content: importText.value, format_hint: "auto" } }));
    const envelope = raw as { result?: unknown };
    if (!envelope.result || typeof envelope.result !== "object") throw new Error("invalid preview");
    importPreview.value = envelope.result as Readonly<Record<string, unknown>>;
    finishFeedback(id, true, "预览完成");
  } catch {
    finishFeedback(id, false, "订阅内容无法预览");
  }
}

async function applyImport(): Promise<void> {
  const candidate = importPreview.value?.candidate_digest ?? importPreview.value?.candidate_config_digest;
  if (typeof candidate !== "string" || !/^[a-f0-9]{64}$/.test(candidate)) return;
  const id = "subscription-import-apply";
  beginFeedback(id, "subscription-import");
  try {
    await uploadPrivatePayload(host, "subscription", "subscription-import-apply", JSON.stringify({ expected_config_digest: uiStores.config.baseDigest.value ?? "0".repeat(64), candidate_digest: candidate, document: { content: importText.value, format_hint: "auto" } }));
    closeImport();
    finishFeedback(id, true, "导入已发布");
  } catch {
    finishFeedback(id, false, "导入发布失败");
  }
}

watch(importText, () => { importPreview.value = undefined; });
onActivated(() => { void load().catch(() => undefined); });
</script>

<template>
  <section class="page subscriptions-page">
    <div class="page-heading subscriptions-heading">
      <div><h2>订阅</h2></div>
      <div class="heading-actions">
        <TButton size="small" shape="square" variant="outline" theme="default" :loading="updateAllPending" :disabled="updateAllPending || items.length === 0" title="更新全部" @click="update()"><IconRefresh :size="18" /></TButton>
      </div>
    </div>

    <OperationBanner v-if="currentFeedback" :phase="currentFeedback.phase" :message="currentFeedback.message ?? ''" @dismiss="feedbackId && operations.clear(feedbackId)" />

    <PageState v-if="loading && items.length === 0" kind="loading" title="正在加载订阅源" />
    <PageState v-else-if="loadError && items.length === 0" kind="error" title="订阅源加载失败" detail="控制服务暂时不可用" action-label="重试" @action="load" />
    <PageState v-else-if="items.length === 0" kind="empty" title="还没有订阅源" detail="点击右下角按钮添加订阅链接" />
    <TRadioGroup v-else v-model="selectedSourceId" class="source-list" name="subscription-source" icon="dot">
      <article v-for="item in items" :key="item.id" class="source-card" :data-state="presentation(item).tone" :data-selected="selectedSourceId === item.id" @click="selectSource(item)">
        <TRadio class="source-selector" :value="item.id" icon="dot" :block="false" borderless :disabled="Boolean(selectingSourceId)" @click.stop="selectSource(item)" />
        <div class="source-main">
          <div class="source-title-row">
            <strong>{{ item.name }}</strong>
            <div class="source-actions" @click.stop>
              <TButton size="small" shape="square" variant="text" theme="default" :loading="isUpdating(item.id)" :disabled="isUpdating(item.id)" title="更新订阅" @click="update(item)"><IconRefresh :size="18" /></TButton>
              <TButton size="small" shape="square" variant="text" theme="default" title="更多操作" @click="openActions(item)"><IconDotsVertical :size="19" /></TButton>
            </div>
          </div>
          <div class="source-quota-track" :data-empty="presentation(item).quotaPercent === undefined"><i :style="{ width: `${presentation(item).quotaPercent ?? 0}%` }" /></div>
          <span>{{ presentation(item).summary }}</span>
          <div class="source-detail">
            <small>{{ presentation(item).detail }}</small>
            <TTag v-if="presentation(item).warning" size="small" variant="light" theme="warning">{{ presentation(item).warning }}</TTag>
          </div>
        </div>
      </article>
    </TRadioGroup>

    <TButton class="subscription-add-fab" size="large" shape="circle" theme="primary" title="添加订阅" @click="openEditor()"><IconPlus :size="26" stroke-width="2" /></TButton>

    <TActionSheet v-model="actionSheetOpen" class="subscription-actions-sheet" theme="list" align="left" show-cancel cancel-text="取消" :description="activeItem?.name ?? ''" :items="actionItems" @selected="selectAction" @close="finishActionSheet" @cancel="activeItem = undefined" />

    <TPopup v-model="editorOpen" placement="bottom" :duration="160" destroy-on-close @visible-change="(visible) => { if (!visible) closeEditor(); }">
      <div class="subscription-editor">
        <div class="editor-heading"><h3>{{ editing ? "编辑订阅" : "添加订阅" }}</h3><span>{{ editing ? "留空链接表示不修改" : "名称和 HTTPS 链接" }}</span></div>
        <TInput v-model="name" label="名称" placeholder="例如：主订阅" :maxlength="256" />
        <TInput v-if="!editing" v-model="url" label="订阅链接" placeholder="仅限 HTTPS 链接" type="url" />
        <TInput v-else v-model="url" label="替换链接（可选）" placeholder="留空表示不修改" type="url" />
        <small v-if="formError" class="form-error">{{ formError }}</small>
        <TButton v-if="!editing" class="content-import-entry" block variant="text" theme="default" @click="openContentImport"><IconFileImport :size="18" />从文本内容导入</TButton>
        <div class="editor-actions"><TButton variant="outline" :disabled="saving" @click="closeEditor">取消</TButton><TButton theme="primary" :loading="saving" :disabled="saving" @click="save">保存</TButton></div>
      </div>
    </TPopup>

    <TPopup v-model="importOpen" placement="bottom" :duration="160" destroy-on-close @visible-change="(visible) => { if (!visible) closeImport(); }">
      <div class="subscription-editor import-editor">
        <div class="editor-heading"><h3>从内容导入</h3><span>URI、Base64、YAML 或 JSON</span></div>
        <TTextarea v-model="importText" placeholder="粘贴订阅内容" :maxlength="786432" :autosize="{ minRows: 6, maxRows: 12 }" />
        <div v-if="importPreview" class="import-preview"><strong>预览已生成</strong><span>接受 {{ importPreview.accepted ?? "--" }} · 跳过 {{ importPreview.skipped ?? "--" }} · 重复 {{ importPreview.duplicate ?? "--" }}</span></div>
        <div class="editor-actions"><TButton variant="outline" :disabled="!importText.trim()" @click="previewImport">预览</TButton><TButton theme="primary" :disabled="!importPreview" @click="applyImport">确认导入</TButton></div>
      </div>
    </TPopup>

    <ConfirmDialog :visible="Boolean(pendingDelete)" title="删除订阅源" :description="`确认删除“${pendingDelete?.name ?? ''}”？`" confirm-label="删除" @update:visible="(value) => { if (!value) pendingDelete = undefined; }" @confirm="pendingDelete && remove(pendingDelete)" />
  </section>
</template>
