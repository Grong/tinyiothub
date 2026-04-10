/**
 * Focus trap utility for modal dialogs.
 * Traps keyboard focus within a container when active.
 */

type FocusTrapOptions = {
  /** Set to false to release the trap. Defaults to true. */
  active?: boolean;
  /** CSS selector for focusable elements. Defaults to common focusables. */
  focusableSelector?: string;
};

const DEFAULT_FOCUSABLE = [
  'a[href]',
  'button:not([disabled])',
  'textarea:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

let activeTrap: { release: () => void } | null = null;

/**
 * Activate a focus trap on the given element.
 * Returns a release function — call it to remove the trap.
 */
export function activateFocusTrap(el: HTMLElement, options: FocusTrapOptions = {}): () => void {
  const { focusableSelector = DEFAULT_FOCUSABLE } = options;

  const getFocusable = (): HTMLElement[] =>
    Array.from(el.querySelectorAll<HTMLElement>(focusableSelector)).filter(
      (e) => !e.hasAttribute('disabled') && e.offsetParent !== null,
    );

  // Release any existing trap
  if (activeTrap) {
    activeTrap.release();
  }

  // Store the previously focused element
  const previousFocus = document.activeElement as HTMLElement | null;

  // Focus the first focusable element
  const focusable = getFocusable();
  if (focusable.length > 0) {
    focusable[0].focus();
  } else {
    el.focus();
  }

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key !== 'Tab') return;

    const focusableEls = getFocusable();
    if (focusableEls.length === 0) return;

    const first = focusableEls[0];
    const last = focusableEls[focusableEls.length - 1];

    if (e.shiftKey) {
      // Shift+Tab: if on first, jump to last
      if (document.activeElement === first) {
        e.preventDefault();
        last.focus();
      }
    } else {
      // Tab: if on last, jump to first
      if (document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  };

  el.addEventListener('keydown', handleKeyDown);

  const release = () => {
    el.removeEventListener('keydown', handleKeyDown);
    activeTrap = null;
    // Restore focus
    if (previousFocus && previousFocus.focus) {
      previousFocus.focus();
    }
  };

  activeTrap = { release };
  return release;
}
