<script setup lang="ts">
import { computed, onActivated, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { IconActivity, IconAdjustments, IconClock, IconDeviceMobile, IconFileText, IconPalette, IconRoute, IconWorld } from "@tabler/icons-vue";
import SettingsFieldControl, { type SettingsFieldControlDefinition } from "@/components/settings/SettingsFieldControl.vue";
import SettingsGroup from "@/components/settings/SettingsGroup.vue";
import SettingsPageHeader from "@/components/settings/SettingsPageHeader.vue";
import SettingsRow from "@/components/settings/SettingsRow.vue";
import SettingsSecondaryShell from "@/components/settings/SettingsSecondaryShell.vue";
import SettingsStatusBanner from "@/components/settings/SettingsStatusBanner.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import PageState from "@/components/ui/feedback/PageState.vue";
import StatusLine from "@/components/StatusLine.vue";
import Dialog from "@/components/ui/overlay/Dialog.vue";
import Button from "@/components/ui/primitives/Button.vue";
import Switch from "@/components/ui/primitives/Switch.vue";
import Textarea from "@/components/ui/primitives/Textarea.vue";
import Segmented from "@/components/ui/navigation/Segmented.vue";
import { useHost } from "@/bridge/context";
import { uploadPrivatePayload } from "@/bridge/private-payload";
import { runJson } from "@/bridge/command";
import { validatedQuery } from "@/model/client";
import { parseConfigDocument, serializeConfigDocument } from "@/model/config-json";
import { parseCapability, parseConfig, parseConfigSchema, type ConfigSchemaFieldDto } from "@/model/dto";
import { SETTINGS_SECTIONS, settingsFieldLabel, settingsSection, settingsSectionFields, type SettingsSectionKey } from "@/model/settings-presentation";
import { uiStores } from "@/runtime/store";
import { createOperationStore } from "@/runtime/operation";
import { useTheme } from "@/shell/theme";

const host = useHost();
const route = useRoute();
const router = useRouter();
const { mode: themeMode, setMode: setThemeMode } = useTheme();
const themeOptions = [
  { value: "system", label: "系统" },
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
] as const;
const loading = ref(false);
const error = ref("");
const fields = ref<readonly ConfigSchemaFieldDto[]>([]);
const capability = ref<ReadonlyMap<string, { status: string; reason: string }>>(new Map());
const validation = ref<Readonly<Record<string, unknown>>>();
const confirmApply = ref(false);
const conflict = ref(false);
const editorMode = ref<"form" | "json">("form");
const rawConfig = ref("");
const rawError = ref("");
const operations = createOperationStore();
const corePending = ref(false);
const coreRunning = ref(false);
const editorModeOptions = [
  { value: "form", label: "常用设置" },
  { value: "json", label: "专家 JSON" },
] as const;
const sectionIcons = {
  updates: IconClock,
  network: IconWorld,
  interfaces: IconDeviceMobile,
  routing: IconRoute,
  logging: IconFileText,
  advanced: IconAdjustments,
} as const;

const sectionKey = computed<SettingsSectionKey | undefined>(() => {
  const value = route.meta.settingsSection;
  return typeof value === "string" && SETTINGS_SECTIONS.some((section) => section.key === value) ? value as SettingsSectionKey : undefined;
});
const isSettingsHome = computed(() => route.path === "/settings");
const currentSection = computed(() => sectionKey.value ? settingsSection(sectionKey.value) : undefined);
const visibleFields = computed(() => sectionKey.value ? settingsSectionFields(fields.value, sectionKey.value) : []);
const availableSections = computed(() => SETTINGS_SECTIONS.filter((section) => settingsSectionFields(fields.value, section.key).length > 0));
const draftState = computed(() => uiStores.config.dirty.value ? "有未应用修改" : "已同步");
const activeDigest = computed(() => {
  const digest = uiStores.config.active.value?.activeConfigDigest;
  return digest ? digest.slice(0, 10) + "…" : "未知";
});

async function syncCoreStatus(): Promise<void> {
  try {
    const response = await runJson(host, { id: "core.status" }) as { result?: { core_state?: unknown } };
    coreRunning.value = response.result?.core_state === "ready";
  } catch {
    coreRunning.value = false;
  }
}

function changeThemeMode(context: { value: string | number }): void {
  if (context.value === "system" || context.value === "light" || context.value === "dark") setThemeMode(context.value);
}

async function toggleCore(): Promise<void> {
  if (corePending.value) return;
  corePending.value = true;
  const operationId = "core-service";
  operations.begin(operationId, "core");
  operations.update(operationId, "running");
  try {
    await runJson(host, { id: coreRunning.value ? "core.stop" : "core.start", wait: true });
    coreRunning.value = !coreRunning.value;
    operations.update(operationId, "success", { message: coreRunning.value ? "核心已启用" : "核心已停止" });
  } catch {
    operations.update(operationId, "failure", { code: "NH-UI-CORE", message: "核心状态切换失败" });
  } finally {
    corePending.value = false;
  }
}

function getValue(path: string): unknown {
  return path.split(".").reduce<unknown>((current, key) => current && typeof current === "object" ? (current as Record<string, unknown>)[key] : undefined, uiStores.config.draft.value);
}

function updateValue(path: string, value: unknown): void {
  const document = structuredClone(uiStores.config.draft.value ?? {});
  const keys = path.split(".");
  let current = document as Record<string, unknown>;
  keys.slice(0, -1).forEach((key) => {
    const next = current[key];
    if (!next || typeof next !== "object" || Array.isArray(next)) current[key] = {};
    current = current[key] as Record<string, unknown>;
  });
  const last = keys.at(-1);
  if (last) current[last] = value;
  uiStores.config.edit(document);
  validation.value = undefined;
  conflict.value = false;
}

function editRawConfig(value: string | number): void {
  rawConfig.value = String(value);
  rawError.value = "";
  validation.value = undefined;
  conflict.value = false;
}

function syncRawDraft(): boolean {
  try {
    uiStores.config.edit(parseConfigDocument(rawConfig.value));
    rawError.value = "";
    return true;
  } catch (cause) {
    rawError.value = cause instanceof Error ? cause.message : "配置 JSON 无效";
    return false;
  }
}

function changeEditorMode(context: { value: string | number }): void {
  const value = context.value;
  if (value === editorMode.value) return;
  if (value === "json") {
    rawConfig.value = uiStores.config.draft.value ? serializeConfigDocument(uiStores.config.draft.value) : "{}";
    rawError.value = "";
    editorMode.value = "json";
    return;
  }
  if (value === "form" && syncRawDraft()) editorMode.value = "form";
}

function definition(field: ConfigSchemaFieldDto): SettingsFieldControlDefinition {
  const raw = getValue(field.path);
  const valueType: SettingsFieldControlDefinition["valueType"] = field.options.length > 0
    ? "enum"
    : field.valueType === "bool" || field.valueType === "boolean"
      ? "bool"
      : field.valueType.includes("int") || field.valueType === "integer"
        ? "int"
        : "string";
  const status = field.capabilityKey ? capability.value.get(field.capabilityKey) : undefined;
  const disabledReason = field.readOnly
    ? "只读字段"
    : status && status.status !== "supported"
      ? status.status + ": " + status.reason
      : undefined;
  return {
    id: field.id,
    label: settingsFieldLabel(field) ?? "未命名设置",
    valueType,
    value: (raw as boolean | number | string | readonly string[] | undefined) ?? (valueType === "bool" ? false : valueType === "int" ? field.minimum ?? 0 : ""),
    options: field.options,
    ...(field.minimum === undefined ? {} : { minimum: field.minimum }),
    ...(field.maximum === undefined ? {} : { maximum: field.maximum }),
    ...(disabledReason ? { disabledReason } : {}),
  };
}

function impactLabel(value: unknown): string {
  if (value === "runtime_only") return "立即生效";
  if (value === "generation_activation") return "应用后重新激活代理核心";
  if (value === "network_plan") return "应用后更新网络接管计划";
  return "应用后由 daemon 决定生效方式";
}

async function load(): Promise<void> {
  loading.value = true;
  error.value = "";
  const timings: Record<string, number> = {};
  const timed = async <T>(name: string, task: Promise<T>): Promise<T> => {
    const started = performance.now();
    try { return await task; }
    finally { timings[name] = Math.round(performance.now() - started); }
  };
  try {
    const [config, schema, report] = await Promise.all([
      timed("config.get", validatedQuery(host, { id: "config.get" }, parseConfig)),
      timed("config.schema", validatedQuery(host, { id: "config.schema" }, parseConfigSchema)),
      timed("capability.get", validatedQuery(host, { id: "capability.get" }, parseCapability)),
    ]);
    uiStores.config.load(config);
    fields.value = schema.fields;
    capability.value = new Map(report.items.map((item) => [item.key, { status: item.status, reason: item.reasonCode }]));
    rawConfig.value = serializeConfigDocument(config.document);
    rawError.value = "";
    conflict.value = false;
  } catch (cause) {
    console.info("[NetHop timing] settings.load", timings);
    error.value = cause instanceof Error ? `配置或设备能力加载失败：${cause.message}` : "配置或设备能力加载失败";
  } finally {
    loading.value = false;
  }
}

async function validateConfig(): Promise<void> {
  if (editorMode.value === "json" && !syncRawDraft()) return;
  const draft = uiStores.config.draft.value;
  if (!draft) return;
  operations.begin("config-validate", "config");
  operations.update("config-validate", "running");
  try {
    const raw = await uploadPrivatePayload(host, "config", "config-validate", JSON.stringify({ expected_config_digest: uiStores.config.baseDigest.value, document: draft }));
    const envelope = raw as { result?: unknown };
    if (!envelope.result || typeof envelope.result !== "object") throw new Error("invalid validation");
    validation.value = envelope.result as Readonly<Record<string, unknown>>;
    conflict.value = false;
    operations.update("config-validate", "success", { message: "配置验证通过" });
  } catch {
    validation.value = undefined;
    conflict.value = true;
    operations.update("config-validate", "conflict", { message: "验证失败或配置已被外部修改" });
  }
}

async function applyConfig(): Promise<void> {
  const draft = uiStores.config.draft.value;
  if (!draft || !validation.value) return;
  confirmApply.value = false;
  operations.begin("config-apply", "config");
  operations.update("config-apply", "running");
  try {
    await uploadPrivatePayload(host, "config", "config-apply", JSON.stringify({ expected_config_digest: uiStores.config.baseDigest.value, document: draft }));
    operations.update("config-apply", "success", { message: "配置已应用" });
    await load();
  } catch {
    operations.update("config-apply", "failure", { message: "应用失败，旧运行配置保持不变" });
  }
}

async function reload(): Promise<void> {
  if (uiStores.config.dirty.value && !window.confirm("重新加载会丢弃当前草稿，是否继续？")) return;
  operations.begin("config-reload", "config");
  operations.update("config-reload", "running");
  try {
    await runJson(host, { id: "config.reload" });
    await load();
    operations.update("config-reload", "success", { message: "配置文件已重新加载" });
  } catch {
    operations.update("config-reload", "failure", { message: "外部配置无效，当前运行配置未改变" });
  }
}

onActivated(() => {
  void syncCoreStatus();
  if (!uiStores.config.dirty.value) void load();
});
</script>

<template>
  <section class="page settings-page">
    <PageState v-if="loading" :model="{ type: 'loading', title: '正在加载配置' }" />
    <PageState v-else-if="error" :model="{ type: 'error', title: '设置不可用', detail: error }" action-label="重试" @action="load" />
    <template v-else>
      <div class="settings-stage">
        <div class="settings-base" :class="{ 'settings-base--pushed': !isSettingsHome }">
          <SettingsPageHeader title="设置" description="只显示当前设备真实支持的配置" :loading="loading" :can-validate="Boolean(uiStores.config.dirty.value)" @reload="reload" @validate="validateConfig" />
          <SettingsStatusBanner :title="draftState" :detail="'活动摘要 ' + activeDigest" :state="uiStores.config.dirty.value ? 'degraded' : 'ready'" />
          <SettingsGroup title="界面">
            <SettingsRow title="主题">
              <template #icon><IconPalette :size="15" /></template>
              <template #trailing><Segmented :model-value="themeMode" :options="themeOptions" aria-label="主题" @change="changeThemeMode" /></template>
            </SettingsRow>
            <SettingsRow title="运维" description="日志、备份、诊断和版本检查" arrow clickable @activate="router.push('/operations')">
              <template #icon><IconActivity :size="15" /></template>
            </SettingsRow>
          </SettingsGroup>
          <SettingsGroup title="核心服务" description="核心生命周期独立于概览页流量接管开关">
            <SettingsRow title="sing-box 核心" :description="coreRunning ? '核心运行中，可快速恢复流量接管' : '核心已停止，启用代理时将重新启动'">
              <template #icon><IconActivity :size="15" /></template>
              <template #trailing><Switch :model-value="coreRunning" :loading="corePending" aria-label="启用 sing-box 核心" @change="toggleCore" /></template>
            </SettingsRow>
          </SettingsGroup>
          <SettingsGroup title="配置" description="来自 daemon schema 的真实设置">
            <SettingsRow v-for="section in availableSections" :key="section.key" :title="section.title" :description="section.description" arrow clickable @activate="router.push(`/settings/${section.key}`)">
              <template #icon><component :is="sectionIcons[section.key]" :size="15" /></template>
            </SettingsRow>
          </SettingsGroup>
        </div>

        <SettingsSecondaryShell :visible="!isSettingsHome" :title="currentSection?.title ?? '设置'" :description="currentSection?.description ?? '只显示当前设备真实支持的配置'" :loading="loading" :can-validate="Boolean(uiStores.config.dirty.value)" @back="router.push('/settings')" @reload="reload" @validate="validateConfig">
          <SettingsStatusBanner :title="draftState" :detail="'活动摘要 ' + activeDigest" :state="uiStores.config.dirty.value ? 'degraded' : 'ready'" />
          <OperationBanner v-for="operation in Object.values(operations.byId)" :key="operation.id" :phase="operation.phase" :message="operation.message ?? ''" />
          <div v-if="conflict" class="settings-notice settings-notice--danger"><div><strong>检测到配置冲突</strong><span>重新加载获取最新配置，或保留当前草稿后重新验证。</span></div><Button size="s" variant="outline" @click="reload">重新加载</Button></div>
          <div v-if="validation" class="settings-notice settings-notice--success"><div><strong>验证通过</strong><span>影响级别：{{ impactLabel(validation.apply_impact) }}</span><span>预计中断：{{ validation.estimated_disruption ?? "由 daemon 决定" }}</span></div><Button size="s" variant="primary" @click="confirmApply = true">应用配置</Button></div>

          <PageState v-if="visibleFields.length === 0" :model="{ type: 'empty', title: '设备未提供此设置', detail: '当前 daemon schema 没有可编辑字段，因此不会显示虚假的控件。' }" />
          <template v-else>
            <div class="config-editor-toolbar"><div><strong>设备配置</strong><small>每个字段都经过 schema、capability 和事务校验</small></div><Segmented class="editor-mode" :model-value="editorMode" :options="editorModeOptions" aria-label="配置编辑模式" @change="changeEditorMode" /></div>
            <div v-if="editorMode === 'form'" class="schema-grid">
              <div v-for="field in visibleFields" :key="field.id" class="schema-field-wrap">
                <SettingsFieldControl :field="definition(field)" @change="(value) => updateValue(field.path, value)" />
                <StatusLine v-if="field.experimental" status="degraded" label="实验功能，是否可用由设备能力决定" />
              </div>
            </div>
            <div v-else class="raw-config-editor"><Textarea :model-value="rawConfig" variant="outline" :min-rows="14" :max-rows="28" resize="vertical" placeholder="仅供高级用户编辑完整配置 JSON" @update:model-value="editRawConfig" /><span v-if="rawError" class="form-error">{{ rawError }}</span></div>
          </template>
        </SettingsSecondaryShell>
      </div>
    </template>
    <Dialog v-model="confirmApply" aria-label="应用配置">
      <template #title>应用配置</template>
      <p>此操作将由 daemon 事务化发布，{{ String(validation?.estimated_disruption ?? '可能影响当前代理连接') }}。</p>
      <template #actions>
        <Button variant="outline" @click="confirmApply = false">取消</Button>
        <Button variant="primary" @click="applyConfig">应用</Button>
      </template>
    </Dialog>
  </section>
</template>

<style scoped>
.settings-page { max-width: 820px; }
.settings-stage { position: relative; min-height: calc(100dvh - 112px); overflow: hidden; border-radius: 16px; }
.settings-base { position: absolute; z-index: 1; inset: 0; overflow: auto; min-height: 0; background: var(--nh-bg); transition: transform .35s cubic-bezier(.4, 0, .2, 1); will-change: transform; }
.settings-base--pushed { pointer-events: none; transform: translateX(-100%); }
.editor-mode { min-width: 168px; }
.config-editor-toolbar { display: flex; align-items: center; justify-content: space-between; margin: 3px 0 12px; gap: 12px; }
.config-editor-toolbar > div:first-child { display: flex; min-width: 0; flex-direction: column; gap: 3px; }
.config-editor-toolbar strong { font-size: 13px; }
.config-editor-toolbar small { color: var(--nh-muted); font-size: 10px; }
.schema-grid { display: flex; overflow: hidden; border: 1px solid var(--nh-border); border-radius: 13px; background: var(--nh-surface); flex-direction: column; }
.schema-field-wrap { position: relative; display: flex; min-width: 0; padding: 0; border-bottom: 0; flex-direction: column; }
.schema-field-wrap + .schema-field-wrap::before { position: absolute; z-index: 1; top: 0; right: 13px; left: 13px; border-top: 1px solid var(--nh-border); content: ""; }
.schema-field-wrap > .status-line { margin: -1px 13px 10px; }
.settings-notice { display: flex; align-items: center; justify-content: space-between; margin: 0 0 12px; padding: 11px 12px; border: 1px solid var(--nh-border); border-radius: 11px; gap: 12px; }
.settings-notice > div { display: flex; min-width: 0; flex-direction: column; gap: 3px; }
.settings-notice strong { font-size: 12px; }
.settings-notice > div span { color: var(--nh-muted); font-size: 10px; line-height: 1.35; }
.settings-notice--danger { border-color: color-mix(in srgb, var(--nh-danger) 45%, var(--nh-border)); background: color-mix(in srgb, var(--nh-danger) 7%, var(--nh-surface)); }
.settings-notice--success { border-color: color-mix(in srgb, var(--nh-success) 45%, var(--nh-border)); background: color-mix(in srgb, var(--nh-success) 7%, var(--nh-surface)); }
@media (max-width: 560px) { .config-editor-toolbar { align-items: stretch; flex-direction: column; } .editor-mode { width: 100%; } }
@media (prefers-reduced-motion: reduce) { .settings-base { transition: none; } }
</style>
