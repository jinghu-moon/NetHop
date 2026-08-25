import { render } from "vitest-browser-vue";
import { describe, expect, it } from "vitest";

import Button from "../../src/components/ui/primitives/Button.vue";
import IconButton from "../../src/components/ui/primitives/IconButton.vue";
import Tag from "../../src/components/ui/primitives/Tag.vue";
import Divider from "../../src/components/ui/primitives/Divider.vue";
import Input from "../../src/components/ui/primitives/Input.vue";
import Textarea from "../../src/components/ui/primitives/Textarea.vue";
import InputNumber from "../../src/components/ui/primitives/InputNumber.vue";
import Switch from "../../src/components/ui/primitives/Switch.vue";
import Checkbox from "../../src/components/ui/primitives/Checkbox.vue";
import Radio from "../../src/components/ui/primitives/Radio.vue";
import Field from "../../src/components/ui/form/Field.vue";
import RadioGroup from "../../src/components/ui/form/RadioGroup.vue";
import PageState from "../../src/components/ui/feedback/PageState.vue";
import MenuList from "../../src/components/ui/menu/MenuList.vue";
import MenuSection from "../../src/components/ui/menu/MenuSection.vue";
import MenuItem from "../../src/components/ui/menu/MenuItem.vue";
import Toast from "../../src/components/ui/feedback/Toast.vue";
import ToastHost from "../../src/components/ui/feedback/ToastHost.vue";
import Dropdown from "../../src/components/ui/overlay/Dropdown.vue";
import DropdownSubmenu from "../../src/components/ui/overlay/DropdownSubmenu.vue";
import SplitButton from "../../src/components/ui/composite/SplitButton.vue";
import IconTextButton from "../../src/components/ui/composite/IconTextButton.vue";
import CompoundButton from "../../src/components/ui/composite/CompoundButton.vue";
import RemovableTag from "../../src/components/ui/composite/RemovableTag.vue";
import SelectableTag from "../../src/components/ui/composite/SelectableTag.vue";
import TagGroup from "../../src/components/ui/composite/TagGroup.vue";
import PasswordInput from "../../src/components/ui/composite/PasswordInput.vue";
import List from "../../src/components/ui/layout/List.vue";
import ListItem from "../../src/components/ui/layout/ListItem.vue";
import ListItemButton from "../../src/components/ui/layout/ListItemButton.vue";
import ListSection from "../../src/components/ui/layout/ListSection.vue";
import BottomBar from "../../src/components/ui/navigation/BottomBar.vue";
import NavBar from "../../src/components/ui/navigation/NavBar.vue";
import UiFoundationView from "../../src/views/UiFoundationView.vue";
import { dispatchBack } from "../../src/shell/useBackDispatcher";
import { IconActivity, IconSettings } from "@tabler/icons-vue";

