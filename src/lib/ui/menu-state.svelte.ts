/**
 * One app-wide M3 menu. Components describe items and an anchor point;
 * <ContextMenu /> (mounted once in App) renders, positions and dismisses it.
 */
export interface MenuItem {
  label: string;
  icon?: string;
  /** Destructive action, shown in the error color. */
  danger?: boolean;
  disabled?: boolean;
  /** Secondary text shown at the trailing edge (e.g. a shortcut). */
  hint?: string;
  onselect: () => void;
}

export type MenuEntry =
  | MenuItem
  | "divider"
  | { reactions: string[]; selected?: string; onreact: (emoji: string) => void };

interface MenuRequest {
  x: number;
  y: number;
  entries: MenuEntry[];
  /** Element to return focus to when the menu closes. */
  returnFocus?: HTMLElement | null;
}

class MenuState {
  current = $state<MenuRequest | null>(null);

  open(x: number, y: number, entries: MenuEntry[], returnFocus?: HTMLElement | null) {
    this.current = { x, y, entries, returnFocus: returnFocus ?? null };
  }

  /** Open below (or above, if needed) an element, aligned to its trailing edge. */
  openAt(anchor: HTMLElement, entries: MenuEntry[]) {
    const rect = anchor.getBoundingClientRect();
    this.open(rect.right, rect.bottom + 4, entries, anchor);
  }

  close() {
    const focus = this.current?.returnFocus;
    this.current = null;
    focus?.focus?.();
  }
}

export const menu = new MenuState();
