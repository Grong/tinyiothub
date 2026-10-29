export interface DialogFocusTrapOptions {
  /** Escape always closes the dialog, even when focus escaped to `body`. */
  onEscape(): void;
  /** Enabled, visible controls that should participate in the Tab cycle. */
  focusables(): HTMLElement[];
  /** Element that receives focus when it escapes the dialog. */
  primary(): HTMLElement | null;
}

export interface DialogFocusTrap {
  attach(): void;
  detach(): void;
}

/**
 * Document-level focus trap shared by Runtime dialogs.
 *
 * Listeners use capture so Tab, Escape and stray focus are handled before the
 * obscured background can react. The host must be the dialog element itself so
 * `contains` never accepts background controls.
 */
export function createDialogFocusTrap(
  host: HTMLElement,
  options: DialogFocusTrapOptions,
): DialogFocusTrap {
  const onKeydown = (event: KeyboardEvent): void => {
    if (!host.isConnected) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      options.onEscape();
      return;
    }
    if (event.key !== 'Tab') return;
    const focusables = options.focusables();
    if (!focusables.length) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement;
    if (event.shiftKey) {
      if (active === first || !(active instanceof Node) || !host.contains(active)) {
        event.preventDefault();
        last.focus();
      }
      return;
    }
    if (active === last || !(active instanceof Node) || !host.contains(active)) {
      event.preventDefault();
      first.focus();
    }
  };

  const onFocusIn = (event: FocusEvent): void => {
    if (!host.isConnected) return;
    const target = event.target;
    if (target instanceof Node && host.contains(target)) return;
    options.primary()?.focus();
  };

  const attach = (): void => {
    document.addEventListener('keydown', onKeydown, true);
    document.addEventListener('focusin', onFocusIn, true);
  };
  const detach = (): void => {
    document.removeEventListener('keydown', onKeydown, true);
    document.removeEventListener('focusin', onFocusIn, true);
  };

  return { attach, detach };
}