describe("UI foundation", () => {
  it("renders the component laboratory with a reload command", async () => {
    const screen = render(UiFoundationView);
    await expect.element(screen.getByRole("heading", { name: "UI 基础组件实验室" })).toBeVisible();
    await expect.element(screen.getByTestId("page-state-error").getByRole("button", { name: "重新加载" })).toBeVisible();
    await expect.element(screen.getByText("主要", { exact: true })).toBeVisible();
    await expect.element(screen.getByTestId("button-content-gallery")).toBeVisible();
    await expect.element(screen.getByRole("button", { name: "删除" })).toBeVisible();
    await expect.element(screen.getByTestId("compound-button-gallery")).toBeVisible();
    await expect.element(screen.getByRole("heading", { name: "Tag" })).toBeVisible();
    await expect.element(screen.getByTestId("tag-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("tag-composite-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("divider-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("input-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("textarea-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("input-number-gallery")).toBeVisible();
    await expect.element(screen.getByRole("heading", { name: "Switch / Checkbox / Radio" })).toBeVisible();
    await expect.element(screen.getByTestId("selection-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("list-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("settings-list-composition")).toBeVisible();
    await expect.element(screen.getByTestId("dialog-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("page-state-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("menu-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("toast-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("dropdown-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("bottom-bar-gallery")).toBeVisible();
    await expect.element(screen.getByTestId("nav-bar-gallery")).toBeVisible();
  });

  it("renders NavBar variants with centered title, slots, search, and fixed placeholder", async () => {
    const screen = render(UiFoundationView);
    const gallery = screen.getByTestId("nav-bar-gallery");
    await expect.element(gallery.getByRole("navigation", { name: "页面导航" }).first()).toBeVisible();
    await expect.element(gallery.getByRole("button", { name: "返回" }).first()).toBeVisible();
    await expect.element(gallery.getByRole("searchbox", { name: "搜索节点" })).toBeVisible();
    const fixed = gallery.getByTestId("foundation-nav-fixed");
    await expect.element(fixed).toBeVisible();
    expect(fixed.element().classList.contains("nh-nav-bar--fixed")).toBe(true);
    expect(document.querySelector(".nh-nav-bar__placeholder")).not.toBeNull();
    expect(fixed.element().querySelector(".nh-nav-bar__title")?.textContent).toContain("自定义颜色");
  });

  it("supports NavBar direct control, title truncation, custom slots, and left/right events", async () => {
    const screen = render({
      components: { NavBar },
      data: () => ({ leftClicks: 0, rightClicks: 0 }),
      template: `<NavBar title="这是一个非常长的标题" :title-max-length="6" left-arrow @left-click="leftClicks += 1" @right-click="rightClicks += 1"><template #right><button type="button" @click="$emit('right-click')">操作</button></template></NavBar>`,
    });
    const nav = screen.getByRole("navigation", { name: "页面导航" });
    await expect.element(nav.getByRole("button", { name: "返回" })).toBeVisible();
    expect(nav.getByText("这是一个非…", { exact: true })).toBeVisible();
    await nav.getByRole("button", { name: "返回" }).click();
    await expect.element(screen.getByRole("navigation")).toHaveTextContent("这是一个非…");
  });

  it("renders BottomBar variants with badges and controlled selection", async () => {
    const screen = render(UiFoundationView);
    const gallery = screen.getByTestId("bottom-bar-gallery");
    await expect.element(gallery.getByTestId("foundation-bottom-bar-standard")).toBeVisible();
    await expect.element(gallery.getByTestId("foundation-bottom-bar-text")).toBeVisible();
    await expect.element(gallery.getByTestId("foundation-bottom-bar-icons")).toBeVisible();
    await expect.element(gallery.getByTestId("foundation-bottom-bar-floating")).toBeVisible();
    expect(document.querySelectorAll(".nh-bottom-bar__badge")).toHaveLength(2);
    for (const bar of Array.from(gallery.element().querySelectorAll<HTMLElement>(".nh-bottom-bar"))) {
      expect(bar.scrollWidth).toBeLessThanOrEqual(bar.clientWidth);
    }
    expect(gallery.getByTestId("foundation-bottom-bar-icons").element().classList.contains("nh-bottom-bar--indicator-pill")).toBe(true);
    expect(gallery.getByTestId("foundation-bottom-bar-standard").element().classList.contains("nh-bottom-bar--item-divider")).toBe(true);
    expect(gallery.getByTestId("foundation-bottom-bar-floating").element().classList.contains("nh-bottom-bar--bordered")).toBe(false);
    expect(document.querySelector(".nh-bottom-bar--indicator-line .nh-bottom-bar__item--active::before")).toBeNull();
    const standard = gallery.getByTestId("foundation-bottom-bar-standard");
    await standard.getByRole("button", { name: "应用" }).click();
    await expect.element(standard.getByRole("button", { name: "应用" })).toHaveAttribute("aria-current", "page");
    expect(standard.element().querySelector(".nh-bottom-bar__item--active .nh-bottom-bar__icon--indicator")).not.toBeNull();
  });

  it("supports BottomBar directly with text-only, icon-only, floating, placeholder, and reselect events", async () => {
    const screen = render({
      components: { BottomBar, IconActivity, IconSettings },
      data: () => ({ value: "home", reselects: 0, textItems: [{ value: "home", label: "首页" }, { value: "settings", label: "设置" }], iconItems: [{ value: "home", label: "首页", icon: IconActivity }, { value: "settings", label: "设置", icon: IconSettings }] }),
      template: `<div><BottomBar v-model="value" :items="textItems" :fixed="false" :safe-area-inset-bottom="false" :show-label="true" @reselect="reselects += 1" data-testid="text-bar" /><BottomBar v-model="value" :items="iconItems" :safe-area-inset-bottom="false" :show-label="false" variant="floating" placeholder data-testid="icon-bar" /><output data-testid="reselects">{{ reselects }}</output></div>`,
    });
    await expect.element(screen.getByTestId("text-bar").getByRole("button", { name: "首页" })).toBeVisible();
    expect(screen.getByTestId("text-bar").element().querySelectorAll(".nh-bottom-bar__icon")).toHaveLength(0);
    expect(screen.getByTestId("icon-bar").element().querySelectorAll(".nh-bottom-bar__label")).toHaveLength(0);
    expect(document.querySelector(".nh-bottom-bar__placeholder")).not.toBeNull();
    await screen.getByTestId("text-bar").getByRole("button", { name: "首页" }).click();
    await expect.element(screen.getByTestId("reselects")).toHaveTextContent("1");
  });

  it("opens Dropdown, supports nested panels, selection, and outside dismissal", async () => {
    const screen = render(UiFoundationView);
    const gallery = screen.getByTestId("dropdown-gallery");
    const trigger = gallery.getByRole("button", { name: "文件操作" });
    await trigger.click();
    await expect.element(screen.getByText("编辑", { exact: true })).toBeVisible();
    const initialPanel = document.querySelector("[data-testid='foundation-dropdown'] .nh-dropdown__panel") as HTMLElement;
    await expect.element(initialPanel).toBeVisible();
    const listButton = document.querySelector("[data-testid='foundation-dropdown'] .nh-list-item-button > button") as HTMLElement;
    expect(getComputedStyle(listButton).fontSize).toBe("12px");
    expect(getComputedStyle(listButton).paddingLeft).toBe("10px");
    expect(getComputedStyle(listButton).transitionProperty).toContain("background-color");
    await screen.getByRole("button", { name: "分享", exact: true }).click();
    await expect.element(screen.getByText("复制链接", { exact: true })).toBeVisible();
    await screen.getByText("复制链接", { exact: true }).click();
    await expect.element(gallery).toHaveTextContent("当前选择：复制链接");
    await new Promise<void>((resolve) => window.setTimeout(resolve, 260));
    expect(document.querySelector("[data-testid='foundation-dropdown'] .nh-dropdown__panel")).toBeNull();
  });

  it("keeps explicit end alignment and exposes the actual collision side", async () => {
    const screen = render(UiFoundationView);
    const gallery = screen.getByTestId("dropdown-gallery");
    await gallery.getByRole("button", { name: "右侧菜单" }).click();
    const panel = document.querySelector("[data-testid='foundation-end-dropdown'] .nh-dropdown__panel") as HTMLElement;
    await expect.element(panel).toBeVisible();
    expect(panel.dataset.align).toBe("end");
    expect(["top", "bottom"]).toContain(panel.dataset.side);
    expect(panel.dataset.positioned).toBe("true");
    expect(getComputedStyle(panel).overflowX).toBe("visible");
  });

  it("aligns bottom-end on the first frame and flips above a bottom-edge trigger", async () => {
    const screen = render({
      components: { Dropdown, List, ListItemButton },
      data: () => ({ open: false }),
      template: `<Dropdown v-model:open="open" placement="bottom-end" data-testid="edge-dropdown" style="position: fixed; right: 12px; bottom: 2px"><template #trigger><button type="button">边缘触发器</button></template><List spacing="none"><ListItemButton compact>第一项</ListItemButton><ListItemButton compact>第二项</ListItemButton></List></Dropdown>`,
    });
    await screen.getByRole("button", { name: "边缘触发器" }).click();
    const panel = document.querySelector("[data-testid='edge-dropdown'] .nh-dropdown__panel") as HTMLElement;
    await expect.element(panel).toBeVisible();
    const trigger = document.querySelector(".nh-dropdown__trigger[data-testid='edge-dropdown'] button") as HTMLElement;
    const triggerRect = trigger.getBoundingClientRect();
    expect(panel.dataset.side).toBe("top");
    expect(panel.dataset.align).toBe("end");
    const panelLayoutRight = Number.parseFloat(panel.style.left) + panel.offsetWidth;
    expect(Math.abs(panelLayoutRight - triggerRect.right)).toBeLessThanOrEqual(1);
    const originX = panel.style.transformOrigin.split(" ")[0] ?? "";
    const expectedOriginX = triggerRect.left + triggerRect.width / 2 - Number.parseFloat(panel.style.left);
    expect(Math.abs(Number.parseFloat(originX) - expectedOriginX)).toBeLessThanOrEqual(1);
    expect(panel.style.transformOrigin).toContain("calc(100% + 6px)");
    expect(getComputedStyle(panel).animationName).toContain("nh-dropdown-in-top");
    const scroll = panel.querySelector<HTMLElement>(".nh-dropdown-panel__scroll")!;
    expect(scroll.scrollWidth).toBeLessThanOrEqual(scroll.clientWidth);
  });

  it("positions a Dropdown that is open on its initial mount", async () => {
    const screen = render({
      components: { Dropdown },
      data: () => ({ open: true }),
      template: `<Dropdown v-model:open="open" data-testid="initial-open-dropdown"><template #trigger><button type="button">初始打开触发器</button></template><button type="button">初始打开内容</button></Dropdown>`,
    });
    await expect.element(screen.getByRole("button", { name: "初始打开内容" })).toBeVisible();
    const panel = document.querySelector<HTMLElement>("[data-testid='initial-open-dropdown'] .nh-dropdown__panel")!;
    expect(panel.dataset.positioned).toBe("true");
    expect(Number.isFinite(Number.parseFloat(panel.style.left))).toBe(true);
    expect(Number.isFinite(Number.parseFloat(panel.style.top))).toBe(true);
  });

  it("retains the panel DOM and its local state when destroyOnClose is false", async () => {
    const screen = render({
      components: { Dropdown },
      data: () => ({ open: true }),
      template: `<Dropdown v-model:open="open" :destroy-on-close="false" data-testid="retained-dropdown"><template #trigger><button type="button">保留触发器</button></template><input aria-label="保留输入" /></Dropdown>`,
    });
    const input = screen.getByRole("textbox", { name: "保留输入" });
    await expect.element(input).toBeVisible();
    await input.fill("保留状态");
    const retainedPanel = document.querySelector<HTMLElement>("[data-testid='retained-dropdown'] .nh-dropdown__panel")!;
    await screen.getByRole("button", { name: "保留触发器" }).click();
    await new Promise<void>((resolve) => window.setTimeout(resolve, 180));
    expect(document.querySelector("[data-testid='retained-dropdown'] .nh-dropdown__panel")).toBe(retainedPanel);
    expect(document.querySelector(".nh-dropdown[data-testid='retained-dropdown']")?.classList.contains("nh-dropdown--inactive")).toBe(true);
    await screen.getByRole("button", { name: "保留触发器" }).click();
    await expect.element(input).toBeVisible();
    await expect.element(input).toHaveValue("保留状态");
    expect(document.querySelector("[data-testid='retained-dropdown'] .nh-dropdown__panel")).toBe(retainedPanel);
  });

  it("keeps Dropdown as a non-modal overlay and supports direct component control", async () => {
    const screen = render({
      components: { Dropdown },
      data: () => ({ open: false }),
      template: `<div><Dropdown v-model:open="open" placement="top-end"><template #trigger><button type="button">触发</button></template><template #default="{ close }"><button type="button" @click="close">关闭</button></template></Dropdown><button type="button">外部</button></div>`,
    });
    await screen.getByRole("button", { name: "触发" }).click();
    await expect.element(screen.getByRole("button", { name: "关闭" })).toBeVisible();
    expect(document.body.style.overflow).toBe("");
    await screen.getByRole("button", { name: "外部" }).click();
    await new Promise<void>((resolve) => window.setTimeout(resolve, 260));
    await expect.poll(() => document.querySelector(".nh-dropdown__panel")).toBeNull();
  });

  it("supports hover trigger with delayed open and keeps the panel open while hovered", async () => {
    const screen = render(UiFoundationView);
    const trigger = document.querySelector("[data-testid='foundation-hover-dropdown']") as HTMLElement;
    trigger.dispatchEvent(new Event("mouseenter", { bubbles: true }));
    await new Promise<void>((resolve) => window.setTimeout(resolve, 100));
    await expect.element(screen.getByText("悬停操作", { exact: true })).toBeVisible();
    document.querySelector(".nh-dropdown__panel")?.dispatchEvent(new Event("mouseenter", { bubbles: true }));
    trigger.dispatchEvent(new Event("mouseleave", { bubbles: true }));
    await new Promise<void>((resolve) => window.setTimeout(resolve, 80));
    await expect.element(screen.getByText("悬停操作", { exact: true })).toBeVisible();
  });

  it("opens context Dropdown at the pointer position", async () => {
    const screen = render(UiFoundationView);
    const target = document.querySelector("[data-testid='foundation-context-dropdown']") as HTMLElement;
    target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 120, clientY: 180 }));
    await expect.element(screen.getByText("上下文操作", { exact: true })).toBeVisible();
    const panel = document.querySelector("[data-testid='foundation-context-dropdown'] .nh-dropdown__panel") as HTMLElement;
    expect(panel.dataset.side).toBeDefined();
    await new Promise<void>((resolve) => window.setTimeout(resolve, 300));
    const rect = panel.getBoundingClientRect();
    expect(rect.left).toBeGreaterThanOrEqual(0);
    expect(rect.top).toBeGreaterThanOrEqual(0);
  });

  it("opens a true side cascade submenu without horizontal overflow", async () => {
    const screen = render(UiFoundationView);
    const cascade = screen.getByTestId("foundation-cascade-dropdown");
    await cascade.getByRole("button", { name: "新建" }).click();
    await screen.getByRole("button", { name: "从模板创建", exact: true }).click();
    await expect.element(screen.getByRole("button", { name: "周报模板" })).toBeVisible();
    const panel = document.querySelector(".nh-dropdown__panel") as HTMLElement;
    const submenuTrigger = screen.getByRole("button", { name: "从模板创建", exact: true });
    const triggerElement = document.querySelector<HTMLElement>(".nh-dropdown-submenu__trigger")!;
    const arrow = triggerElement.querySelector<SVGElement>("svg")!;
    expect(triggerElement.getBoundingClientRect().right - arrow.getBoundingClientRect().right).toBeLessThanOrEqual(12);
    await expect.element(submenuTrigger).toBeVisible();
    expect(getComputedStyle(panel).overflowX).toBe("visible");
    expect(document.querySelector(".nh-dropdown-submenu__panel")).not.toBeNull();
  });

  it("closes and resets a retained side submenu with its parent Dropdown", async () => {
    const screen = render({
      components: { Dropdown, DropdownSubmenu },
      data: () => ({ open: false }),
      template: `<Dropdown v-model:open="open" :destroy-on-close="false" data-testid="retained-cascade"><template #trigger><button type="button">级联触发器</button></template><ul><DropdownSubmenu label="级联入口"><button type="button">级联内容</button></DropdownSubmenu></ul></Dropdown>`,
    });
    await screen.getByRole("button", { name: "级联触发器" }).click();
    const submenuTrigger = screen.getByRole("button", { name: "级联入口", exact: true });
    await submenuTrigger.click();
    await expect.element(screen.getByRole("button", { name: "级联内容" })).toBeVisible();
    const submenuPanel = document.querySelector<HTMLElement>(".nh-dropdown-submenu__panel")!;
    await screen.getByRole("button", { name: "级联触发器" }).click();
    await expect.poll(() => submenuPanel.classList.contains("nh-dropdown-submenu__panel--closing")).toBe(true);
    expect(getComputedStyle(submenuPanel).animationName).toContain("nh-dropdown-submenu-out");
    await new Promise<void>((resolve) => window.setTimeout(resolve, 180));
    expect(document.querySelector(".nh-dropdown-submenu__panel")).toBeNull();
    await screen.getByRole("button", { name: "级联触发器" }).click();
    await expect.element(submenuTrigger).toBeVisible();
    await expect.element(submenuTrigger).toHaveAttribute("aria-expanded", "false");
    await submenuTrigger.click();
    await expect.element(screen.getByRole("button", { name: "级联内容" })).toBeVisible();
  });

  it("composes SplitButton from independent main and dropdown actions", async () => {
    const screen = render({
      components: { SplitButton, DropdownSubmenu },
      data: () => ({ mainClicks: 0, menuClicks: 0 }),
      template: '<div><SplitButton label="导出" data-testid="split" @click="mainClicks += 1"><template #menu="{ close }"><button type="button" @click="menuClicks += 1; close()">导出 PNG</button></template></SplitButton><output data-testid="split-count">{{ mainClicks }}:{{ menuClicks }}</output></div>',
    });
    await screen.getByRole("button", { name: "导出", exact: true }).click();
    await expect.element(screen.getByTestId("split-count")).toHaveTextContent("1:0");
    await screen.getByRole("button", { name: "导出更多操作" }).click();
    await screen.getByRole("button", { name: "导出 PNG" }).click();
    await expect.element(screen.getByTestId("split-count")).toHaveTextContent("1:1");
  });

  it("renders Toast progress, action buttons, operation updates, and placement", async () => {
    const screen = render(UiFoundationView);
    const gallery = screen.getByTestId("toast-gallery");
    await gallery.getByTestId("toast-pending").click();
    const toast = screen.getByText("正在同步节点", { exact: true });
    await expect.element(toast).toHaveTextContent("正在同步节点");
    expect(document.querySelector(".nh-toast--entering")).not.toBeNull();
    expect(getComputedStyle(document.querySelector(".nh-toast--entering") as HTMLElement).animationName).toContain("nh-toast-enter");
    const enteringToast = document.querySelector(".nh-toast--entering") as HTMLElement;
    const initialTransform = getComputedStyle(enteringToast).transform;
    await new Promise<void>((resolve) => window.setTimeout(resolve, 120));
    const midTransform = getComputedStyle(enteringToast).transform;
    expect(midTransform).not.toBe(initialTransform);
    await expect.element(screen.getByRole("button", { name: "撤回" })).toBeVisible();
    expect(document.querySelector(".nh-toast__progress")).not.toBeNull();
    const hoverToast = document.querySelector("[data-toast-id='foundation-operation']") as HTMLElement;
    hoverToast.dispatchEvent(new MouseEvent("mouseenter", { bubbles: true }));
    const pausedProgress = document.querySelector(".nh-toast__progress > span")?.getAttribute("style");
    await new Promise<void>((resolve) => window.setTimeout(resolve, 140));
    expect(document.querySelector(".nh-toast__progress > span")?.getAttribute("style")).toBe(pausedProgress);
    hoverToast.dispatchEvent(new MouseEvent("mouseleave", { bubbles: true }));

    await screen.getByRole("button", { name: "撤回" }).click();
    await expect.element(screen.getByText("已撤回节点同步", { exact: true })).toBeVisible();
    await gallery.getByTestId("toast-success").click();
    await expect.element(screen.getByText("节点同步完成", { exact: true })).toBeVisible();
    expect(document.querySelector(".nh-toast--pulsing")).not.toBeNull();

    await gallery.getByTestId("toast-placement").selectOptions("top-end");
    expect(document.querySelector(".nh-toast-host--top-end")).not.toBeNull();
    await gallery.getByTestId("toast-all-types").click();
    await expect.element(gallery.getByText("信息提示", { exact: true })).toBeVisible();
    await expect.element(gallery.getByText("正在同步节点", { exact: true })).toBeVisible();
    await expect.element(gallery.getByText("节点同步完成", { exact: true })).toBeVisible();
    await expect.element(gallery.getByText("节点数据已过期", { exact: true })).toBeVisible();
    await expect.element(gallery.getByText("节点同步失败", { exact: true })).toBeVisible();
    expect(document.querySelectorAll(".nh-toast")).toHaveLength(5);
    expect(document.querySelector(".nh-toast-host [data-toast-id='foundation-error']")?.getAttribute("role")).toBe("alert");
    expect(document.querySelector(".nh-toast-host [data-toast-id='foundation-loading'] .nh-toast__spinner")).not.toBeNull();
    await expect.element(screen.getByRole("button", { name: "关闭错误提示" })).toBeVisible();
    expect(document.querySelector(".nh-toast--shaking .nh-toast__content")).not.toBeNull();
    expect(document.querySelector(".nh-toast--success .nh-toast__icon path:last-child")).not.toBeNull();
  });

  it("keeps ToastHost non-modal and supports error alert semantics", async () => {
    const screen = render({
      components: { Toast, ToastHost },
      data: () => ({ items: [{ id: "error-1", tone: "error", message: "同步失败", persistent: true }] }),
      template: `<ToastHost :items="items" placement="top-center" />`,
    });
    await expect.element(screen.getByRole("alert")).toHaveTextContent("同步失败");
    expect(document.body.style.overflow).toBe("");
    expect(document.querySelector(".nh-toast-host--top-center")).not.toBeNull();
  });

  it("supports MenuList roving focus, type-ahead, disabled skipping, and selection", async () => {
    const screen = render(UiFoundationView);
    const menu = screen.getByTestId("foundation-menu");
    await expect.element(menu).toHaveAttribute("role", "menu");
    const refresh = menu.getByRole("menuitem", { name: "刷新节点" });
    await refresh.click();
    await expect.element(screen.getByTestId("menu-selection-value")).toHaveTextContent("当前：refresh");
    const refreshElement = document.querySelector("[data-testid='foundation-menu'] [data-value='refresh']") as HTMLElement;
    const editElement = document.querySelector("[data-testid='foundation-menu'] [data-value='edit']") as HTMLElement;
    refreshElement.focus();
    refreshElement.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(document.activeElement).toBe(editElement);
    editElement.dispatchEvent(new KeyboardEvent("keydown", { key: "e", bubbles: true }));
    expect(document.activeElement).toBe(editElement);
    await expect.element(menu.getByRole("menuitem", { name: "暂不可用" })).toBeDisabled();
    expect(document.querySelectorAll("[data-testid='foundation-menu'] [role='separator']")).toHaveLength(1);
  });

  it("renders MenuList listbox semantics and updates selected option", async () => {
    const screen = render({
      components: { MenuList, MenuSection, MenuItem },
      data: () => ({ selected: "one" }),
      template: `<MenuList v-model="selected" semantic="listbox" aria-label="选项"><MenuSection title="分组"><MenuItem value="one">一</MenuItem><MenuItem value="two">二</MenuItem></MenuSection></MenuList>`,
    });

    const listbox = screen.getByRole("listbox", { name: "选项" });
    const one = listbox.getByRole("option", { name: "一" });
    const two = listbox.getByRole("option", { name: "二" });
    await expect.element(one).toHaveAttribute("aria-selected", "true");
    await two.click();
    await expect.element(two).toHaveAttribute("aria-selected", "true");
    await expect.element(one).toHaveAttribute("aria-selected", "false");
  });

  it("renders structured PageState models with native feedback semantics", async () => {
    const screen = render(UiFoundationView);
    const gallery = screen.getByTestId("page-state-gallery");
    await expect.element(gallery.getByTestId("page-state-loading").getByRole("status")).toHaveAttribute("aria-busy", "true");
    await expect.element(gallery.getByTestId("page-state-empty").getByRole("status")).toHaveTextContent("暂无节点");
    await expect.element(gallery.getByTestId("page-state-error").getByRole("alert")).toHaveTextContent("节点加载失败");
    await expect.element(gallery.getByTestId("page-state-warning").getByRole("status")).toHaveTextContent("节点数据已过期");
    await gallery.getByTestId("page-state-error").getByRole("button", { name: "重新加载" }).click();
    await expect.element(document.querySelector("[data-testid='page-state-error'] .nh-page-state") as HTMLElement).toHaveAttribute("aria-busy", "true");
    await expect.element(gallery.getByTestId("page-state-error")).toHaveTextContent("正在加载节点");
    await expect.element(gallery.getByTestId("page-state-error")).toHaveTextContent("已触发 1 次");
  });

  it("keeps ready PageState silent and supports action slots", async () => {
    const screen = render({
      components: { PageState },
      data: () => ({ model: { type: "ready" } }),
      template: `<div><PageState :model="model" data-testid="ready-state" /><PageState :model="{ type: 'warning', title: '注意' }"><template #action><button type="button">查看</button></template></PageState></div>`,
    });

    expect(document.querySelector("[data-testid='ready-state']")).toBeNull();
    await expect.element(screen.getByRole("status")).toHaveTextContent("注意");
    await expect.element(screen.getByRole("button", { name: "查看" })).toBeVisible();
  });

  it("opens a modal Dialog with semantics, focus trap, and focus restoration", async () => {
    const screen = render(UiFoundationView);
    const trigger = screen.getByTestId("open-basic-dialog");
    const triggerElement = document.querySelector("[data-testid='open-basic-dialog']");
    (triggerElement as HTMLElement).focus();
    await trigger.click();

    const dialog = screen.getByRole("dialog", { name: "基础对话框" });
    await expect.element(dialog).toBeVisible();
    await expect.element(dialog).toHaveAttribute("aria-modal", "true");
    expect(getComputedStyle(document.querySelector(".nh-dialog__mask") as HTMLElement).backdropFilter).toContain("blur");
    expect(getComputedStyle(document.querySelector(".nh-dialog__panel") as HTMLElement).willChange).toContain("transform");
    expect(document.body.style.overflow).toBe("hidden");

    const panel = document.querySelector(".nh-dialog__panel") as HTMLElement;
    const closeButton = document.querySelector(".nh-dialog__actions .nh-button") as HTMLElement;
    closeButton.focus();
    panel.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    expect(document.activeElement).toBe(closeButton);

    await closeButton.click();
    await expect.poll(() => document.querySelector("[data-testid='basic-dialog']")).toBeNull();
    expect(document.activeElement).toBe(triggerElement);
    expect(document.body.style.overflow).toBe("");
  });

  it("keeps a guarded Dialog open until its async dismiss guard allows closing", async () => {
    const screen = render(UiFoundationView);
    await screen.getByTestId("open-guarded-dialog").click();
    const dialog = screen.getByRole("dialog", { name: "未保存的编辑" });
    await expect.element(dialog).toBeVisible();

    expect(dispatchBack()).toBe(true);
    await expect.element(dialog).toBeVisible();
    await expect.element(screen.getByText("仍有未保存内容，关闭已阻止")).toBeVisible();

    await screen.getByText("模拟校验通过", { exact: true }).click();
    await screen.getByRole("button", { name: "保存并关闭" }).click();
    await expect.poll(() => document.querySelector("[data-testid='guarded-dialog']")).toBeNull();
  });

  it("keeps dangerous Dialog focus on the dialog root and exposes an explicit close button", async () => {
    const screen = render(UiFoundationView);
    await screen.getByTestId("open-danger-dialog").click();
    await expect.element(screen.getByRole("dialog", { name: "删除节点？" })).toBeVisible();
    const panel = document.querySelector("[data-testid='danger-dialog'] .nh-dialog__panel") as HTMLElement;
    expect(document.activeElement).toBe(panel);
    await expect.element(screen.getByRole("button", { name: "关闭删除确认" })).toBeVisible();
  });

  it("shows interactive Switch, Checkbox, and Radio examples", async () => {
    const screen = render(UiFoundationView);
    const selection = screen.getByTestId("selection-gallery");

    const switchControl = selection.getByRole("switch", { name: "允许代理接管" });
    await expect.element(switchControl).toHaveAttribute("aria-checked", "true");
    await switchControl.click();
    await expect.element(switchControl).toHaveAttribute("aria-checked", "false");
    await expect.element(selection.getByTestId("foundation-switch-text")).toHaveClass("nh-switch--with-text");
    await expect.element(selection.getByTestId("foundation-switch-icon")).toHaveClass("nh-switch--with-icon");
    expect(document.querySelector("[data-testid='foundation-switch-text'] .nh-switch__label--on")?.textContent).toBe("开");
    expect(document.querySelector("[data-testid='foundation-switch-text'] .nh-switch__label--off")?.textContent).toBe("关");
    expect(document.querySelectorAll("[data-testid='foundation-switch-icon'] .nh-switch__icon")).toHaveLength(2);

    const switchBox = document.querySelector("[data-testid='foundation-switch']") as HTMLElement;
    const trackBox = switchBox.querySelector(".nh-switch__track")?.getBoundingClientRect();
    const thumbBox = switchBox.querySelector(".nh-switch__thumb")?.getBoundingClientRect();
    expect(trackBox).toBeDefined();
    expect(thumbBox).toBeDefined();
    expect(Math.abs((thumbBox!.top + thumbBox!.height / 2) - (trackBox!.top + trackBox!.height / 2))).toBeLessThanOrEqual(1);

    const accepted = selection.getByRole("checkbox", { name: "接受服务条款" });
    await expect.element(accepted).toBeChecked();
    await selection.getByText("接受服务条款", { exact: true }).click();
    await expect.element(accepted).not.toBeChecked();
    expect(document.querySelector("[data-testid='foundation-checkbox-circle']")?.closest(".nh-checkbox")).toHaveClass("nh-checkbox--circle");

    const auto = selection.getByRole("radio", { name: "自动选择" });
    const manual = selection.getByRole("radio", { name: "手动选择" });
    await expect.element(auto).toBeChecked();
    await selection.getByText("手动选择", { exact: true }).click();
    await expect.element(manual).toBeChecked();
    await expect.element(auto).not.toBeChecked();
    await expect.element(selection.getByTestId("foundation-radio-value")).toHaveTextContent("当前：manual");
    await expect.element(selection.getByTestId("foundation-radio-group")).toHaveAttribute("role", "radiogroup");
    const foundationRadioGroup = document.querySelector("[data-testid='foundation-radio-group']") as HTMLElement;
    foundationRadioGroup.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
    await expect.element(auto).toBeChecked();
    expect(document.activeElement).toBe(document.querySelector("[data-testid='foundation-radio-auto']"));
  });

  it("renders List as a narrow semantic container", async () => {
    const screen = render({
      components: { List, ListItem },
      template: `<div><List aria-label="普通列表" divided inset="m" data-testid="list"><ListItem data-testid="first">第一项</ListItem><ListItem selected disabled data-testid="second">第二项</ListItem></List><List as="ol" aria-label="有序列表" data-testid="ordered"><ListItem>步骤一</ListItem></List></div>`,
    });

    await expect.element(screen.getByRole("list", { name: "普通列表" })).toBeVisible();
    expect(document.querySelector("[data-testid='list']")?.tagName).toBe("UL");
    expect(document.querySelector("[data-testid='ordered']")?.tagName).toBe("OL");
    expect(document.querySelectorAll("[data-testid='list'] > li")).toHaveLength(2);
    await expect.element(screen.getByTestId("list")).toHaveClass("nh-list--divided");
    await expect.element(screen.getByTestId("list")).toHaveClass("nh-list--inset-m");
    await expect.element(screen.getByTestId("second")).toHaveAttribute("data-selected", "true");
    await expect.element(screen.getByTestId("second")).toHaveAttribute("aria-disabled", "true");
  });

  it("keeps ListItem slots structural and ListItemButton natively interactive", async () => {
    const screen = render({
      components: { List, ListItem, ListItemButton },
      data: () => ({ clicks: 0 }),
      template: `<div><List aria-label="结构列表"><ListItem data-testid="item"><template #leading><span data-testid="leading">前</span></template><strong>标题</strong><small>说明</small><template #trailing><span data-testid="trailing">后</span></template></ListItem></List><List aria-label="操作列表"><ListItemButton @click="clicks += 1">执行</ListItemButton><ListItemButton selected disabled aria-pressed="true">禁用</ListItemButton></List><output data-testid="clicks">{{ clicks }}</output></div>`,
    });

    expect(document.querySelector("[data-testid='item'] > .nh-list-item__leading [data-testid='leading']")).not.toBeNull();
    expect(document.querySelector("[data-testid='item'] > .nh-list-item__content strong")?.textContent).toBe("标题");
    expect(document.querySelector("[data-testid='item'] > .nh-list-item__trailing [data-testid='trailing']")).not.toBeNull();
    await screen.getByRole("button", { name: "执行" }).click();
    await expect.element(screen.getByTestId("clicks")).toHaveTextContent("1");
    await expect.element(screen.getByRole("button", { name: "禁用" })).toBeDisabled();
    await expect.element(screen.getByRole("button", { name: "禁用" })).toHaveAttribute("aria-pressed", "true");
  });

  it("keeps ListSection independent from list data and interaction", async () => {
    const screen = render({
      components: { ListSection, List, ListItem },
      template: `<ListSection title="网络" description="连接设置" data-testid="section"><List aria-label="网络选项"><ListItem>DNS</ListItem></List></ListSection>`,
    });

    await expect.element(screen.getByText("网络", { exact: true })).toBeVisible();
    await expect.element(screen.getByText("连接设置", { exact: true })).toBeVisible();
    await expect.element(screen.getByRole("list", { name: "网络选项" })).toBeVisible();
    expect(document.querySelector("[data-testid='section']")?.tagName).toBe("SECTION");
  });

  it("composes setting rows from independent trailing controls and navigation buttons", async () => {
    const screen = render(UiFoundationView);
    const composition = screen.getByTestId("settings-list-composition");
    const basicList = screen.getByTestId("list-basic");
    const proxySwitch = basicList.getByRole("switch", { name: "允许代理接管" });
    await expect.element(proxySwitch).toHaveAttribute("aria-checked", "true");
    await proxySwitch.click();
    await expect.element(proxySwitch).toHaveAttribute("aria-checked", "false");

    await composition.getByRole("button", { name: "深色" }).click();
    await expect.element(composition.getByRole("button", { name: "深色" })).toHaveAttribute("data-active", "true");

    const networkRow = composition.getByRole("button", { name: /网络接管/ });
    await expect.element(networkRow).toBeVisible();
    expect(networkRow.element().querySelectorAll("button")).toHaveLength(0);
  });

  it("keeps Tag as a non-interactive status primitive with tone and size contracts", async () => {
    const screen = render({
      components: { Tag },
      template: `<div><Tag data-testid="tag-neutral">普通</Tag><Tag tone="warning" size="m" data-testid="tag-warning">待更新</Tag></div>`,
    });

    await expect.element(screen.getByTestId("tag-neutral")).toHaveTextContent("普通");
    await expect.element(screen.getByTestId("tag-warning")).toHaveTextContent("待更新");
    expect(document.querySelectorAll("[data-testid='tag-neutral'] button, [data-testid='tag-warning'] button")).toHaveLength(0);
    expect(document.querySelector("[data-testid='tag-neutral']")?.classList.contains("nh-tag--neutral")).toBe(true);
    expect(document.querySelector("[data-testid='tag-warning']")?.classList.contains("nh-tag--warning")).toBe(true);
    expect(document.querySelector("[data-testid='tag-warning']")?.classList.contains("nh-tag--m")).toBe(true);
  });

  it("supports Tag visual variants, pill shape, icon slot, and text truncation", async () => {
    const screen = render({
      components: { Tag },
      template: `<div><Tag tone="info" variant="solid" data-testid="tag-solid">实心</Tag><Tag tone="success" shape="pill" variant="outline" data-testid="tag-pill"><template #icon><svg /></template>胶囊</Tag><Tag data-testid="tag-long" style="max-width: 100px">这是一段很长的标签文字</Tag></div>`,
    });

    await expect.element(screen.getByTestId("tag-solid")).toHaveClass("nh-tag--solid");
    await expect.element(screen.getByTestId("tag-pill")).toHaveClass("nh-tag--pill");
    expect(document.querySelector("[data-testid='tag-pill'] svg")).not.toBeNull();
    expect(getComputedStyle(document.querySelector("[data-testid='tag-pill'] svg") as SVGElement).width).toBe("12px");
    expect(getComputedStyle(document.querySelector("[data-testid='tag-long'] .nh-tag__label") as HTMLElement).textOverflow).toBe("ellipsis");
    expect(document.querySelectorAll("[data-testid='tag-solid'] button, [data-testid='tag-pill'] button")).toHaveLength(0);
  });

  it("renders Divider with semantic orientation, inset, label alignment, and dashed variants", async () => {
    const screen = render({
      components: { Divider },
      template: `<div><Divider data-testid="divider-solid" /><Divider orientation="vertical" data-testid="divider-vertical" /><Divider label="文字信息" align="end" variant="dashed" inset="m" data-testid="divider-label" /></div>`,
    });

    await expect.element(screen.getByRole("separator").first()).toHaveAttribute("aria-orientation", "horizontal");
    expect(document.querySelector("[data-testid='divider-solid']")?.classList.contains("nh-divider--solid")).toBe(true);
    expect(document.querySelector("[data-testid='divider-vertical']")?.getAttribute("aria-orientation")).toBe("vertical");
    expect(document.querySelector("[data-testid='divider-label']")?.classList.contains("nh-divider--align-end")).toBe(true);
    expect(document.querySelector("[data-testid='divider-label']")?.classList.contains("nh-divider--dashed")).toBe(true);
    expect(document.querySelector("[data-testid='divider-label'] .nh-divider__label")?.textContent).toBe("文字信息");
    expect(getComputedStyle(document.querySelector("[data-testid='divider-label']") as HTMLElement).marginInline).toBe("16px");
  });

  it("renders native Input semantics, v-model updates, slots, and visual states", async () => {
    const screen = render({
      components: { Input },
      data: () => ({ value: "", changed: "" }),
      template: `<div><Input v-model="value" variant="outline" :maxlength="10" invalid data-testid="input" @change="changed = $event"><template #prefix><svg /></template><template #suffix><span>尾部</span></template></Input><output data-testid="value">{{ value }}</output><output data-testid="changed">{{ changed }}</output></div>`,
    });

    const input = screen.getByRole("textbox");
    await expect.element(input).toHaveAttribute("maxlength", "10");
    await expect.element(input).toHaveAttribute("aria-invalid", "true");
    const inputRoot = document.querySelector("[data-testid='input']")?.parentElement as HTMLElement;
    expect(inputRoot.classList.contains("nh-input--outline")).toBe(true);
    expect(inputRoot.querySelector(".nh-input__prefix svg")).not.toBeNull();
    expect(inputRoot.querySelector(".nh-input__suffix")).not.toBeNull();
    await input.fill("NetHop");
    await expect.element(screen.getByTestId("value")).toHaveTextContent("NetHop");
    expect(inputRoot.classList.contains("nh-input--invalid")).toBe(true);
  });

  it("connects Field label, description, error, and required state to Input", async () => {
    const screen = render({
      components: { Field, Input },
      template: `<Field label="订阅链接" description="只接受 HTTPS" error="链接无效" required id="subscription-url-field"><Input data-testid="field-input" /></Field>`,
    });

    const input = screen.getByRole("textbox");
    expect(document.querySelector(".nh-field__label")?.textContent).toContain("订阅链接");
    await expect.element(input).toHaveAttribute("id", "subscription-url-field");
    await expect.element(input).toHaveAttribute("aria-invalid", "true");
    await expect.element(input).toHaveAttribute("aria-required", "true");
    const describedBy = document.querySelector("[data-testid='field-input']")?.getAttribute("aria-describedby");
    expect(describedBy).toContain("subscription-url-field-description");
    expect(describedBy).toContain("subscription-url-field-error");
    await expect.element(screen.getByRole("alert")).toHaveTextContent("链接无效");
  });

  it("renders native Textarea semantics, v-model, resize modes, and maxlength", async () => {
    const screen = render({
      components: { Textarea },
      data: () => ({ value: "", limited: "" }),
      template: `<div><Textarea v-model="value" variant="outline" :rows="3" resize="none" data-testid="textarea" /><Textarea v-model="limited" :maxlength="5" data-testid="textarea-limited" /><output data-testid="value">{{ value }}</output></div>`,
    });

    const textarea = screen.getByTestId("textarea");
    await expect.element(textarea).toHaveAttribute("rows", "3");
    await expect.element(textarea).toHaveClass("nh-textarea__control");
    await textarea.fill("NetHop\nUI");
    await expect.element(screen.getByTestId("value")).toHaveTextContent("NetHop UI");
    expect(document.querySelector("[data-testid='textarea']")?.parentElement?.classList.contains("nh-textarea--outline")).toBe(true);
    expect(document.querySelector("[data-testid='textarea']")?.parentElement?.classList.contains("nh-textarea--resize-none")).toBe(true);
    await expect.element(screen.getByTestId("textarea-limited")).toHaveAttribute("maxlength", "5");
  });

  it("supports Textarea autosize bounds and Field accessibility context", async () => {
    const screen = render({
      components: { Field, Textarea },
      template: `<Field label="订阅说明" description="支持多行" error="内容无效" required id="textarea-field"><Textarea :min-rows="2" :max-rows="4" data-testid="field-textarea" /></Field>`,
    });

    const textarea = screen.getByTestId("field-textarea");
    await expect.element(textarea).toHaveAttribute("id", "textarea-field");
    await expect.element(textarea).toHaveAttribute("aria-invalid", "true");
    await expect.element(textarea).toHaveAttribute("aria-required", "true");
    const describedBy = document.querySelector("[data-testid='field-textarea']")?.getAttribute("aria-describedby");
    expect(describedBy).toContain("textarea-field-description");
    expect(describedBy).toContain("textarea-field-error");
    expect(document.querySelector("[data-testid='field-textarea']")?.parentElement?.classList.contains("nh-textarea--resize-vertical")).toBe(true);
    await expect.element(screen.getByRole("alert")).toHaveTextContent("内容无效");
  });

  it("toggles PasswordInput visibility without losing value or focus", async () => {
    const screen = render({
      components: { PasswordInput },
      data: () => ({ value: "secret" }),
      template: `<PasswordInput v-model="value" data-testid="password" />`,
    });

    const input = screen.getByTestId("password");
    await expect.element(input).toHaveAttribute("type", "password");
    await screen.getByRole("button", { name: "显示密码" }).click();
    await expect.element(input).toHaveAttribute("type", "text");
    await expect.element(input).toHaveValue("secret");
    await expect.element(screen.getByRole("button", { name: "隐藏密码" })).toHaveAttribute("aria-pressed", "true");
    expect(document.activeElement).toBe(document.querySelector("[data-testid='password']"));
  });

  it("constrains InputNumber with stepper buttons, keyboard, and ARIA values", async () => {
    const screen = render({
      components: { InputNumber },
      data: () => ({ value: 1 }),
      template: `<InputNumber v-model="value" :min="0" :max="2" :step="0.5" :precision="1" aria-label="权重" data-testid="number" />`,
    });

    const input = screen.getByRole("spinbutton", { name: "权重" });
    await expect.element(input).toHaveAttribute("aria-valuemin", "0");
    await expect.element(input).toHaveAttribute("aria-valuemax", "2");
    await expect.element(input).toHaveAttribute("aria-valuenow", "1");
    await screen.getByRole("button", { name: "增加" }).click();
    await expect.element(input).toHaveAttribute("aria-valuenow", "1.5");
    (document.querySelector("[data-testid='number']") as HTMLInputElement).dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    await expect.element(input).toHaveAttribute("aria-valuenow", "1");
    await input.fill("9");
    (document.querySelector("[data-testid='number']") as HTMLInputElement).blur();
    await expect.element(input).toHaveAttribute("aria-valuenow", "2");
    await expect.element(screen.getByRole("button", { name: "增加" })).toBeDisabled();
  });

  it("exposes native Switch and Checkbox semantics with boolean updates", async () => {
    const screen = render({
      components: { Switch, Checkbox, Radio },
      data: () => ({ enabled: false, accepted: false }),
      template: `<div><Switch v-model="enabled" aria-label="启用代理" /><Checkbox v-model="accepted" aria-label="接受条款">接受条款</Checkbox><Radio :model-value="enabled" name="demo" aria-label="示例单选">示例</Radio><output data-testid="state">{{ enabled }}|{{ accepted }}</output></div>`,
    });

    const switchControl = screen.getByRole("switch", { name: "启用代理" });
    await expect.element(switchControl).toHaveAttribute("aria-checked", "false");
    await switchControl.click();
    await expect.element(switchControl).toHaveAttribute("aria-checked", "true");
    await screen.getByText("接受条款", { exact: true }).click();
    await expect.element(screen.getByTestId("state")).toHaveTextContent("true|true");
    await expect.element(screen.getByRole("radio", { name: "示例单选" })).toBeChecked();
  });

  it("coordinates Radio values and keyboard navigation through RadioGroup", async () => {
    const screen = render({
      components: { RadioGroup, Radio },
      data: () => ({ value: "one" }),
      template: `<RadioGroup v-model="value" name="demo-group" aria-label="演示选项"><Radio value="one">一</Radio><Radio value="two">二</Radio><Radio value="three" disabled>三</Radio><output data-testid="group-value">{{ value }}</output></RadioGroup>`,
    });

    const one = screen.getByRole("radio", { name: "一" });
    const two = screen.getByRole("radio", { name: "二" });
    const group = screen.getByRole("radiogroup", { name: "演示选项" });
    await expect.element(one).toBeChecked();
    const radioGroupElement = document.querySelector("[aria-label='演示选项']") as HTMLElement;
    radioGroupElement.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    await expect.element(two).toBeChecked();
    await expect.element(screen.getByTestId("group-value")).toHaveTextContent("two");
    const twoInput = document.querySelector("input[value='two']") as HTMLInputElement;
    radioGroupElement.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true }));
    expect(document.activeElement).toBe(twoInput);
    await expect.element(group).not.toHaveAttribute("aria-disabled");
  });

  it("composes RemovableTag from a display Tag and an accessible remove button", async () => {
    const screen = render({
      components: { RemovableTag },
      data: () => ({ removed: 0 }),
      template: `<div><RemovableTag remove-label="移除香港" @remove="removed += 1" data-testid="removable"><template #icon><svg /></template>香港</RemovableTag><output data-testid="removed">{{ removed }}</output></div>`,
    });

    expect(document.querySelector("[data-testid='removable'].nh-tag")).not.toBeNull();
    expect(document.querySelector("[data-testid='removable'] .nh-tag__end button")).not.toBeNull();
    expect(document.querySelector("[data-testid='removable'] .nh-tag__end svg")).not.toBeNull();
    const closeButton = document.querySelector("[data-testid='removable'] .nh-tag__end button") as HTMLElement;
    const closeSvg = closeButton.querySelector("svg") as SVGElement;
    expect(getComputedStyle(closeButton).width).toBe("14px");
    expect(getComputedStyle(closeButton).height).toBe("14px");
    expect(getComputedStyle(closeSvg).width).toBe("10px");
    await expect.element(screen.getByRole("button", { name: "移除香港" })).toBeVisible();
    await screen.getByRole("button", { name: "移除香港" }).click();
    await expect.element(screen.getByTestId("removed")).toHaveTextContent("1");
    expect(document.querySelector("[data-testid='removable'] .nh-tag__end button")?.getAttribute("type")).toBe("button");
  });

  it("renders SelectableTag as a real toggle button with aria-pressed", async () => {
    const screen = render({
      components: { SelectableTag },
      data: () => ({ selected: false }),
      template: `<SelectableTag v-model:selected="selected" tone="info" data-testid="selectable">香港</SelectableTag>`,
    });

    const selectable = screen.getByRole("button", { name: "香港" });
    await expect.element(selectable).toHaveAttribute("aria-pressed", "false");
    await selectable.click();
    await expect.element(selectable).toHaveAttribute("aria-pressed", "true");
    expect(document.querySelector("[data-testid='selectable'] .nh-tag--solid")).not.toBeNull();
  });

  it("coordinates multiple SelectableTags through TagGroup", async () => {
    const screen = render({
      components: { TagGroup, SelectableTag },
      data: () => ({ selected: ["香港"] }),
      template: `<TagGroup v-model="selected" aria-label="地区筛选"><SelectableTag value="香港" data-testid="group-hk">香港</SelectableTag><SelectableTag value="日本" data-testid="group-jp">日本</SelectableTag><output data-testid="group-value">{{ selected.join(',') }}</output></TagGroup>`,
    });

    await expect.element(screen.getByRole("button", { name: "香港" })).toHaveAttribute("aria-pressed", "true");
    await expect.element(screen.getByRole("button", { name: "日本" })).toHaveAttribute("aria-pressed", "false");
    await screen.getByRole("button", { name: "日本" }).click();
    await expect.element(screen.getByTestId("group-value")).toHaveTextContent("香港,日本");
    await expect.element(screen.getByRole("button", { name: "日本" })).toHaveAttribute("aria-pressed", "true");
  });

  it("keeps text button padding and content spacing stable", async () => {
    render({
      components: { Button },
      template: `<div><Button size="s" data-testid="text-button-s">小</Button><Button data-testid="text-button">文本按钮</Button><Button size="l" data-testid="text-button-l">大</Button></div>`,
    });

    const button = document.querySelector("[data-testid='text-button']") as HTMLElement;
    const content = button.querySelector(".nh-button__content") as HTMLElement;
    const buttonStyle = getComputedStyle(button);
    expect([buttonStyle.paddingTop, buttonStyle.paddingRight, buttonStyle.paddingBottom, buttonStyle.paddingLeft]).toEqual(["7px", "10px", "7px", "10px"]);
    expect(getComputedStyle(content).gap).toBe("6px");

    for (const [size, padding] of [["s", 8], ["m", 10], ["l", 14]] as const) {
      const testId = size === "m" ? "text-button" : `text-button-${size}`;
      const style = getComputedStyle(document.querySelector(`[data-testid='${testId}']`) as HTMLElement);
      expect([style.paddingRight, style.paddingLeft]).toEqual([`${padding}px`, `${padding}px`]);
    }
  });

  it("reuses Button behavior for IconButton and requires an accessible name", async () => {
    const screen = render(IconButton, { props: { ariaLabel: "刷新", loading: true } });
    const button = screen.getByRole("button", { name: "刷新" });
    await expect.element(button).toBeDisabled();
    await expect.element(button).toHaveAttribute("aria-busy", "true");
    await expect.element(button).toHaveClass("nh-icon-button");
  });

  it("keeps icon-only buttons square with four-sided inner spacing at every supported size", () => {
    render({
      components: { IconButton },
      template: `<div><IconButton v-for="size in ['s', 'm', 'l']" :key="size" :size="size" :aria-label="'图标 ' + size" :data-testid="'icon-' + size"><span aria-hidden="true">+</span></IconButton></div>`,
    });

    for (const [size, expected, padding] of [["s", 32, 6], ["m", 36, 8], ["l", 44, 10]] as const) {
      const button = document.querySelector(`[data-testid='icon-${size}']`) as HTMLElement;
      const box = button.getBoundingClientRect();
      const style = getComputedStyle(button);
      expect(box.width).toBe(box.height);
      expect(box.width).toBe(expected);
      expect([style.paddingTop, style.paddingRight, style.paddingBottom, style.paddingLeft]).toEqual([
        `${padding}px`,
        `${padding}px`,
        `${padding}px`,
        `${padding}px`,
      ]);
    }
  });

  it("composes an icon button and a text button with independent actions", async () => {
    const screen = render({
      components: { CompoundButton },
      data: () => ({ iconClicks: 0, textClicks: 0 }),
      template: `<div><CompoundButton icon-aria-label="复合图标" @icon-click="iconClicks += 1" @text-click="textClicks += 1"><template #icon><span>图标</span></template>文本</CompoundButton><output data-testid="compound-counts">{{ iconClicks }}:{{ textClicks }}</output></div>`,
    });

    const buttons = screen.getByRole("button");
    await buttons.nth(0).click();
    await buttons.nth(1).click();
    await expect.element(screen.getByTestId("compound-counts")).toHaveTextContent("1:1");
  });

  it("keeps icon and text in one native button for a single action", () => {
    render({
      components: { IconTextButton },
      template: `<div><IconTextButton data-testid="icon-text-horizontal"><template #icon><svg aria-hidden="true"><path d="M0 0h24v24H0z" /></svg></template>执行</IconTextButton><IconTextButton orientation="vertical" data-testid="icon-text-vertical"><template #icon><svg aria-hidden="true"><path d="M0 0h24v24H0z" /></svg></template>执行</IconTextButton></div>`,
    });

    expect(document.querySelectorAll("[data-testid='icon-text-horizontal'] button")).toHaveLength(0);
    expect(document.querySelectorAll("[data-testid='icon-text-vertical'] button")).toHaveLength(0);
    expect(document.querySelectorAll("button.nh-icon-text-button")).toHaveLength(2);
    expect(getComputedStyle(document.querySelector("[data-testid='icon-text-horizontal'] .nh-icon-text-button__content") as HTMLElement).flexDirection).toBe("row");
    expect(getComputedStyle(document.querySelector("[data-testid='icon-text-vertical'] .nh-icon-text-button__content") as HTMLElement).flexDirection).toBe("column");
    expect(getComputedStyle(document.querySelector("[data-testid='icon-text-horizontal'] svg") as SVGElement).width).toBe("20px");
    expect(getComputedStyle(document.querySelector("[data-testid='icon-text-vertical'] svg") as SVGElement).height).toBe("20px");
  });

  it("maps icon-text button icon size to the button size", () => {
    render({
      components: { IconTextButton },
      template: `<div><IconTextButton size="s" data-testid="icon-text-s"><template #icon><svg /></template>小</IconTextButton><IconTextButton size="l" data-testid="icon-text-l"><template #icon><svg /></template>大</IconTextButton></div>`,
    });

    expect(getComputedStyle(document.querySelector("[data-testid='icon-text-s'] svg") as SVGElement).width).toBe("18px");
    expect(getComputedStyle(document.querySelector("[data-testid='icon-text-l'] svg") as SVGElement).width).toBe("22px");
  });

  it("supports constrained button shapes", () => {
    render({
      components: { Button, IconButton },
      template: `<div><Button shape="rounded" data-testid="rounded">圆角</Button><Button shape="pill" data-testid="pill">胶囊</Button><IconButton shape="circle" aria-label="圆形" data-testid="circle"><span>+</span></IconButton></div>`,
    });

    expect(getComputedStyle(document.querySelector("[data-testid='rounded']") as HTMLElement).borderRadius).toBe("6px");
    expect(getComputedStyle(document.querySelector("[data-testid='pill']") as HTMLElement).borderRadius).toBe("999px");
    expect(getComputedStyle(document.querySelector("[data-testid='circle']") as HTMLElement).borderRadius).toBe("50%");
  });

  it("keeps button geometry stable instead of using a press jitter animation", () => {
    render(Button, { props: { variant: "primary" }, slots: { default: "执行" } });
    const button = document.querySelector("button.nh-button") as HTMLElement;
    const before = button.getBoundingClientRect();
    button.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    const after = button.getBoundingClientRect();

    expect(getComputedStyle(button).transform).toBe("none");
    expect(after.left).toBe(before.left);
    expect(after.top).toBe(before.top);
    expect(after.width).toBe(before.width);
    expect(after.height).toBe(before.height);
  });

  it("keeps compound button segments contiguous and supports vertical orientation", () => {
    render({
      components: { CompoundButton },
      template: `<CompoundButton orientation="vertical" icon-aria-label="垂直复合"><template #icon><span>图标</span></template>文本</CompoundButton>`,
    });

    const root = document.querySelector(".nh-compound-button") as HTMLElement;
    const icon = root.querySelector(".nh-compound-button__icon") as HTMLElement;
    const text = root.querySelector(".nh-compound-button__text") as HTMLElement;
    expect(getComputedStyle(root).flexDirection).toBe("column");
    expect(Math.abs(text.getBoundingClientRect().top - icon.getBoundingClientRect().bottom)).toBeLessThanOrEqual(1);
  });

  it("keeps horizontal compound segments aligned and contiguous", () => {
    render({
      components: { CompoundButton },
      template: `<CompoundButton icon-aria-label="水平复合"><template #icon><span>图标</span></template>文本</CompoundButton>`,
    });

    const root = document.querySelector(".nh-compound-button") as HTMLElement;
    const iconBox = (root.querySelector(".nh-compound-button__icon") as HTMLElement).getBoundingClientRect();
    const textBox = (root.querySelector(".nh-compound-button__text") as HTMLElement).getBoundingClientRect();
    expect(getComputedStyle(root).flexDirection).toBe("row");
    expect(iconBox.height).toBe(textBox.height);
    expect(Math.abs(textBox.left - iconBox.right)).toBeLessThanOrEqual(1);
  });

  it("keeps the native button contract and exposes loading semantics", async () => {
    const screen = render(Button, { props: { variant: "primary", loading: true } });
    const button = screen.getByRole("button");
    await expect.element(button).toHaveAttribute("type", "button");
    await expect.element(button).toBeDisabled();
    await expect.element(button).toHaveAttribute("aria-busy", "true");
    await expect.element(button).toHaveClass("nh-button--loading");
  });

  it("emits clicks for active buttons and blocks disabled buttons", async () => {
    const screen = render({
      components: { Button },
      data: () => ({ clicks: 0 }),
      template: `<div><Button @click="clicks += 1">执行</Button><Button disabled @click="clicks += 1">禁用</Button><output data-testid="clicks">{{ clicks }}</output></div>`,
    });

    await screen.getByRole("button", { name: "执行" }).click();
    await expect.element(screen.getByRole("button", { name: "禁用" })).toBeDisabled();
    await expect.element(screen.getByTestId("clicks")).toHaveTextContent("1");
  });
});
