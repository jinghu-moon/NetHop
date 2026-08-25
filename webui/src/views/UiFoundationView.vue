<script setup lang="ts">
import { ref } from "vue";
import { IconActivity, IconApps, IconCheck, IconChevronRight, IconCloudUpload, IconDots, IconHome, IconMessage, IconPalette, IconRefresh, IconSearch, IconSettings, IconTag, IconTrash, IconX } from "@tabler/icons-vue";
import Button from "@/components/ui/primitives/Button.vue";
import IconButton from "@/components/ui/primitives/IconButton.vue";
import Tag from "@/components/ui/primitives/Tag.vue";
import Divider from "@/components/ui/primitives/Divider.vue";
import Input from "@/components/ui/primitives/Input.vue";
import Textarea from "@/components/ui/primitives/Textarea.vue";
import InputNumber from "@/components/ui/primitives/InputNumber.vue";
import Switch from "@/components/ui/primitives/Switch.vue";
import Checkbox from "@/components/ui/primitives/Checkbox.vue";
import Radio from "@/components/ui/primitives/Radio.vue";
import Field from "@/components/ui/form/Field.vue";
import RadioGroup from "@/components/ui/form/RadioGroup.vue";
import Dialog from "@/components/ui/overlay/Dialog.vue";
import Dropdown from "@/components/ui/overlay/Dropdown.vue";
import DropdownSubmenu from "@/components/ui/overlay/DropdownSubmenu.vue";
import SplitButton from "@/components/ui/composite/SplitButton.vue";
import PageState, { type PageStateModel } from "@/components/ui/feedback/PageState.vue";
import MenuList from "@/components/ui/menu/MenuList.vue";
import MenuSection from "@/components/ui/menu/MenuSection.vue";
import MenuItem from "@/components/ui/menu/MenuItem.vue";
import ToastHost from "@/components/ui/feedback/ToastHost.vue";
import type { ToastItem, ToastPlacement } from "@/components/ui/feedback/toast-types";
import IconTextButton from "@/components/ui/composite/IconTextButton.vue";
import CompoundButton from "@/components/ui/composite/CompoundButton.vue";
import RemovableTag from "@/components/ui/composite/RemovableTag.vue";
import SelectableTag from "@/components/ui/composite/SelectableTag.vue";
import TagGroup from "@/components/ui/composite/TagGroup.vue";
import PasswordInput from "@/components/ui/composite/PasswordInput.vue";
import List from "@/components/ui/layout/List.vue";
import ListItem from "@/components/ui/layout/ListItem.vue";
import ListItemButton from "@/components/ui/layout/ListItemButton.vue";
import ListSection from "@/components/ui/layout/ListSection.vue";
import BottomBar, { type BottomBarItem } from "@/components/ui/navigation/BottomBar.vue";
import NavBar from "@/components/ui/navigation/NavBar.vue";
import Select from "@/components/ui/primitives/Select.vue";
import Segmented from "@/components/ui/navigation/Segmented.vue";

const submitted = ref(0);
const removableTags = ref(["香港节点", "日本节点", "美国节点"]);
const selectedHongKong = ref(true);
const selectedJapan = ref(false);
const selectedRegions = ref<string[]>(["香港"]);
const inputSamples = ref({ basic: "", outline: "", disabled: "", readonly: "只读内容", search: "", invalid: "", limited: "", url: "", phone: "" });
const textareaSamples = ref({ basic: "", autosize: "第一行\n第二行", limited: "", readonly: "只读文本", invalid: "" });
const numberSample = ref<number | undefined>(2);
const passwordSample = ref("");
const switchSamples = ref({ enabled: true, loading: false });
const checkboxSamples = ref({ accepted: true, notifications: false });
const radioSample = ref("auto");
const dialogSamples = ref({ basic: false, guarded: false, danger: false });
const dialogAllowClose = ref(false);
const dialogGuardState = ref("等待关闭请求");
const dialogGuardAttempts = ref(0);
const pageStateActionCount = ref(0);
const pageStateModels = {
  loading: { type: "loading", title: "正在加载节点" },
  empty: { type: "empty", title: "暂无节点", detail: "添加订阅或手动创建节点后会显示在这里。" },
  error: { type: "error", title: "节点加载失败", detail: "连接服务失败，请稍后重试。" },
  warning: { type: "warning", title: "节点数据已过期", detail: "当前显示的是最近一次成功加载的结果。" },
} as const;
const pageStateErrorModel = ref<PageStateModel>(pageStateModels.error);
const menuSample = ref("refresh");
const menuActionCount = ref(0);
const dropdownOpen = ref(false);
const cascadeOpen = ref(false);
const hoverDropdownOpen = ref(false);
const contextDropdownOpen = ref(false);
const endDropdownOpen = ref(false);
const dropdownSelection = ref("未选择");
const toastPlacement = ref<ToastPlacement>("bottom-center");
const toastPlacementOptions = [
  { value: "top-center", label: "顶部居中" },
  { value: "top-start", label: "顶部左侧" },
  { value: "top-end", label: "顶部右侧" },
  { value: "bottom-center", label: "底部居中" },
  { value: "bottom-start", label: "底部左侧" },
  { value: "bottom-end", label: "底部右侧" },
] as const;
const toastItems = ref<ToastItem[]>([]);
const listActionCount = ref(0);
const listSwitchEnabled = ref(true);
const listTheme = ref("系统");
const bottomBarValue = ref("overview");
const navBarSearch = ref("");
const bottomBarItems: readonly BottomBarItem[] = [
  { value: "overview", label: "概览", icon: IconActivity },
  { value: "applications", label: "应用", icon: IconApps, badge: "dot" },
  { value: "messages", label: "消息", icon: IconMessage, badge: 3 },
  { value: "settings", label: "设置", icon: IconSettings },
];
const bottomBarTextItems: readonly BottomBarItem[] = [
  { value: "overview", label: "概览" },
  { value: "applications", label: "应用" },
  { value: "settings", label: "设置" },
];
const bottomBarIconItems: readonly BottomBarItem[] = [
  { value: "overview", label: "概览", icon: IconActivity },
  { value: "applications", label: "应用", icon: IconApps },
  { value: "messages", label: "消息", icon: IconMessage },
  { value: "settings", label: "设置", icon: IconSettings },
];

function reload(): void {
  window.location.reload();
}

function submit(): void {
  submitted.value += 1;
}

function removeTag(label: string): void {
  removableTags.value = removableTags.value.filter((tag) => tag !== label);
}

async function guardDialogDismiss(): Promise<boolean> {
  dialogGuardAttempts.value += 1;
  dialogGuardState.value = "异步校验中";
  await Promise.resolve();
  const allowed = dialogAllowClose.value;
  dialogGuardState.value = allowed ? "校验通过" : "仍有未保存内容，关闭已阻止";
  return allowed;
}

function reloadPageState(): void {
  pageStateActionCount.value += 1;
  pageStateErrorModel.value = { type: "loading", title: "正在加载节点", detail: "正在重新获取最新节点数据。" };
}

function selectMenu(value: string | undefined): void {
  if (value) menuSample.value = value;
  menuActionCount.value += 1;
}

function selectDropdown(value: string, close: () => void): void {
  dropdownSelection.value = value;
  close();
}

function updateToast(item: ToastItem): void {
  const index = toastItems.value.findIndex((current) => current.id === item.id);
  toastItems.value = index < 0 ? [...toastItems.value, item] : toastItems.value.map((current, currentIndex) => currentIndex === index ? item : current);
}

