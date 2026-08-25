export type ToastTone = "info" | "success" | "warning" | "error" | "loading";
export type ToastPlacement = "top-center" | "top-start" | "top-end" | "bottom-center" | "bottom-start" | "bottom-end";

export interface ToastAction {
  readonly id: string;
  readonly label: string;
  readonly disabled?: boolean;
}

export interface ToastItem {
  readonly id: string;
  readonly tone: ToastTone;
  readonly message: string;
  readonly detail?: string;
  readonly duration?: number | null;
  readonly showProgress?: boolean;
  readonly persistent?: boolean;
  readonly pauseOnHover?: boolean;
  readonly closable?: boolean;
  readonly closeLabel?: string;
  readonly action?: ToastAction;
}
