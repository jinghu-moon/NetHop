import { territoryCodes, type TerritoryCode } from "./territories";

const modules = import.meta.glob<string>("../assets/flags/*.svg", {
  eager: true,
  query: "?url",
  import: "default",
});

const byCode = Object.fromEntries(Object.entries(modules).map(([path, url]) => {
  const filename = path.slice(path.lastIndexOf("/") + 1, -4);
  return [filename, url];
})) as Readonly<Record<string, string>>;

export const territoryFlagAssets = Object.freeze(Object.fromEntries(
  territoryCodes.map((code) => [code, byCode[code]]),
)) as Readonly<Record<TerritoryCode, string | undefined>>;