function showPendingToast(): void {
  updateToast({ id: "foundation-operation", tone: "loading", message: "正在同步节点", detail: "完成后会自动更新当前列表。", duration: 6000, showProgress: true, action: { id: "undo", label: "撤回" } });
}

function showSuccessToast(): void {
  updateToast({ id: "foundation-operation", tone: "success", message: "节点同步完成", detail: "同一 operation ID 原位更新。", duration: 3000, showProgress: true });
}

function showAllToastTypes(): void {
  toastItems.value = [
    { id: "foundation-info", tone: "info", message: "信息提示", detail: "普通信息状态。", duration: 30000, showProgress: true },
    { id: "foundation-loading", tone: "loading", message: "正在同步节点", detail: "loading 状态可原位更新。", duration: null, persistent: true },
    { id: "foundation-success", tone: "success", message: "节点同步完成", detail: "成功结果保留短暂时间。", duration: 30000, showProgress: true },
    { id: "foundation-warning", tone: "warning", message: "节点数据已过期", detail: "可以继续浏览旧数据。", duration: 30000, showProgress: true },
    { id: "foundation-error", tone: "error", message: "节点同步失败", detail: "请检查连接后重试。", persistent: true, closable: true, closeLabel: "关闭错误提示", action: { id: "retry", label: "重试" } },
  ];
}

function handleToastAction(action: { id: string }, toastId: string): void {
  if (action.id !== "undo") return;
  updateToast({ id: toastId, tone: "success", message: "已撤回节点同步", detail: "操作按钮由页面处理。", duration: 2600, showProgress: true });
}

function dismissToast(id: string): void { toastItems.value = toastItems.value.filter((item) => item.id !== id); }

</script>

