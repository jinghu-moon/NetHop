export const MINIMUM_WEBVIEW_MAJOR = 105;

export function isSupportedWebViewMajor(major: number): boolean {
  return Number.isInteger(major) && major >= MINIMUM_WEBVIEW_MAJOR;
}
