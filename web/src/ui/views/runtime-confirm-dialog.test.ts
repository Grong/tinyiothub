import { afterEach, describe, expect, it, vi } from 'vitest';
import './runtime-confirm-dialog.js';
import type { RuntimeConfirmDialog } from './runtime-confirm-dialog.js';

const mounted: RuntimeConfirmDialog[] = [];

async function mount(): Promise<RuntimeConfirmDialog> {
  const dialog = document.createElement('runtime-confirm-dialog') as RuntimeConfirmDialog;
  dialog.title = 'Cancel task';
  dialog.body = 'Objective: Inspect pumps';
  dialog.primaryLabel = 'Keep running';
  dialog.dangerLabel = 'Cancel task';
  document.body.append(dialog);
  mounted.push(dialog);
  await dialog.updateComplete;
  return dialog;
}

function primary(dialog: RuntimeConfirmDialog): HTMLButtonElement {
  return dialog.querySelector<HTMLButtonElement>('button[data-action="primary"]')!;
}

function danger(dialog: RuntimeConfirmDialog): HTMLButtonElement {
  return dialog.querySelector<HTMLButtonElement>('button[data-action="danger"]')!;
}

afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
});

describe('runtime-confirm-dialog', () => {
  it('renders a named modal dialog with the safe primary action focused', async () => {
    const dialog = await mount();
    const surface = dialog.querySelector<HTMLElement>('[role="dialog"]')!;

    expect(surface.getAttribute('aria-modal')).toBe('true');
    expect(surface.getAttribute('aria-labelledby')).toBeTruthy();
    expect(dialog.textContent).toContain('Cancel task');
    expect(dialog.textContent).toContain('Objective: Inspect pumps');
    expect(primary(dialog).textContent).toContain('Keep running');
    expect(danger(dialog).textContent).toContain('Cancel task');
    expect(document.activeElement).toBe(primary(dialog));
  });

  it('keeps Tab and Shift+Tab inside the dialog', async () => {
    const dialog = await mount();

    danger(dialog).focus();
    danger(dialog).dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }));
    expect(document.activeElement).toBe(primary(dialog));

    primary(dialog).dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true }));
    expect(document.activeElement).toBe(danger(dialog));
  });

  it('closes with Escape without submitting the danger action', async () => {
    const dialog = await mount();
    const primaryEvent = vi.fn();
    const dangerEvent = vi.fn();
    dialog.addEventListener('confirm-primary', primaryEvent);
    dialog.addEventListener('confirm-danger', dangerEvent);

    primary(dialog).dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));

    expect(primaryEvent).toHaveBeenCalledOnce();
    expect(dangerEvent).not.toHaveBeenCalled();
  });

  it('submits only the body-selected action and blocks repeats while busy', async () => {
    const dialog = await mount();
    const primaryEvent = vi.fn();
    const dangerEvent = vi.fn();
    dialog.addEventListener('confirm-primary', primaryEvent);
    dialog.addEventListener('confirm-danger', dangerEvent);

    primary(dialog).click();
    expect(primaryEvent).toHaveBeenCalledOnce();
    expect(dangerEvent).not.toHaveBeenCalled();

    dialog.busy = true;
    await dialog.updateComplete;
    danger(dialog).click();
    expect(dangerEvent).not.toHaveBeenCalled();
    expect(danger(dialog).disabled).toBe(true);
  });

  it('emits the danger action only when the danger button is chosen', async () => {
    const dialog = await mount();
    const dangerEvent = vi.fn();
    dialog.addEventListener('confirm-danger', dangerEvent);

    danger(dialog).click();

    expect(dangerEvent).toHaveBeenCalledOnce();
  });
});
