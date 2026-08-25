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
import { computed, h, nextTick, onActivated, ref, watch } from "vue";
import { runJson } from "@/bridge/command";
import { useHost } from "@/bridge/context";
import { uploadPrivatePayload } from "@/bridge/private-payload";
import OptionDropdown from "@/components/ui/composite/OptionDropdown.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import PageState from "@/components/ui/feedback/PageState.vue";
import Segmented from "@/components/ui/navigation/Segmented.vue";
import Input from "@/components/ui/primitives/Input.vue";
import Textarea from "@/components/ui/primitives/Textarea.vue";
import Field from "@/components/ui/form/Field.vue";
import Checkbox from "@/components/ui/primitives/Checkbox.vue";
import Radio from "@/components/ui/primitives/Radio.vue";
import Button from "@/components/ui/primitives/Button.vue";
import IconButton from "@/components/ui/primitives/IconButton.vue";
import Tag from "@/components/ui/primitives/Tag.vue";
import Popup from "@/components/ui/overlay/Popup.vue";
import ActionSheet from "@/components/ui/overlay/ActionSheet.vue";
import Dialog from "@/components/ui/overlay/Dialog.vue";
import Disclosure from "@/components/ui/layout/Disclosure.vue";
import { validatedQuery } from "@/model/client";
import { parseConfig, parseSubscriptionSnapshot, type SourceUpdateHistoryDto, type SubscriptionDto, type SubscriptionModeDto } from "@/model/dto";
import { presentSubscription, type SubscriptionPresentation } from "@/model/subscription-presentation";
import {
  buildSourceSettings,
  defaultSourceEditorSettings,
  parseSourceEditorSettings,
  subscriptionFormatHints,
  subscriptionProtocols,
  subscriptionRequestProfiles,
  type SourceEditorSettings,
  type SubscriptionFormatHint,
  type SubscriptionRequestProfile,
} from "@/model/subscription-settings";
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
const sourceSettings = ref<SourceEditorSettings>({ ...defaultSourceEditorSettings, protocols: [] });
const formError = ref("");
const saving = ref(false);
const importText = ref("");
const importPreview = ref<Readonly<Record<string, unknown>>>();
const updatingIds = ref<ReadonlySet<string>>(new Set());
const updateAllPending = ref(false);
const selectingSourceId = ref<string>();
const modePending = ref(false);
const singleTargetOpen = ref(false);
const advancedSettingsOpen = ref(false);
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
const mode = computed(() => uiStores.runtime.subscriptionMode.value ?? "single");
const modeOptions = [{ value: "single", label: "单订阅" }, { value: "merge", label: "合并" }] as const;
const requestProfileOptions = subscriptionRequestProfiles.map((value) => ({ value, label: ({ generic: "通用", mihomo: "Mihomo", clash_standard: "Clash Standard", surfboard: "Surfboard", sing_box: "sing-box", sing_box_android: "sing-box Android" } as const)[value] }));
const formatHintOptions = subscriptionFormatHints.map((value) => ({ value, label: ({ auto: "自动识别", uri_list: "URI 列表", base64_list: "Base64 列表", clash_yaml: "Clash YAML", singbox_json: "sing-box JSON", surfboard_ini: "Surfboard INI" } as const)[value] }));
const presentations = computed<Readonly<Record<string, SubscriptionPresentation>>>(() => Object.fromEntries(items.value.map((item) => [item.id, presentSubscription(item, clockSeconds.value)])));
const sourceHistory = computed(() => uiStores.config.active.value?.sourceHistory.slice(0, 8) ?? []);
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
function historySourceName(entry: SourceUpdateHistoryDto): string { return uiStores.runtime.subscriptionsById.value[entry.sourceId]?.name ?? entry.sourceId; }
function historyTime(entry: SourceUpdateHistoryDto): string { return new Intl.DateTimeFormat("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }).format(entry.attemptedAtWallSeconds * 1_000); }
function historySummary(entry: SourceUpdateHistoryDto): string {
  if (entry.health === "failed") return entry.diagnosticCode ?? "更新失败";
  const count = entry.accepted + entry.duplicate;
  return entry.usingLastKnownGood ? `沿用缓存 · ${count} 个节点` : `${count} 个节点 · 排除 ${entry.rejected}`;
}
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
    const [config, snapshot] = await Promise.all([
      validatedQuery(host, { id: "config.get" }, parseConfig),
      validatedQuery(host, { id: "subscription.mode.get" }, parseSubscriptionSnapshot),
    ]);
    uiStores.config.load(config);
    const result = snapshot.sources.map((source) => {
      const status = config.sourceStatus.find((entry) => entry.sourceId === source.id);
      return { ...source, ...(status ? { state: status.health, status } : {}) };
    });
    clockSeconds.value = Math.floor(Date.now() / 1_000);
    uiStores.runtime.loadSubscriptionSnapshot({ ...snapshot, sources: result });
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
  sourceSettings.value = item && uiStores.config.active.value
    ? parseSourceEditorSettings(uiStores.config.active.value.document, item.id)
    : { ...defaultSourceEditorSettings, protocols: [] };
  formError.value = "";
}
function closeEditor(): void {
  editorOpen.value = false;
  editing.value = undefined;
  name.value = "";
  url.value = "";
  sourceSettings.value = { ...defaultSourceEditorSettings, protocols: [] };
  formError.value = "";
}

function updateRequestProfile(value: string): void {
  if (subscriptionRequestProfiles.includes(value as SubscriptionRequestProfile)) sourceSettings.value.requestProfile = value as SubscriptionRequestProfile;
}
function updateFormatHint(value: string): void {
  if (subscriptionFormatHints.includes(value as SubscriptionFormatHint)) sourceSettings.value.formatHint = value as SubscriptionFormatHint;
}
function updateProtocol(protocol: string, checked: boolean): void {
  const next = new Set(sourceSettings.value.protocols);
  if (checked) next.add(protocol); else next.delete(protocol);
  sourceSettings.value.protocols = subscriptionProtocols.filter((value) => next.has(value));
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
  let settings: Readonly<Record<string, unknown>>;
  try {
    settings = buildSourceSettings(sourceSettings.value, editing.value ? "update" : "add");
  } catch (error) {
    formError.value = error instanceof Error ? error.message : "高级设置无效";
    return;
  }
  const id = "subscription-save";
  const wasEditing = Boolean(editing.value);
  beginFeedback(id, "subscription-config");
  saving.value = true;
  try {
    const digest = uiStores.config.baseDigest.value ?? "0".repeat(64);
    const mutation = editing.value
      ? { type: "update_source", source_id: editing.value.id, name: nextName, ...(nextUrl ? { url: nextUrl } : {}), settings }
      : { type: "add_source", name: nextName, url: nextUrl, settings };
    await uploadPrivatePayload(host, "config", "config-mutate", JSON.stringify({ expected_config_digest: digest, mutation }));
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

async function selectSource(item: SubscriptionDto): Promise<void> {
  if (mode.value !== "single" || item.active || selectingSourceId.value) return;
  selectingSourceId.value = item.id;
  const id = `subscription-select-${item.id}`;
  beginFeedback(id, "subscription-config");
  try {
    await runJson(host, { id: "subscription.select", sourceId: item.id, expectedDigest: uiStores.runtime.subscriptionConfigDigest.value ?? uiStores.config.baseDigest.value ?? "0".repeat(64) });
    await load();
    finishFeedback(id, true, `已切换到“${item.name}”`);
  } catch {
    finishFeedback(id, false, "订阅切换失败");
  } finally {
    selectingSourceId.value = undefined;
  }
}

async function setSourceEnabled(item: SubscriptionDto, enabled: boolean): Promise<void> {
  if (mode.value !== "merge" || selectingSourceId.value || item.active === enabled) return;
  selectingSourceId.value = item.id;
  const id = `subscription-enabled-${item.id}`;
  beginFeedback(id, "subscription-config");
  try {
    await runJson(host, { id: "subscription.set-enabled", sourceId: item.id, enabled, expectedDigest: uiStores.runtime.subscriptionConfigDigest.value ?? "0".repeat(64) });
    await load();
    finishFeedback(id, true, enabled ? `已加入“${item.name}”` : `已停用“${item.name}”`);
  } catch {
    finishFeedback(id, false, enabled ? "订阅启用失败" : "至少保留一个有效订阅");
  } finally { selectingSourceId.value = undefined; }
}

async function commitMode(next: SubscriptionModeDto, sourceId?: string): Promise<void> {
  modePending.value = true;
  const id = "subscription-mode";
  beginFeedback(id, "subscription-mode");
  try {
    await runJson(host, { id: "subscription.mode.set", mode: next, ...(sourceId ? { sourceId } : {}), expectedDigest: uiStores.runtime.subscriptionConfigDigest.value ?? "0".repeat(64) });
    singleTargetOpen.value = false;
    await load();
    finishFeedback(id, true, next === "merge" ? "已启用订阅合并" : "已切换为单订阅");
  } catch { finishFeedback(id, false, "订阅模式切换失败"); }
  finally { modePending.value = false; }
}

function changeMode(context: { value: string | number }): void {
  const next = context.value;
  if ((next !== "single" && next !== "merge") || next === mode.value || modePending.value) return;
  if (next === "merge") { void commitMode("merge"); return; }
  const active = items.value.filter((item) => item.active);
  if (active.length === 1) void commitMode("single", active[0]!.id);
  else singleTargetOpen.value = true;
}

async function remove(item: SubscriptionDto): Promise<void> {
  const id = `subscription-remove-${item.id}`;
  beginFeedback(id, "subscription-config");
  try {
    const digest = uiStores.config.baseDigest.value ?? "0".repeat(64);
    await uploadPrivatePayload(host, "config", "config-mutate", JSON.stringify({ expected_config_digest: digest, mutation: { type: "remove_source", source_id: item.id } }));
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
    await uploadPrivatePayload(host, "config", "config-mutate", JSON.stringify({ expected_config_digest: uiStores.config.baseDigest.value ?? "0".repeat(64), mutation }));
    await load();
    finishFeedback(id, true, "订阅顺序已更新");
  } catch {
    finishFeedback(id, false, "订阅排序失败");
  }
}

function openActions(item: SubscriptionDto): void {
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
        <IconButton size="s" variant="outline" aria-label="更新全部" :loading="updateAllPending" :disabled="updateAllPending || items.length === 0" title="更新全部" @click="update()"><IconRefresh :size="18" /></IconButton>
      </div>
    </div>

    <OperationBanner v-if="currentFeedback" :phase="currentFeedback.phase" :message="currentFeedback.message ?? ''" @dismiss="feedbackId && operations.clear(feedbackId)" />

    <section class="subscription-mode-panel">
      <div><strong>订阅模式</strong><span>{{ mode === "single" ? "只使用一个订阅源" : "合并已启用订阅源的节点" }}</span></div>
      <Segmented :model-value="mode" :options="modeOptions" :disabled="modePending" @change="changeMode" />
    </section>

    <PageState v-if="loading && items.length === 0" :model="{ type: 'loading', title: '正在加载订阅源' }" />
    <PageState v-else-if="loadError && items.length === 0" :model="{ type: 'error', title: '订阅源加载失败', detail: '控制服务暂时不可用' }" action-label="重试" @action="load" />
    <PageState v-else-if="items.length === 0" :model="{ type: 'empty', title: '还没有订阅源', detail: '点击右下角按钮添加订阅链接' }" />
    <div v-else class="source-list">
      <article v-for="item in items" :key="item.id" class="source-card" :data-state="presentation(item).tone" :data-selected="item.active" @click="mode === 'single' ? selectSource(item) : setSourceEnabled(item, !item.active)">
        <Radio v-if="mode === 'single'" class="source-selector" :model-value="item.active" :value="item.id" :aria-label="`选择 ${item.name}`" :disabled="Boolean(selectingSourceId)" @click.stop="selectSource(item)" />
        <Checkbox v-else class="source-selector" :model-value="item.active" :disabled="Boolean(selectingSourceId) || (!item.configured && !item.active)" :aria-label="`启用 ${item.name}`" @update:model-value="(checked) => setSourceEnabled(item, checked)" @click.stop />
        <div class="source-main">
          <div class="source-title-row">
            <strong>{{ item.name }}</strong>
            <div class="source-actions" @click.stop>
              <IconButton size="s" variant="text" aria-label="更新订阅" :loading="isUpdating(item.id)" :disabled="isUpdating(item.id)" title="更新订阅" @click="update(item)"><IconRefresh :size="18" /></IconButton>
              <IconButton size="s" variant="text" aria-label="更多操作" title="更多操作" @click="openActions(item)"><IconDotsVertical :size="19" /></IconButton>
            </div>
          </div>
          <div class="source-quota-track" :data-empty="presentation(item).quotaPercent === undefined"><i :style="{ width: `${presentation(item).quotaPercent ?? 0}%` }" /></div>
          <span>{{ presentation(item).summary }}</span>
          <div class="source-detail">
            <small>{{ presentation(item).detail }}</small>
            <Tag v-if="presentation(item).warning" size="s" variant="soft" tone="warning">{{ presentation(item).warning }}</Tag>
          </div>
        </div>
      </article>
    </div>

    <section v-if="sourceHistory.length > 0" class="source-history-section">
      <h3>最近更新</h3>
      <div class="source-history-list">
        <div v-for="entry in sourceHistory" :key="`${entry.sourceId}-${entry.attemptedAtWallSeconds}`" class="source-history-row" :data-health="entry.health">
          <i />
          <div><strong>{{ historySourceName(entry) }}</strong><span>{{ historySummary(entry) }}</span></div>
          <time>{{ historyTime(entry) }}</time>
        </div>
      </div>
    </section>

    <IconButton class="subscription-add-fab" size="l" shape="circle" variant="primary" aria-label="添加订阅" title="添加订阅" @click="openEditor()"><IconPlus :size="26" stroke-width="2" /></IconButton>

    <ActionSheet v-model="actionSheetOpen" :title="activeItem?.name ?? ''" :items="actionItems" @selected="(_item, index) => selectAction(undefined, index)" @close="finishActionSheet" @cancel="activeItem = undefined" />

    <Popup v-model="editorOpen" @visible-change="(visible) => { if (!visible) closeEditor(); }">
      <div class="subscription-editor">
        <div class="editor-heading"><h3>{{ editing ? "编辑订阅" : "添加订阅" }}</h3><span>{{ editing ? "留空链接表示不修改" : "名称和 HTTPS 链接" }}</span></div>
        <Field label="名称" required><Input v-model="name" variant="outline" placeholder="例如：主订阅" :maxlength="256" /></Field>
        <Field v-if="!editing" label="订阅链接" required><Input v-model="url" variant="outline" placeholder="仅限 HTTPS 链接" type="url" /></Field>
        <Field v-else label="替换链接（可选）"><Input v-model="url" variant="outline" placeholder="留空表示不修改" type="url" /></Field>
        <Disclosure v-model="advancedSettingsOpen" class="source-advanced-settings">
          <template #summary>高级设置</template>
          <div class="source-advanced-fields">
            <Field label="请求配置档"><OptionDropdown :model-value="sourceSettings.requestProfile" :options="requestProfileOptions" aria-label="请求配置档" @update:model-value="updateRequestProfile" /></Field>
            <Field label="内容格式"><OptionDropdown :model-value="sourceSettings.formatHint" :options="formatHintOptions" aria-label="内容格式" @update:model-value="updateFormatHint" /></Field>
            <div v-if="editing && sourceSettings.mirrorCount > 0" class="source-mirror-state"><span>已配置 {{ sourceSettings.mirrorCount }} 个镜像</span><Checkbox :model-value="sourceSettings.replaceMirrors" aria-label="替换镜像" @update:model-value="(checked) => { sourceSettings.replaceMirrors = checked; }">替换镜像</Checkbox></div>
            <Field v-if="!editing || sourceSettings.replaceMirrors" label="镜像链接"><Textarea v-model="sourceSettings.mirrorsText" variant="outline" placeholder="每行一个 HTTPS 链接" :min-rows="2" :max-rows="4" /></Field>
            <Field label="名称包含"><Textarea v-model="sourceSettings.includeNamesText" variant="outline" placeholder="每行一个匹配词" :min-rows="2" :max-rows="4" /></Field>
            <Field label="名称排除"><Textarea v-model="sourceSettings.excludeNamesText" variant="outline" placeholder="每行一个匹配词" :min-rows="2" :max-rows="4" /></Field>
            <div class="source-protocol-field"><span>协议过滤</span><div class="source-protocol-grid"><Checkbox v-for="protocol in subscriptionProtocols" :key="protocol" :model-value="sourceSettings.protocols.includes(protocol)" :aria-label="protocol" @update:model-value="(checked) => updateProtocol(protocol, checked)">{{ protocol }}</Checkbox></div></div>
          </div>
        </Disclosure>
        <small v-if="formError" class="form-error">{{ formError }}</small>
        <Button v-if="!editing" class="content-import-entry" variant="text" @click="openContentImport"><IconFileImport :size="18" />从文本内容导入</Button>
        <div class="editor-actions"><Button variant="outline" :disabled="saving" @click="closeEditor">取消</Button><Button variant="primary" :loading="saving" :disabled="saving" @click="save">保存</Button></div>
      </div>
    </Popup>

    <Popup v-model="importOpen" @visible-change="(visible) => { if (!visible) closeImport(); }">
      <div class="subscription-editor import-editor">
        <div class="editor-heading"><h3>从内容导入</h3><span>URI、Base64、YAML 或 JSON</span></div>
        <Textarea v-model="importText" variant="outline" placeholder="粘贴订阅内容" :maxlength="786432" :min-rows="6" :max-rows="12" />
        <div v-if="importPreview" class="import-preview"><strong>预览已生成</strong><span>接受 {{ importPreview.accepted ?? "--" }} · 跳过 {{ importPreview.skipped ?? "--" }} · 重复 {{ importPreview.duplicate ?? "--" }}</span></div>
        <div class="editor-actions"><Button variant="outline" :disabled="!importText.trim()" @click="previewImport">预览</Button><Button variant="primary" :disabled="!importPreview" @click="applyImport">确认导入</Button></div>
      </div>
    </Popup>

    <Popup v-model="singleTargetOpen">
      <div class="subscription-editor single-target-editor">
        <div class="editor-heading"><h3>选择单订阅</h3><span>合并模式中有多个活动来源，请明确保留一个</span></div>
        <Button v-for="item in items.filter((source) => source.active)" :key="item.id" class="single-target-row" variant="outline" :disabled="modePending" @click="commitMode('single', item.id)"><strong>{{ item.name }}</strong><span class="single-target-summary">{{ presentation(item).summary }}</span></Button>
        <Button variant="outline" :disabled="modePending" @click="singleTargetOpen = false">取消</Button>
      </div>
    </Popup>

    <Dialog :model-value="Boolean(pendingDelete)" aria-label="删除订阅源" @update:model-value="(value) => { if (!value) pendingDelete = undefined; }">
      <template #title>删除订阅源</template>
      <p>确认删除“{{ pendingDelete?.name ?? '' }}”？</p>
      <template #actions>
        <Button variant="outline" @click="pendingDelete = undefined">取消</Button>
        <Button variant="danger" @click="pendingDelete && remove(pendingDelete)">删除</Button>
      </template>
    </Dialog>
  </section>
</template>
