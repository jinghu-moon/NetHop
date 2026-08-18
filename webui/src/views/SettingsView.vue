<script setup lang="ts">
import { computed, onActivated, ref } from "vue";
import { useRouter } from "vue-router";
import { IconDeviceFloppy, IconRefresh, IconShieldCheck, IconTool } from "@tabler/icons-vue";
import { Button as TButton, Collapse as TCollapse, CollapsePanel as TCollapsePanel, Textarea as TTextarea } from "tdesign-mobile-vue";
import SchemaField, { type SchemaFieldDefinition } from "@/components/SchemaField.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import OptionDropdown from "@/components/OptionDropdown.vue";
import PageState from "@/components/PageState.vue";
import SegmentedControl from "@/components/SegmentedControl.vue";
import StatusLine from "@/components/StatusLine.vue";
import { useHost } from "@/bridge/context";
import { uploadPrivatePayload } from "@/bridge/private-payload";
import { runJson } from "@/bridge/command";
import { validatedQuery } from "@/model/client";
import { parseConfigDocument, serializeConfigDocument } from "@/model/config-json";
import { parseCapability, parseConfig, parseConfigSchema, type ConfigSchemaFieldDto } from "@/model/dto";
import { uiStores } from "@/runtime/store";
import { createOperationStore } from "@/runtime/operation";
import { useTheme } from "@/shell/theme";

const host = useHost();
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
const editorModeOptions = [
  { value: "form", label: "表单" },
  { value: "json", label: "JSON" },
] as const;

const groups = computed(() => {
  const result = new Map<string, ConfigSchemaFieldDto[]>();
  fields.value.filter((field) => !field.path.includes("[]") && !field.sensitive).forEach((field) => { const list = result.get(field.group) ?? []; list.push(field); result.set(field.group, list); });
  return [...result.entries()].map(([name, values]) => ({ name, fields: values.sort((a, b) => a.order - b.order) }));
});