<template>
  <main class="ui-foundation-page" data-ui-foundation="true">
    <header class="ui-foundation-header">
      <div>
        <p class="ui-foundation-eyebrow">DEVELOPMENT ONLY</p>
        <h1>UI 基础组件实验室</h1>
        <p>先验证组件契约、状态和移动端布局，再迁移业务页面。</p>
      </div>
      <Button variant="outline" size="s" aria-label="重新加载" @click="reload">
        <IconRefresh :size="18" aria-hidden="true" />
      </Button>
    </header>

    <section class="ui-foundation-section" aria-labelledby="button-title">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">PRIMITIVE / BUTTON</p>
          <h2 id="button-title">Button</h2>
        </div>
        <span>原生 button · 窄 API</span>
      </div>

      <div class="ui-foundation-grid" data-button-gallery>
        <Button variant="default">默认</Button>
        <Button variant="primary">主要</Button>
        <Button variant="danger">危险</Button>
        <Button variant="outline">描边</Button>
        <Button variant="text">文本</Button>
        <Button size="s">小尺寸</Button>
        <Button size="l">大尺寸</Button>
        <Button loading>加载中</Button>
        <Button disabled>已禁用</Button>
      </div>

      <div class="ui-foundation-subsection" data-button-content-gallery data-testid="button-content-gallery">
        <div class="ui-foundation-subsection-heading">
          <strong>内容形态</strong>
          <span>文本 Button、IconButton</span>
        </div>
        <div class="ui-foundation-grid">
          <Button variant="primary">应用配置</Button>
          <Button variant="outline">导入订阅</Button>
          <Button variant="primary" shape="pill">胶囊按钮</Button>
          <IconButton variant="primary" shape="circle" aria-label="圆形图标">
            <IconSettings :size="18" aria-hidden="true" />
          </IconButton>
          <IconTextButton variant="primary" shape="pill">
            <template #icon><IconCloudUpload /></template>
            图标与文本
          </IconTextButton>
          <IconTextButton orientation="vertical" variant="outline">
            <template #icon><IconSettings /></template>
            垂直排列
          </IconTextButton>
          <IconButton variant="text" aria-label="删除"><IconTrash :size="18" aria-hidden="true" /></IconButton>
          <IconButton variant="outline" loading aria-label="刷新中"><IconRefresh :size="18" aria-hidden="true" /></IconButton>
        </div>
      </div>

      <div class="ui-foundation-subsection" data-compound-button-gallery data-testid="compound-button-gallery">
        <div class="ui-foundation-subsection-heading">
          <strong>复合按钮</strong>
          <span>IconButton + Button · 独立交互</span>
        </div>
        <div class="ui-foundation-grid">
          <CompoundButton icon-aria-label="设置快捷操作" icon-variant="danger" text-variant="primary">
            <template #icon><IconSettings :size="18" aria-hidden="true" /></template>
            进入设置
          </CompoundButton>
          <CompoundButton icon-aria-label="导入快捷操作" icon-variant="outline" text-variant="outline">
            <template #icon><IconCloudUpload :size="18" aria-hidden="true" /></template>
            导入订阅
          </CompoundButton>
          <CompoundButton orientation="vertical" icon-aria-label="创建快捷操作" icon-variant="outline" text-variant="primary">
            <template #icon><IconSettings :size="18" aria-hidden="true" /></template>
            创建项目
          </CompoundButton>
        </div>
      </div>

      <form class="ui-foundation-form" @submit.prevent="submit">
        <span>原生提交次数：{{ submitted }}</span>
        <Button variant="primary" native-type="submit">提交表单</Button>
      </form>
    </section>

    <section class="ui-foundation-section" aria-labelledby="input-title" data-input-gallery data-testid="input-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">PRIMITIVE / INPUT</p>
          <h2 id="input-title">Input 输入框</h2>
        </div>
        <span>原生 input · v-model · slots</span>
      </div>
      <div class="ui-foundation-subsection ui-foundation-input-demo">
        <div class="ui-foundation-subsection-heading"><strong>基础输入框</strong><span>plain / outline</span></div>
        <Input v-model="inputSamples.basic" placeholder="请输入文字" data-testid="input-basic" />
        <Input v-model="inputSamples.outline" variant="outline" placeholder="请输入文字" data-testid="input-outline" />
        <Input v-model="inputSamples.disabled" disabled placeholder="已禁用" data-testid="input-disabled" />
        <Input v-model="inputSamples.readonly" readonly data-testid="input-readonly" />
      </div>
      <div class="ui-foundation-subsection ui-foundation-input-demo">
        <div class="ui-foundation-subsection-heading"><strong>插槽与状态</strong><span>prefix / suffix / invalid</span></div>
        <Input v-model="inputSamples.search" type="search" placeholder="搜索应用">
          <template #prefix><IconTag /></template>
          <template #suffix><span class="ui-foundation-input-hint">⌕</span></template>
        </Input>
        <Input v-model="inputSamples.invalid" invalid placeholder="请输入正确内容" data-testid="input-invalid">
          <template #suffix><span class="ui-foundation-input-error">!</span></template>
        </Input>
      </div>
      <div class="ui-foundation-subsection ui-foundation-input-demo">
        <div class="ui-foundation-subsection-heading"><strong>长度和类型</strong><span>maxlength / url / tel</span></div>
        <Input v-model="inputSamples.limited" :maxlength="10" placeholder="最多输入 10 个字符" data-testid="input-limited" />
        <small class="ui-foundation-input-count">{{ inputSamples.limited.length }}/10</small>
        <Input v-model="inputSamples.url" type="url" placeholder="请输入 HTTPS 链接" />
        <Input v-model="inputSamples.phone" type="tel" inputmode="tel" placeholder="请输入手机号码" />
      </div>
      <div class="ui-foundation-subsection ui-foundation-input-demo">
        <div class="ui-foundation-subsection-heading"><strong>Field 组合</strong><span>label / description / error</span></div>
        <Field label="订阅名称" description="用于识别这条订阅" required>
          <Input v-model="inputSamples.basic" variant="outline" placeholder="例如：主订阅" />
        </Field>
        <Field label="订阅链接" error="请输入有效的 HTTPS 链接" id="foundation-subscription-url">
          <Input v-model="inputSamples.url" type="url" variant="outline" placeholder="https://example.com/subscription" data-testid="field-input" />
        </Field>
      </div>
      <div class="ui-foundation-subsection ui-foundation-input-demo" data-textarea-gallery data-testid="textarea-gallery">
        <div class="ui-foundation-subsection-heading"><strong>Textarea</strong><span>rows / autosize / resize</span></div>
        <Textarea v-model="textareaSamples.basic" variant="outline" :rows="3" placeholder="请输入多行内容" data-testid="textarea-basic" />
        <Textarea v-model="textareaSamples.autosize" variant="outline" :min-rows="2" :max-rows="5" placeholder="自动扩展高度" data-testid="textarea-autosize" />
        <Textarea v-model="textareaSamples.limited" variant="outline" :maxlength="80" resize="none" placeholder="最多输入 80 个字符" data-testid="textarea-limited" />
        <small class="ui-foundation-input-count">{{ textareaSamples.limited.length }}/80</small>
        <Textarea v-model="textareaSamples.readonly" variant="outline" readonly data-testid="textarea-readonly" />
        <Field label="订阅说明" description="支持多行文本" error="内容不能为空" required>
          <Textarea v-model="textareaSamples.invalid" variant="outline" :min-rows="2" :max-rows="4" data-testid="field-textarea" />
        </Field>
      </div>
      <div class="ui-foundation-subsection ui-foundation-input-demo" data-input-number-gallery data-testid="input-number-gallery">
        <div class="ui-foundation-subsection-heading"><strong>PasswordInput / InputNumber</strong><span>组合控件 · 边界约束</span></div>
        <PasswordInput v-model="passwordSample" variant="outline" autocomplete="current-password" placeholder="请输入密码" data-testid="password-input" />
        <InputNumber v-model="numberSample" variant="outline" :min="0" :max="10" :step="0.5" :precision="1" aria-label="节点权重" data-testid="input-number" />
        <output class="ui-foundation-input-count">当前值：{{ numberSample ?? "空" }}</output>
      </div>
    </section>

    <section class="ui-foundation-section" aria-labelledby="selection-title" data-selection-gallery data-testid="selection-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">PRIMITIVE / SELECTION</p>
          <h2 id="selection-title">Switch / Checkbox / Radio</h2>
        </div>
        <span>原生语义 · 布尔值与选中值</span>
      </div>

      <div class="ui-foundation-subsection ui-foundation-control-demo">
        <div class="ui-foundation-subsection-heading"><strong>Switch</strong><span>off / on / loading / disabled</span></div>
        <div class="ui-foundation-control-grid">
          <div class="ui-foundation-control-row">
            <span>允许代理接管</span>
            <Switch v-model="switchSamples.enabled" aria-label="允许代理接管" data-testid="foundation-switch" />
          </div>
          <div class="ui-foundation-control-row">
            <span>保存中</span>
            <Switch v-model="switchSamples.loading" loading aria-label="保存中" data-testid="foundation-switch-loading" />
          </div>
          <div class="ui-foundation-control-row">
            <span>不可用</span>
            <Switch disabled aria-label="不可用" data-testid="foundation-switch-disabled" />
          </div>
          <div class="ui-foundation-control-row">
            <span>带文字开关</span>
            <Switch v-model="switchSamples.enabled" size="l" on-text="开" off-text="关" aria-label="带文字开关" data-testid="foundation-switch-text" />
          </div>
          <div class="ui-foundation-control-row">
            <span>带图标开关</span>
            <Switch v-model="switchSamples.enabled" aria-label="带图标开关" data-testid="foundation-switch-icon">
              <template #on-icon><IconCheck aria-hidden="true" /></template>
              <template #off-icon><IconX aria-hidden="true" /></template>
            </Switch>
          </div>
        </div>
      </div>

      <div class="ui-foundation-subsection ui-foundation-control-demo">
        <div class="ui-foundation-subsection-heading"><strong>Checkbox</strong><span>boolean · checked / disabled</span></div>
        <div class="ui-foundation-control-grid">
          <Checkbox v-model="checkboxSamples.accepted" aria-label="接受服务条款" data-testid="foundation-checkbox-accepted">接受服务条款</Checkbox>
          <Checkbox v-model="checkboxSamples.notifications" aria-label="接收通知" data-testid="foundation-checkbox-notifications">接收通知</Checkbox>
          <Checkbox v-model="checkboxSamples.notifications" shape="circle" aria-label="圆形选择" data-testid="foundation-checkbox-circle">圆形选择</Checkbox>
          <Checkbox disabled aria-label="暂不可用" data-testid="foundation-checkbox-disabled">暂不可用</Checkbox>
        </div>
      </div>

      <div class="ui-foundation-subsection ui-foundation-control-demo">
        <div class="ui-foundation-subsection-heading"><strong>Radio</strong><span>native radio · selected value</span></div>
        <div class="ui-foundation-control-grid">
          <RadioGroup v-model="radioSample" name="foundation-route-mode" aria-label="路由模式" data-testid="foundation-radio-group">
            <Radio value="auto" aria-label="自动选择" data-testid="foundation-radio-auto">自动选择</Radio>
            <Radio value="manual" aria-label="手动选择" data-testid="foundation-radio-manual">手动选择</Radio>
            <Radio value="disabled" aria-label="暂不可用" disabled data-testid="foundation-radio-disabled">暂不可用</Radio>
          </RadioGroup>
          <output class="ui-foundation-input-count" data-testid="foundation-radio-value">当前：{{ radioSample }}</output>
        </div>
      </div>
    </section>

    <section class="ui-foundation-section" aria-labelledby="tag-title" data-tag-gallery data-testid="tag-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">PRIMITIVE / TAG</p>
          <h2 id="tag-title">Tag</h2>
        </div>
        <span>短文本状态 · tone + size</span>
      </div>
      <div class="ui-foundation-subsection ui-foundation-tag-group">
        <div class="ui-foundation-subsection-heading">
          <strong>状态色</strong>
          <span>不可交互</span>
        </div>
        <div class="ui-foundation-grid">
          <Tag>普通</Tag>
          <Tag tone="info">信息</Tag>
          <Tag tone="success">成功</Tag>
          <Tag tone="warning">警告</Tag>
          <Tag tone="danger">危险</Tag>
        </div>
      </div>
      <div class="ui-foundation-subsection ui-foundation-tag-group">
        <div class="ui-foundation-subsection-heading">
          <strong>尺寸</strong>
          <span>S / M</span>
        </div>
        <div class="ui-foundation-grid">
          <Tag size="s" tone="info">小标签</Tag>
          <Tag size="m" tone="info">中标签</Tag>
          <Tag size="m" tone="warning">订阅待更新</Tag>
        </div>
      </div>
      <div class="ui-foundation-subsection ui-foundation-tag-group">
        <div class="ui-foundation-subsection-heading">
          <strong>视觉变体与形状</strong>
          <span>soft / solid / outline</span>
        </div>
        <div class="ui-foundation-grid">
          <Tag tone="info" variant="soft">浅色</Tag>
          <Tag tone="info" variant="solid">实心</Tag>
          <Tag tone="info" variant="outline">描边</Tag>
          <Tag tone="success" shape="pill">胶囊</Tag>
          <Tag tone="warning" shape="pill" variant="outline">待更新</Tag>
        </div>
      </div>
      <div class="ui-foundation-subsection ui-foundation-tag-group">
        <div class="ui-foundation-subsection-heading">
          <strong>图标与长文本</strong>
          <span>slot + ellipsis</span>
        </div>
        <div class="ui-foundation-grid">
          <Tag tone="info" variant="outline"><template #icon><IconTag /></template>带图标</Tag>
          <Tag tone="success" variant="soft" style="max-width: 180px" data-testid="tag-ellipsis">这是一个超过可用宽度后会省略的长标签文本</Tag>
        </div>
      </div>
      <div class="ui-foundation-subsection ui-foundation-tag-group" data-tag-composite-gallery data-testid="tag-composite-gallery">
        <div class="ui-foundation-subsection-heading">
          <strong>交互组合</strong>
          <span>Tag 保持纯展示</span>
        </div>
        <div class="ui-foundation-subsection-heading">
          <span>可关闭：Tag + IconButton</span>
          <span>{{ removableTags.length }} 个</span>
        </div>
        <div class="ui-foundation-grid">
          <RemovableTag
            v-for="tag in removableTags"
            :key="tag"
            tone="info"
            shape="pill"
            size="m"
            :remove-label="`移除${tag}`"
            :data-testid="`removable-${tag}`"
            @remove="removeTag(tag)"
          >
            {{ tag }}
          </RemovableTag>
          <span v-if="removableTags.length === 0" class="ui-foundation-inline-note">已全部移除</span>
        </div>
        <div class="ui-foundation-subsection-heading ui-foundation-subsection-heading--nested">
          <span>可选择：真实 button + aria-pressed</span>
          <span>已选 {{ Number(selectedHongKong) + Number(selectedJapan) }} 个</span>
        </div>
        <div class="ui-foundation-grid">
          <SelectableTag v-model:selected="selectedHongKong" tone="info" data-testid="selectable-hong-kong">香港</SelectableTag>
          <SelectableTag v-model:selected="selectedJapan" tone="info" data-testid="selectable-japan">日本</SelectableTag>
          <SelectableTag tone="success" disabled data-testid="selectable-disabled">暂不可用</SelectableTag>
        </div>
        <div class="ui-foundation-subsection-heading ui-foundation-subsection-heading--nested">
          <span>多选组合：TagGroup 统一管理值</span>
          <span>已选 {{ selectedRegions.length }} 个</span>
        </div>
        <TagGroup v-model="selectedRegions" aria-label="地区筛选" data-testid="tag-group">
          <SelectableTag value="香港" tone="info">香港</SelectableTag>
          <SelectableTag value="日本" tone="info">日本</SelectableTag>
          <SelectableTag value="美国" tone="info">美国</SelectableTag>
        </TagGroup>
      </div>
    </section>

    <section class="ui-foundation-section" aria-labelledby="divider-title" data-divider-gallery data-testid="divider-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">PRIMITIVE / DIVIDER</p>
          <h2 id="divider-title">Divider 分割线</h2>
        </div>
        <span>分组 · 组织 · 细化结构</span>
      </div>
      <div class="ui-foundation-subsection ui-foundation-divider-demo">
        <div class="ui-foundation-subsection-heading"><strong>水平分割线</strong><span>solid / inset</span></div>
        <Divider />
        <Divider inset="s" />
        <Divider inset="m" />
      </div>
      <div class="ui-foundation-subsection ui-foundation-divider-demo">
        <div class="ui-foundation-subsection-heading"><strong>带文字水平分割线</strong><span>start / center / end</span></div>
        <Divider label="文字信息" align="start" />
        <Divider label="文字信息" align="center" />
        <Divider label="文字信息" align="end" />
      </div>
      <div class="ui-foundation-subsection ui-foundation-divider-demo">
        <div class="ui-foundation-subsection-heading"><strong>虚线样式</strong><span>solid / dashed</span></div>
        <Divider variant="dashed" />
        <Divider variant="dashed" label="文字信息" align="center" />
      </div>
      <div class="ui-foundation-subsection ui-foundation-divider-demo">
        <div class="ui-foundation-subsection-heading"><strong>垂直分割线</strong><span>用于横向内容分组</span></div>
        <div class="ui-foundation-divider-row">
          <span>文字信息</span><Divider orientation="vertical" /><span>文字信息</span><Divider orientation="vertical" variant="dashed" /><span>文字信息</span>
        </div>
      </div>
    </section>

    <section class="ui-foundation-section" aria-labelledby="list-title" data-list-gallery data-testid="list-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">LAYOUT / LIST</p>
          <h2 id="list-title">List 列表</h2>
        </div>
        <span>语义容器 · 业务内容 slot</span>
      </div>

      <div class="ui-foundation-subsection ui-foundation-settings-list-demo" data-testid="settings-list-composition">
        <div class="ui-foundation-subsection-heading"><strong>设置列表组合</strong><span>leading / content / trailing</span></div>
        <ListSection title="界面">
          <List divided inset="m" aria-label="界面设置" class="ui-foundation-list-surface">
            <ListItem>
              <template #leading><span class="ui-foundation-list-icon"><IconPalette aria-hidden="true" /></span></template>
              <strong>主题</strong><small>跟随系统或手动选择</small>
              <template #trailing>
                <Segmented class="ui-foundation-list-segmented" :model-value="listTheme" :options="[{ value: '系统', label: '系统' }, { value: '浅色', label: '浅色' }, { value: '深色', label: '深色' }]" aria-label="主题" @change="listTheme = String($event.value)" />
              </template>
            </ListItem>
            <ListItem>
              <template #leading><span class="ui-foundation-list-icon"><IconActivity aria-hidden="true" /></span></template>
              <strong>运维</strong><small>日志、备份、诊断和版本检查</small>
              <template #trailing><IconButton size="s" variant="text" aria-label="打开运维操作"><IconSettings aria-hidden="true" /></IconButton></template>
            </ListItem>
          </List>
        </ListSection>
        <ListSection title="配置" description="每个入口都由业务层决定实际路由">
          <List divided inset="m" aria-label="配置设置" class="ui-foundation-list-surface">
            <ListItemButton>
              <template #leading><span class="ui-foundation-list-icon"><IconSettings aria-hidden="true" /></span></template>
              <strong>网络接管</strong><small>协议、DNS、IPv6 和 TUN 栈</small>
              <template #trailing><IconChevronRight aria-hidden="true" /></template>
            </ListItemButton>
            <ListItemButton>
              <template #leading><span class="ui-foundation-list-icon"><IconTag aria-hidden="true" /></span></template>
              <strong>路由策略</strong><small>私网、中国大陆和 QUIC 处理策略</small>
              <template #trailing><IconChevronRight aria-hidden="true" /></template>
            </ListItemButton>
          </List>
        </ListSection>
      </div>

      <div class="ui-foundation-subsection ui-foundation-list-demo">
        <div class="ui-foundation-subsection-heading"><strong>基础列表</strong><span>spacing / leading / trailing</span></div>
        <List aria-label="基础列表示例" data-testid="list-basic">
          <ListItem>
            <strong>允许代理接管</strong><small>右侧是独立开关，点击只改变开关状态</small>
            <template #trailing><Switch v-model="listSwitchEnabled" aria-label="允许代理接管" /></template>
          </ListItem>
          <ListItem align="start">
            <strong>允许长文本自然换行</strong><small>列表容器不固定行高，较长的辅助说明会在可用空间内换行，不会挤压尾部操作。</small>
            <template #trailing><IconButton size="s" variant="text" aria-label="基础行更多操作"><IconSettings aria-hidden="true" /></IconButton></template>
          </ListItem>
        </List>
      </div>

      <div class="ui-foundation-subsection ui-foundation-list-demo">
        <div class="ui-foundation-subsection-heading"><strong>内缩分隔线</strong><span>divided + inset</span></div>
        <List divided inset="m" aria-label="分隔列表示例" data-testid="list-divided">
          <ListItem><strong>第一项</strong><small>分隔线距离两侧 16px</small></ListItem>
          <ListItem><strong>普通结构项</strong><small>普通 ListItem 不表达选择状态</small></ListItem>
          <ListItem disabled><strong>不可用项</strong><small>结构行不自行处理点击</small></ListItem>
        </List>
      </div>

      <div class="ui-foundation-subsection ui-foundation-list-demo">
        <div class="ui-foundation-subsection-heading"><strong>可点击列表行</strong><span>原生 button · 独立交互</span></div>
        <List divided inset="s" aria-label="操作列表示例" data-testid="list-actions">
          <ListItemButton @click="listActionCount += 1">
            <strong>进入网络接管设置</strong><small>点击整行进入二级界面 · 已点击 {{ listActionCount }} 次</small>
            <template #trailing><IconChevronRight aria-hidden="true" /></template>
          </ListItemButton>
          <ListItemButton disabled>
            <strong>暂不可用</strong><small>禁用状态阻止交互</small>
          </ListItemButton>
        </List>
      </div>

      <div class="ui-foundation-subsection ui-foundation-list-demo ui-foundation-list-sections">
        <div class="ui-foundation-subsection-heading"><strong>分组与有序列表</strong><span>ListSection + ol</span></div>
        <ListSection title="启动顺序" description="分组只负责标题与说明">
          <List as="ol" divided aria-label="启动顺序" data-testid="list-ordered">
            <ListItem><strong>加载配置</strong></ListItem>
            <ListItem><strong>启动代理核心</strong></ListItem>
            <ListItem><strong>发布运行状态</strong></ListItem>
          </List>
        </ListSection>
      </div>
    </section>

    <section class="ui-foundation-section" aria-labelledby="dialog-title" data-dialog-gallery data-testid="dialog-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">OVERLAY / DIALOG</p>
          <h2 id="dialog-title">Dialog 对话框</h2>
        </div>
        <span>焦点 · 关闭原因 · 异步守卫</span>
      </div>
      <div class="ui-foundation-subsection ui-foundation-dialog-demo">
        <div class="ui-foundation-subsection-heading"><strong>基础 Dialog</strong><span>Escape / Back / focus restore</span></div>
        <div class="ui-foundation-grid">
          <Button variant="outline" data-testid="open-basic-dialog" @click="dialogSamples.basic = true">打开基础对话框</Button>
          <span class="ui-foundation-inline-note">关闭后焦点恢复到触发按钮</span>
        </div>
        <Dialog v-model="dialogSamples.basic" title="基础对话框" aria-describedby="foundation-basic-dialog-description" data-testid="basic-dialog">
          <p id="foundation-basic-dialog-description">这是一个只负责容器、焦点和关闭行为的基础 Dialog。</p>
          <template #actions="{ requestClose, dismissing }">
            <Button variant="outline" :disabled="dismissing" @click="requestClose('action')">关闭</Button>
          </template>
        </Dialog>
      </div>

      <div class="ui-foundation-subsection ui-foundation-dialog-demo">
        <div class="ui-foundation-subsection-heading"><strong>异步关闭守卫</strong><span>beforeDismiss · reject close</span></div>
        <div class="ui-foundation-grid">
          <Button variant="outline" data-testid="open-guarded-dialog" @click="dialogSamples.guarded = true">打开未保存编辑</Button>
          <span class="ui-foundation-inline-note">{{ dialogGuardState }} · {{ dialogGuardAttempts }} 次请求</span>
        </div>
        <Dialog v-model="dialogSamples.guarded" title="未保存的编辑" :before-dismiss="guardDialogDismiss" data-testid="guarded-dialog">
          <p>关闭前需要完成异步校验。取消勾选时，Escape、Back 和按钮关闭都会被阻止。</p>
          <Checkbox v-model="dialogAllowClose" data-testid="dialog-allow-close">模拟校验通过</Checkbox>
          <template #actions="{ requestClose, dismissing }">
            <Button variant="outline" :disabled="dismissing" @click="requestClose('action')">取消</Button>
            <Button variant="primary" :loading="dismissing" @click="requestClose('action')">保存并关闭</Button>
          </template>
        </Dialog>
      </div>

      <div class="ui-foundation-subsection ui-foundation-dialog-demo">
        <div class="ui-foundation-subsection-heading"><strong>危险操作</strong><span>显式 action · 不自动提交</span></div>
        <div class="ui-foundation-grid">
          <Button variant="danger" data-testid="open-danger-dialog" @click="dialogSamples.danger = true">打开危险操作</Button>
          <span class="ui-foundation-inline-note">默认不允许遮罩误关闭</span>
        </div>
        <Dialog v-model="dialogSamples.danger" title="删除节点？" :initial-focus="'dialog'" show-close-button close-label="关闭删除确认" data-testid="danger-dialog">
          <p>该操作无法撤销。Enter 不会自动触发删除。</p>
          <template #actions="{ requestClose, dismissing }">
            <Button variant="outline" :disabled="dismissing" @click="requestClose('action')">取消</Button>
            <Button variant="danger" :disabled="dismissing" data-dialog-danger @click="requestClose('action')">删除节点</Button>
          </template>
        </Dialog>
      </div>
    </section>

    <section class="ui-foundation-section" aria-labelledby="page-state-title" data-page-state-gallery data-testid="page-state-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">FEEDBACK / PAGE STATE</p>
          <h2 id="page-state-title">PageState 页面状态</h2>
        </div>
        <span>结构化状态 · loading / empty / error / warning</span>
      </div>
      <div class="ui-foundation-subsection ui-foundation-page-state-grid">
        <div class="ui-foundation-page-state-card" data-testid="page-state-loading">
          <strong>Loading</strong>
          <PageState :model="pageStateModels.loading" />
        </div>
        <div class="ui-foundation-page-state-card" data-testid="page-state-empty">
          <strong>Empty</strong>
          <PageState :model="pageStateModels.empty" />
        </div>
        <div class="ui-foundation-page-state-card" data-testid="page-state-error">
          <strong>Error + action</strong>
          <PageState :model="pageStateErrorModel" action-label="重新加载" @action="reloadPageState" />
          <output>已触发 {{ pageStateActionCount }} 次</output>
        </div>
        <div class="ui-foundation-page-state-card" data-testid="page-state-warning">
          <strong>Warning + slot action</strong>
          <PageState :model="pageStateModels.warning">
            <template #action><Button variant="outline" @click="pageStateActionCount += 1">查看详情</Button></template>
          </PageState>
        </div>
      </div>
    </section>

    <section class="ui-foundation-section" aria-labelledby="menu-title" data-menu-gallery data-testid="menu-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">MENU / PARTS</p>
          <h2 id="menu-title">MenuList 菜单部件</h2>
        </div>
        <span>menu / listbox · roving focus · type-ahead</span>
      </div>
      <div class="ui-foundation-subsection ui-foundation-menu-demo">
        <div class="ui-foundation-subsection-heading"><strong>命令菜单</strong><span>Arrow / Home / End / type-ahead</span></div>
        <MenuList aria-label="节点操作" data-testid="foundation-menu" @select="selectMenu">
          <MenuSection title="节点操作">
            <MenuItem value="refresh">刷新节点</MenuItem>
            <MenuItem value="edit" description="修改节点配置">编辑节点</MenuItem>
          </MenuSection>
          <MenuSection title="危险操作" divided>
            <MenuItem divider />
            <MenuItem value="exclude" danger>排除节点</MenuItem>
            <MenuItem value="disabled" disabled>暂不可用</MenuItem>
          </MenuSection>
        </MenuList>
        <output class="ui-foundation-input-count" data-testid="menu-selection-value">当前：{{ menuSample }} · 已触发 {{ menuActionCount }} 次</output>
      </div>
      <div class="ui-foundation-subsection ui-foundation-menu-demo">
        <div class="ui-foundation-subsection-heading"><strong>值选择列表</strong><span>listbox / option</span></div>
        <MenuList v-model="menuSample" semantic="listbox" aria-label="节点排序" data-testid="foundation-listbox">
          <MenuItem value="refresh">最近刷新</MenuItem>
          <MenuItem value="latency">最低延迟</MenuItem>
          <MenuItem value="traffic">流量最多</MenuItem>
        </MenuList>
      </div>
    </section>

    <section class="ui-foundation-section" aria-labelledby="nav-bar-title" data-nav-bar-gallery data-testid="nav-bar-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">NAVIGATION / NAV BAR</p>
          <h2 id="nav-bar-title">NavBar 导航栏</h2>
        </div>
        <span>返回 · 标题 · 页面操作</span>
      </div>
      <div class="ui-foundation-subsection ui-foundation-nav-bar-demo">
        <div class="ui-foundation-subsection-heading"><strong>基础导航栏</strong><span>left-arrow · right actions</span></div>
        <NavBar title="节点详情" left-arrow @left-click="dropdownSelection = 'NavBar 返回'">
          <template #right><IconButton variant="text" aria-label="更多操作"><IconDots :size="20" /></IconButton></template>
        </NavBar>
      </div>
      <div class="ui-foundation-subsection ui-foundation-nav-bar-demo">
        <div class="ui-foundation-subsection-heading"><strong>长标题与副标题</strong><span>title-max-length · title slot</span></div>
        <NavBar title="这是一个非常长的页面标题用于验证截断效果" :title-max-length="12" left-arrow>
          <template #title><span class="ui-foundation-nav-title-stack"><strong>标题文字</strong><small>同步状态正常</small></span></template>
        </NavBar>
      </div>
      <div class="ui-foundation-subsection ui-foundation-nav-bar-demo">
        <div class="ui-foundation-subsection-heading"><strong>带搜索区域</strong><span>Input slot composition</span></div>
        <NavBar left-arrow aria-label="搜索导航栏">
          <template #title><Input v-model="navBarSearch" type="search" size="s" variant="outline" placeholder="搜索节点" aria-label="搜索节点" /></template>
          <template #right><IconButton variant="text" aria-label="刷新搜索"><IconRefresh :size="19" /></IconButton></template>
        </NavBar>
      </div>
      <div class="ui-foundation-subsection ui-foundation-nav-bar-demo">
        <div class="ui-foundation-subsection-heading"><strong>自定义左侧与固定占位</strong><span>safe-area · fixed · placeholder</span></div>
        <div class="ui-foundation-nav-fixed-surface">
          <NavBar title="自定义颜色" fixed placeholder safe-area-inset-top :animation="false" class="ui-foundation-nav-fixed" data-testid="foundation-nav-fixed">
            <template #left><IconButton variant="text" aria-label="首页"><IconHome :size="20" /></IconButton></template>
            <template #right><IconButton variant="text" aria-label="搜索"><IconSearch :size="19" /></IconButton></template>
          </NavBar>
          <div class="ui-foundation-nav-fixed-content">固定导航栏的内容占位区域</div>
        </div>
      </div>
    </section>

    <section class="ui-foundation-section" aria-labelledby="bottom-bar-title" data-bottom-bar-gallery data-testid="bottom-bar-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">NAVIGATION / BOTTOM BAR</p>
          <h2 id="bottom-bar-title">BottomBar 底部栏</h2>
        </div>
        <span>一级导航 · 徽标 · safe-area</span>
      </div>
      <div class="ui-foundation-subsection ui-foundation-bottom-bar-demo">
        <div class="ui-foundation-subsection-heading"><strong>图标 + 文字 + 徽标</strong><span>受控切换 · pill</span></div>
        <BottomBar v-model="bottomBarValue" :items="bottomBarItems" :fixed="false" :safe-area-inset-bottom="false" show-item-divider data-testid="foundation-bottom-bar-standard" />
      </div>
      <div class="ui-foundation-subsection ui-foundation-bottom-bar-demo">
        <div class="ui-foundation-subsection-heading"><strong>纯文本标签栏</strong><span>icon 可选</span></div>
        <BottomBar v-model="bottomBarValue" :items="bottomBarTextItems" :fixed="false" :safe-area-inset-bottom="false" indicator="line" data-testid="foundation-bottom-bar-text" />
      </div>
      <div class="ui-foundation-subsection ui-foundation-bottom-bar-demo">
        <div class="ui-foundation-subsection-heading"><strong>纯图标标签栏</strong><span>show-label=false · pill</span></div>
        <BottomBar v-model="bottomBarValue" :items="bottomBarIconItems" :fixed="false" :safe-area-inset-bottom="false" :show-label="false" indicator="pill" data-testid="foundation-bottom-bar-icons" />
      </div>
      <div class="ui-foundation-subsection ui-foundation-bottom-bar-demo">
        <div class="ui-foundation-subsection-heading"><strong>悬浮胶囊</strong><span>视觉变体 · 非业务操作容器</span></div>
        <div class="ui-foundation-bottom-bar-floating-surface">
          <BottomBar v-model="bottomBarValue" :items="bottomBarIconItems" :fixed="false" :safe-area-inset-bottom="false" :show-label="false" variant="floating" :bordered="false" data-testid="foundation-bottom-bar-floating" />
        </div>
      </div>
    </section>

    <section class="ui-foundation-section" aria-labelledby="dropdown-title" data-dropdown-gallery data-testid="dropdown-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">OVERLAY / DROPDOWN</p>
          <h2 id="dropdown-title">Dropdown 下拉菜单</h2>
        </div>
        <span>定位 · 碰撞修正 · nested panel</span>
      </div>
      <div class="ui-foundation-subsection ui-foundation-dropdown-demo">
        <div class="ui-foundation-grid">
          <Dropdown v-model:open="dropdownOpen" placement="bottom-start" show-arrow data-testid="foundation-dropdown">
            <template #trigger="{ open }">
              <Button variant="outline" :aria-expanded="open">文件操作</Button>
            </template>
            <template #default="{ activePanel, close, pushPanel, popPanel }">
              <List v-if="activePanel === 'root'" class="ui-foundation-dropdown-list" aria-label="文件操作" spacing="none">
                <ListItemButton compact @click="selectDropdown('编辑', close)">编辑</ListItemButton>
                <ListItemButton compact @click="pushPanel('share')">分享</ListItemButton>
                <ListItemButton compact disabled>已锁定</ListItemButton>
                <ListItemButton compact class="ui-foundation-list-danger" @click="selectDropdown('删除', close)">删除</ListItemButton>
              </List>
              <List v-else class="ui-foundation-dropdown-list" aria-label="分享操作" spacing="none">
                <ListItemButton compact @click="popPanel">‹ 返回</ListItemButton>
                <ListItemButton compact @click="selectDropdown('复制链接', close)">复制链接</ListItemButton>
                <ListItemButton compact @click="selectDropdown('发送邮件', close)">发送邮件</ListItemButton>
              </List>
            </template>
          </Dropdown>
          <span class="ui-foundation-inline-note">当前选择：{{ dropdownSelection }}</span>
        </div>
      </div>
      <div class="ui-foundation-subsection ui-foundation-dropdown-demo">
        <div class="ui-foundation-subsection-heading"><strong>Hover 触发</strong><span>桌面增强 · 延迟进入/离开</span></div>
        <Dropdown v-model:open="hoverDropdownOpen" trigger="hover" placement="bottom-start" data-testid="foundation-hover-dropdown">
          <template #trigger="{ open }"><Button variant="outline" :aria-expanded="open">悬停打开</Button></template>
          <template #default="{ close }"><List class="ui-foundation-dropdown-list" aria-label="悬停操作" spacing="none"><ListItemButton compact @click="selectDropdown('悬停操作', close)">悬停操作</ListItemButton></List></template>
        </Dropdown>
      </div>
      <div class="ui-foundation-subsection ui-foundation-dropdown-demo ui-foundation-dropdown-demo--end">
        <div class="ui-foundation-subsection-heading"><strong>右对齐与碰撞翻转</strong><span>bottom-end · 空间不足自动 top-end</span></div>
        <Dropdown v-model:open="endDropdownOpen" placement="bottom-end" show-arrow data-testid="foundation-end-dropdown">
          <template #trigger="{ open }"><Button variant="outline" :aria-expanded="open">右侧菜单</Button></template>
          <template #default="{ close }"><List class="ui-foundation-dropdown-list" aria-label="右侧操作" spacing="none"><ListItemButton compact @click="selectDropdown('右对齐操作', close)">右对齐操作</ListItemButton><ListItemButton compact @click="selectDropdown('更多设置', close)">更多设置</ListItemButton></List></template>
        </Dropdown>
      </div>
      <div class="ui-foundation-subsection ui-foundation-dropdown-demo">
        <div class="ui-foundation-subsection-heading"><strong>Context 触发</strong><span>右键/长按 · cursor placement</span></div>
        <Dropdown v-model:open="contextDropdownOpen" trigger="context" placement="cursor" :show-arrow="false" data-testid="foundation-context-dropdown">
          <template #trigger><div class="ui-foundation-context-target">在此区域右键打开</div></template>
          <template #default="{ close }"><List class="ui-foundation-dropdown-list" aria-label="上下文操作" spacing="none"><ListItemButton compact @click="selectDropdown('上下文操作', close)">上下文操作</ListItemButton><ListItemButton compact class="ui-foundation-list-danger" @click="selectDropdown('删除上下文', close)">删除</ListItemButton></List></template>
        </Dropdown>
      </div>
      <div class="ui-foundation-subsection ui-foundation-dropdown-demo">
        <div class="ui-foundation-subsection-heading"><strong>侧向级联</strong><span>真正子面板 · hover / click</span></div>
        <Dropdown v-model:open="cascadeOpen" placement="bottom-start" data-testid="foundation-cascade-dropdown">
          <template #trigger="{ open }"><Button variant="outline" :aria-expanded="open">新建</Button></template>
          <template #default="{ close }">
            <List class="ui-foundation-dropdown-list" aria-label="新建操作" spacing="none">
              <DropdownSubmenu label="从模板创建">
                <List class="ui-foundation-dropdown-list" aria-label="模板列表" spacing="none">
                  <ListItemButton compact @click="selectDropdown('周报模板', close)">周报模板</ListItemButton>
                  <ListItemButton compact @click="selectDropdown('项目计划', close)">项目计划</ListItemButton>
                </List>
              </DropdownSubmenu>
              <ListItemButton compact @click="selectDropdown('空白文档', close)">空白文档</ListItemButton>
            </List>
          </template>
        </Dropdown>
      </div>
      <div class="ui-foundation-subsection ui-foundation-dropdown-demo">
        <div class="ui-foundation-subsection-heading"><strong>SplitButton</strong><span>主操作 + 更多操作</span></div>
        <SplitButton label="导出 PDF" data-testid="foundation-split-button" @click="selectDropdown('导出 PDF', () => undefined)">
          <template #menu="{ close }">
            <List class="ui-foundation-dropdown-list" aria-label="导出格式" spacing="none"><ListItemButton compact @click="selectDropdown('导出 PNG', close)">导出 PNG</ListItemButton><ListItemButton compact @click="selectDropdown('导出 SVG', close)">导出 SVG</ListItemButton></List>
          </template>
        </SplitButton>
      </div>
    </section>

    <section class="ui-foundation-section" aria-labelledby="toast-title" data-toast-gallery data-testid="toast-gallery">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">FEEDBACK / TOAST</p>
          <h2 id="toast-title">Toast 操作提示</h2>
        </div>
        <span>operation ID · progress · action · placement</span>
      </div>
      <div class="ui-foundation-subsection ui-foundation-toast-demo">
        <div class="ui-foundation-grid">
          <Button variant="outline" data-testid="toast-pending" @click="showPendingToast">显示同步中</Button>
          <Button variant="primary" data-testid="toast-success" @click="showSuccessToast">更新为成功</Button>
          <Button variant="danger" data-testid="toast-all-types" @click="showAllToastTypes">显示全部类型</Button>
          <label class="ui-foundation-toast-placement">位置
            <Select v-model="toastPlacement" :options="toastPlacementOptions" aria-label="Toast 位置" data-testid="toast-placement" />
          </label>
        </div>
        <span class="ui-foundation-inline-note">同一 operation ID 会原位更新，进度条随 duration 倒计时。</span>
      </div>
      <ToastHost :items="toastItems" :placement="toastPlacement" :max-visible="5" @action="handleToastAction" @dismiss="dismissToast" />
    </section>

    <section class="ui-foundation-section ui-foundation-section--next" aria-labelledby="roadmap-title">
      <div class="ui-foundation-section-heading">
        <div>
          <p class="ui-foundation-eyebrow">P1 ROADMAP</p>
          <h2 id="roadmap-title">下一批基础能力</h2>
        </div>
      </div>
      <div class="ui-foundation-roadmap">
        <span>Overlay Runtime</span>
        <span>Input / Textarea</span>
        <span>Switch / Checkbox / Radio</span>
        <span>Toast / Menu parts</span>
      </div>
    </section>
  </main>
