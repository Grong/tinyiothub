import { describe, expect, it } from 'vitest';
import { applyRouteSeo, normalizeRoutePath, resolveSeo } from './seo.js';

describe('normalizeRoutePath', () => {
  it('normalizes slash, query and hash forms', () => {
    expect(normalizeRoutePath('/')).toBe('');
    expect(normalizeRoutePath('/terms')).toBe('terms');
    expect(normalizeRoutePath('/terms/')).toBe('terms');
    expect(normalizeRoutePath('/terms?from=footer')).toBe('terms');
    expect(normalizeRoutePath('/terms#top')).toBe('terms');
  });
});

describe('resolveSeo', () => {
  it('indexes the landing page', () => {
    const seo = resolveSeo('/');
    expect(seo.title).toContain('TinyIoTHub');
    expect(seo.title).toContain('IoT Agent Harness');
    expect(seo.robots).toContain('index');
    expect(seo.robots).not.toContain('noindex');
  });

  it('treats /home as an alias of the landing page', () => {
    expect(resolveSeo('/home').title).toBe(resolveSeo('/').title);
  });

  it('keeps login and register out of the index', () => {
    expect(resolveSeo('/login').robots).toContain('noindex');
    expect(resolveSeo('/register').robots).toContain('noindex');
  });

  it('indexes the legal pages with route-specific titles', () => {
    expect(resolveSeo('/terms').title).toContain('服务条款');
    expect(resolveSeo('/privacy').title).toContain('隐私政策');
    expect(resolveSeo('/terms').robots).toContain('index');
  });

  it('keeps every console route noindex and accepts a page title', () => {
    expect(resolveSeo('/dashboard').robots).toContain('noindex');
    expect(resolveSeo('/things/dev-1').robots).toContain('noindex');
    expect(resolveSeo('/dashboard', '概览').title).toBe('概览 · TinyIoTHub');
  });
});

describe('applyRouteSeo', () => {
  it('writes title, description, canonical and social metadata', () => {
    window.history.pushState({}, '', '/terms');
    const seo = applyRouteSeo('/terms');

    expect(document.title).toBe(seo.title);
    expect(
      document.head.querySelector<HTMLMetaElement>('meta[name="description"]')?.content,
    ).toBe(seo.description);
    expect(document.head.querySelector<HTMLMetaElement>('meta[name="robots"]')?.content).toBe(
      seo.robots,
    );
    expect(document.head.querySelector<HTMLLinkElement>('link[rel="canonical"]')?.href).toBe(
      `${window.location.origin}/terms`,
    );
    expect(document.head.querySelector<HTMLMetaElement>('meta[property="og:url"]')?.content).toBe(
      `${window.location.origin}/terms`,
    );
    expect(
      document.head.querySelector<HTMLMetaElement>('meta[name="twitter:title"]')?.content,
    ).toBe(seo.title);
  });

  it('canonicalizes the /home alias to the root URL', () => {
    window.history.pushState({}, '', '/home');
    applyRouteSeo('/home');
    expect(document.head.querySelector<HTMLLinkElement>('link[rel="canonical"]')?.href).toBe(
      `${window.location.origin}/`,
    );
  });
});