function getValue(path: string): unknown {
  return path.split(".").reduce<unknown>((current, key) => current && typeof current === "object" ? (current as Record<string, unknown>)[key] : undefined, uiStores.config.draft.value);
}
function updateValue(path: string, value: unknown): void {
  const document = structuredClone(uiStores.config.draft.value ?? {});
  const keys = path.split("."); let current = document as Record<string, unknown>;
  keys.slice(0, -1).forEach((key) => { const next = current[key]; if (!next || typeof next !== "object" || Array.isArray(next)) current[key] = {}; current = current[key] as Record<string, unknown>; });
  const last = keys.at(-1); if (last) current[last] = value;
  uiStores.config.edit(document); validation.value = undefined;
}
function editRawConfig(value: string | number): void {
  rawConfig.value = String(value);
  rawError.value = "";
  validation.value = undefined;
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
function changeEditorMode(value: string | number): void {
  if (value === editorMode.value) return;
  if (value === "json") {
    const draft = uiStores.config.draft.value;
    rawConfig.value = draft ? serializeConfigDocument(draft) : "{}";
    rawError.value = "";
    editorMode.value = "json";
    return;
  }
  if (value === "form" && syncRawDraft()) editorMode.value = "form";
}
function definition(field: ConfigSchemaFieldDto): SchemaFieldDefinition {
  const raw = getValue(field.path);
  const valueType: SchemaFieldDefinition["valueType"] = field.valueType.includes("array") ? "array" : field.options.length > 0 ? "enum" : field.valueType === "bool" || field.valueType === "boolean" ? "bool" : field.valueType.includes("int") ? "int" : "string";
  const status = field.capabilityKey ? capability.value.get(field.capabilityKey) : undefined;
  const disabledReason = field.readOnly ? "只读字段" : status && status.status !== "supported" ? `${status.status}: ${status.reason}` : undefined;
  return { id: field.id, label: field.id, valueType, value: (raw as boolean | number | string | readonly string[] | undefined) ?? (valueType === "bool" ? false : valueType === "int" ? 0 : valueType === "array" ? [] : ""), options: field.options, ...(disabledReason ? { disabledReason } : {}) };
}
async function load(): Promise<void> {
  loading.value = true; error.value = "";
  try {
    const [config, schema, report] = await Promise.all([validatedQuery(host, { id: "config.get" }, parseConfig), validatedQuery(host, { id: "config.schema" }, parseConfigSchema), validatedQuery(host, { id: "capability.get" }, parseCapability)]);
    uiStores.config.load(config); fields.value = schema.fields; capability.value = new Map(report.items.map((item) => [item.key, { status: item.status, reason: item.reasonCode }])); rawConfig.value = serializeConfigDocument(config.document); rawError.value = ""; conflict.value = false;
  } catch { error.value = "配置或设备能力加载失败"; }
  finally { loading.value = false; }
}
async function validateConfig(): Promise<void> {
  if (editorMode.value === "json" && !syncRawDraft()) return;
  const draft = uiStores.config.draft.value; if (!draft) return;
  operations.begin("config-validate", "config"); operations.update("config-validate", "running");
  try { const raw = await uploadPrivatePayload(host, "config", "config-validate", JSON.stringify({ expected_config_digest: uiStores.config.baseDigest.value, document: draft })); const envelope = raw as { result?: unknown }; if (!envelope.result || typeof envelope.result !== "object") throw new Error("invalid validation"); validation.value = envelope.result as Readonly<Record<string, unknown>>; operations.update("config-validate", "success", { message: "配置验证通过" }); }
  catch { validation.value = undefined; conflict.value = true; operations.update("config-validate", "conflict", { message: "验证失败或配置已被外部修改" }); }
}
async function applyConfig(): Promise<void> {
  const draft = uiStores.config.draft.value; if (!draft || !validation.value) return;
  confirmApply.value = false; operations.begin("config-apply", "config"); operations.update("config-apply", "running");
  try { await uploadPrivatePayload(host, "config", "config-apply", JSON.stringify({ expected_config_digest: uiStores.config.baseDigest.value, document: draft })); operations.update("config-apply", "success", { message: "配置已应用" }); await load(); }
  catch { operations.update("config-apply", "failure", { message: "应用失败，旧运行配置保持不变" }); }
}
async function reload(): Promise<void> {
  if (uiStores.config.dirty.value && !window.confirm("重新加载会丢弃当前草稿，是否继续？")) return;
  operations.begin("config-reload", "config"); operations.update("config-reload", "running");
  try { await runJson(host, { id: "config.reload" }); await load(); operations.update("config-reload", "success", { message: "配置文件已重新加载" }); }
  catch { operations.update("config-reload", "failure", { message: "外部配置无效，当前运行配置未改变" }); }
}
onActivated(() => { if (!uiStores.config.dirty.value) void load(); });
</script>

<template>
  <section class="page settings-page">
    <div class="page-heading"><div><h2>设置</h2></div><div class="heading-actions"><TButton shape="square" variant="outline" theme="default" @click="reload"><IconRefresh :size="18" /></TButton><TButton theme="primary" :disabled="!uiStores.config.dirty.value" @click="validateConfig"><IconShieldCheck :size="17" />验证</TButton></div></div>
    <section class="settings-utilities">
      <div class="settings-utility"><div><strong>界面主题</strong><small>跟随系统或固定明暗外观</small></div><OptionDropdown class="theme-dropdown" compact :model-value="themeMode" :options="themeOptions" @update:model-value="(value) => setThemeMode(value as 'system' | 'light' | 'dark')" /></div>
      <TButton size="small" variant="outline" @click="router.push('/operations')"><IconTool :size="17" />运维</TButton>
    </section>
    <PageState v-if="loading" kind="loading" title="正在加载配置" />
    <PageState v-else-if="error" kind="error" title="设置不可用" :detail="error" action-label="重试" @action="load" />
    <template v-else>
      <OperationBanner v-for="operation in Object.values(operations.byId)" :key="operation.id" :phase="operation.phase" :message="operation.message ?? ''" />
      <div v-if="conflict" class="conflict-panel"><strong>检测到配置冲突</strong><span>重新加载获取最新配置，或保留当前草稿后重新验证。</span><TButton size="small" variant="outline" @click="reload">重新加载</TButton></div>
      <div v-if="validation" class="impact-panel"><strong>验证通过</strong><span>影响级别：{{ validation.apply_impact ?? '由 daemon 决定' }}</span><span>预计中断：{{ validation.estimated_disruption ?? '未知' }}</span><TButton theme="primary" @click="confirmApply = true"><IconDeviceFloppy :size="17" />应用配置</TButton></div>
      <div class="config-editor-toolbar"><div><strong>配置编辑</strong><small>JSON 仍经过 schema、安全审计和事务激活</small></div><SegmentedControl :model-value="editorMode" :options="editorModeOptions" @update:model-value="changeEditorMode" /></div>
      <TCollapse v-if="editorMode === 'form'" :default-value="groups.filter((group) => group.name !== 'advanced').map((group) => group.name)"><TCollapsePanel v-for="group in groups" :key="group.name" :value="group.name" :header="group.name"><div class="schema-grid"><div v-for="field in group.fields" :key="field.id" class="schema-field-wrap"><SchemaField :field="definition(field)" @change="(value) => updateValue(field.path, value)" /><StatusLine v-if="field.experimental" status="degraded" label="实验功能" /><small>{{ field.applyImpact }} · {{ field.riskLevel }}</small></div></div></TCollapsePanel></TCollapse>
      <div v-else class="raw-config-editor"><TTextarea :value="rawConfig" :autosize="{ minRows: 18, maxRows: 32 }" placeholder="输入完整配置 JSON" @change="editRawConfig" /><span v-if="rawError" class="form-error">{{ rawError }}</span></div>
    </template>
    <ConfirmDialog v-model:visible="confirmApply" title="应用配置" :description="`此操作影响级别为 ${String(validation?.apply_impact ?? 'unknown')}，由 daemon 事务化发布。`" confirm-label="应用" @confirm="applyConfig" />
  </section>
</template>