</template>

<style scoped>
.ui-foundation-page { min-height: 100dvh; padding: max(24px, env(safe-area-inset-top)) 16px max(32px, env(safe-area-inset-bottom)); color: var(--nh-text); background: var(--nh-bg); }
.ui-foundation-header, .ui-foundation-section { width: min(100%, 760px); margin: 0 auto; }
.ui-foundation-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; margin-bottom: 24px; }
.ui-foundation-header h1, .ui-foundation-section h2 { margin: 0; letter-spacing: 0; }
.ui-foundation-header h1 { font-size: 24px; line-height: 1.2; }
.ui-foundation-header p:not(.ui-foundation-eyebrow) { margin: 7px 0 0; color: var(--nh-muted); font-size: 13px; line-height: 1.45; }
.ui-foundation-eyebrow { margin: 0 0 5px; color: var(--nh-muted); font-size: 10px; font-weight: 700; letter-spacing: .08em; }
.ui-foundation-section { padding: 16px; border: 1px solid var(--nh-border); border-radius: 8px; background: var(--nh-surface); }
.ui-foundation-section + .ui-foundation-section { margin-top: 12px; }
.ui-foundation-section-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; margin-bottom: 16px; }
.ui-foundation-section-heading h2 { font-size: 18px; line-height: 1.25; }
.ui-foundation-section-heading > span { color: var(--nh-muted); font-size: 11px; }
.ui-foundation-grid { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.ui-foundation-subsection { margin-top: 16px; padding-top: 14px; border-top: 1px solid var(--nh-border); }
.ui-foundation-subsection-heading { display: flex; align-items: center; justify-content: space-between; margin-bottom: 10px; color: var(--nh-muted); font-size: 11px; gap: 12px; }
.ui-foundation-subsection-heading strong { color: var(--nh-text); font-size: 12px; }
.ui-foundation-subsection-heading--nested { margin-top: 14px; }
.ui-foundation-inline-note { color: var(--nh-muted); font-size: 12px; }
.ui-foundation-divider-demo { display: grid; gap: 14px; }
.ui-foundation-divider-row { display: inline-flex; min-height: 24px; align-items: center; gap: 12px; color: var(--nh-text); font-size: 13px; }
.ui-foundation-control-demo { display: grid; gap: 10px; }
.ui-foundation-control-grid { display: flex; flex-wrap: wrap; align-items: center; gap: 12px 16px; }
.ui-foundation-control-row { display: inline-flex; min-height: 32px; align-items: center; justify-content: space-between; gap: 12px; color: var(--nh-text); font-size: 13px; }
.ui-foundation-dialog-demo { display: grid; gap: 10px; }
.ui-foundation-page-state-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
.ui-foundation-page-state-card { min-width: 0; overflow: hidden; border: 1px solid var(--nh-border); border-radius: 6px; background: var(--nh-bg); }
.ui-foundation-page-state-card > strong { display: block; padding: 10px 10px 0; color: var(--nh-text); font-size: 12px; }
.ui-foundation-page-state-card .nh-page-state { min-height: 190px; padding: 18px 10px; }
.ui-foundation-page-state-card output { display: block; padding: 0 10px 10px; color: var(--nh-muted); font-size: 11px; text-align: center; }
.ui-foundation-menu-demo { display: grid; gap: 10px; }
.ui-foundation-menu-demo .nh-menu-list { width: min(100%, 360px); border: 1px solid var(--nh-border); border-radius: 6px; background: var(--nh-bg); }
.ui-foundation-bottom-bar-demo { display: grid; gap: 10px; }
.ui-foundation-bottom-bar-demo .nh-bottom-bar { overflow: visible; border-right: 1px solid var(--nh-border); border-left: 1px solid var(--nh-border); border-radius: 6px; }
.ui-foundation-bottom-bar-floating-surface { position: relative; min-height: 90px; overflow: hidden; border: 1px solid var(--nh-border); border-radius: 8px; background: var(--nh-bg); }
.ui-foundation-nav-bar-demo { display: grid; gap: 10px; }
.ui-foundation-nav-bar-demo .nh-nav-bar { border: 1px solid var(--nh-border); border-radius: 6px; }
.ui-foundation-nav-title-stack { display: inline-flex; min-width: 0; flex-direction: column; gap: 1px; }
.ui-foundation-nav-title-stack strong { font-size: 15px; line-height: 18px; }
.ui-foundation-nav-title-stack small { color: var(--nh-muted); font-size: 10px; font-weight: 400; line-height: 14px; }
.ui-foundation-nav-bar-demo .nh-input { width: min(100%, 220px); }
.ui-foundation-nav-fixed-surface { position: relative; min-height: 112px; overflow: hidden; border: 1px solid var(--nh-border); border-radius: 8px; background: var(--nh-bg); }
.ui-foundation-nav-fixed-surface .nh-nav-bar { position: absolute; top: 0; right: 0; left: 0; }
.ui-foundation-nav-fixed-content { display: grid; min-height: 112px; place-items: end center; padding-bottom: 10px; color: var(--nh-muted); font-size: 11px; }
.ui-foundation-dropdown-demo { display: grid; gap: 10px; }
.ui-foundation-dropdown-demo--end { justify-items: end; }
.ui-foundation-dropdown-options { display: grid; min-width: 170px; gap: 2px; }
.ui-foundation-dropdown-options button { display: flex; min-height: 36px; align-items: center; justify-content: space-between; padding: 0 8px; border: 0; border-radius: 5px; color: var(--nh-text); background: transparent; font: inherit; font-size: 12px; text-align: left; }
.ui-foundation-dropdown-options button:hover:not(:disabled) { background: var(--state-hover); }
.ui-foundation-dropdown-options button:disabled { color: var(--text-disabled); opacity: .55; }
.ui-foundation-dropdown-options button.danger { color: var(--error); }
.ui-foundation-list-danger :deep(button) { color: var(--error); }
.ui-foundation-dropdown-options hr { width: calc(100% - 16px); margin: 4px 8px; border: 0; border-top: 1px solid var(--border-divider); }
.ui-foundation-context-target { display: grid; width: min(100%, 280px); min-height: 48px; place-items: center; border: 1px dashed var(--nh-border); border-radius: 6px; color: var(--nh-muted); font-size: 12px; }
.ui-foundation-toast-demo { display: grid; gap: 10px; }
.ui-foundation-toast-placement { display: inline-flex; min-height: 32px; align-items: center; color: var(--nh-muted); font-size: 12px; gap: 6px; }
.ui-foundation-toast-placement select { min-height: 32px; padding: 0 8px; border: 1px solid var(--nh-border); border-radius: 6px; color: var(--nh-text); background: var(--nh-bg); font: inherit; }
.ui-foundation-list-demo { display: grid; gap: 8px; }
.ui-foundation-list-demo .nh-list { padding: 0 10px; border-radius: 6px; background: var(--nh-bg); }
.ui-foundation-settings-list-demo { display: grid; gap: 14px; }
.ui-foundation-list-surface { padding: 0 10px; border: 1px solid var(--nh-border); border-radius: 16px; background: var(--nh-surface); }
.ui-foundation-list-segmented { width: auto; min-width: 150px; }
.ui-foundation-list-icon { display: inline-grid; width: 30px; height: 30px; place-items: center; border-radius: 6px; color: var(--info-on-container); background: var(--info-container); }
.ui-foundation-list-icon svg { width: 17px; height: 17px; }
.ui-foundation-list-demo :deep(.nh-list-item__trailing > svg), .ui-foundation-list-demo :deep(.nh-list-item-button__trailing > svg) { width: 17px; height: 17px; color: var(--text-secondary); }
.ui-foundation-list-sections { gap: 12px; }
.ui-foundation-input-demo { display: grid; gap: 10px; }
.ui-foundation-input-demo .nh-input { width: 100%; }
.ui-foundation-input-hint, .ui-foundation-input-error { display: inline-grid; width: 18px; height: 18px; place-items: center; border-radius: 50%; color: var(--text-inverse); background: var(--text-secondary); font-size: 12px; font-weight: 700; }
.ui-foundation-input-error { background: var(--error); }
.ui-foundation-input-count { color: var(--nh-muted); font-size: 11px; }
.ui-foundation-form { display: flex; align-items: center; justify-content: space-between; margin-top: 18px; padding-top: 14px; border-top: 1px solid var(--nh-border); gap: 12px; color: var(--nh-muted); font-size: 12px; }
.ui-foundation-roadmap { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
.ui-foundation-roadmap span { padding: 10px; border: 1px solid var(--nh-border); border-radius: 6px; color: var(--nh-muted); background: var(--nh-bg); font-size: 12px; }
@media (max-width: 520px) { .ui-foundation-page-state-grid { grid-template-columns: 1fr; } }
@media (max-width: 420px) { .ui-foundation-header { align-items: center; } .ui-foundation-section-heading > span { display: none; } .ui-foundation-form { align-items: flex-start; flex-direction: column; } .ui-foundation-form .nh-button { width: 100%; } }
</style>
