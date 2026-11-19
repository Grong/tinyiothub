/**
 * Route-aware document metadata.
 *
 * The app is a single-page shell served by one index.html, so the static head
 * only describes the landing page. This module rewrites title/description/
 * canonical/Open Graph tags as the router moves between public pages, and
 * marks every authenticated console route `noindex` so the app never leaks
 * thin, duplicated URLs into search indexes.
 */

export interface RouteSeo {
  title: string;
  description: string;
  /** Value for `<meta name="robots">`. */
  robots: string;
  /** Canonical path when this route is an alias of another URL. */
  canonicalPath?: string;
}

export const SITE_NAME = 'TinyIoTHub';
export const SITE_URL = 'https://tinyiothub.com';

const HOME_SEO: RouteSeo = {
  title: 'TinyIoTHub · IoT Agent Harness — 给设备写 harness，让 AI 真正动手',
  description:
    'TinyIoTHub 是开源的 IoT agent harness：设备接入是底座，让常驻 Assistant 被设备事件和定时任务唤醒，查本体、做决策、过审批、执行动作并回读验证。Rust + SQLite 单进程，MIT 协议，支持私有化与 Docker 部署。',
  robots: 'index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1',
  canonicalPath: '/',
};

const PUBLIC_ROUTES: Record<string, RouteSeo> = {
  '': HOME_SEO,
  home: HOME_SEO,
  login: {
    title: `登录 · ${SITE_NAME}`,
    description: `登录 ${SITE_NAME} 控制台，管理物、告警、工单与 AI Assistant。`,
    robots: 'noindex,follow',
  },
  register: {
    title: `注册 · ${SITE_NAME}`,
    description: `创建 ${SITE_NAME} 账户，接入第一台设备并启动常驻 AI Assistant。`,
    robots: 'noindex,follow',
  },
  terms: {
    title: `服务条款 · ${SITE_NAME}`,
    description: `${SITE_NAME} 服务条款：服务说明、账户安全、用户行为规范、数据所有权与责任限制。`,
    robots: 'index,follow',
  },
  privacy: {
    title: `隐私政策 · ${SITE_NAME}`,
    description: `${SITE_NAME} 隐私政策：我们收集哪些信息、如何使用与保护个人信息及物数据，以及你享有的权利。`,
    robots: 'index,follow',
  },
};

const PRIVATE_SEO: RouteSeo = {
  title: `${SITE_NAME} 控制台`,
  description: `${SITE_NAME} 控制台：设备、告警、工单与 AI Assistant 的统一工作台，需要登录访问。`,
  robots: 'noindex,nofollow',
};

/** Normalize a pathname to the route key used by the router (`/` → `''`). */
export function normalizeRoutePath(pathname: string): string {
  return pathname.split(/[?#]/, 1)[0].replace(/^\/+|\/+$/g, '');
}

export function resolveSeo(pathname: string, pageTitle?: string): RouteSeo {
  const path = normalizeRoutePath(pathname);
  const publicRoute = PUBLIC_ROUTES[path];
  if (publicRoute) return publicRoute;
  if (pageTitle) return { ...PRIVATE_SEO, title: `${pageTitle} · ${SITE_NAME}` };
  return PRIVATE_SEO;
}

/** Rewrite the document head for the current route. Safe to call repeatedly. */
export function applyRouteSeo(pathname: string, pageTitle?: string): RouteSeo {
  const seo = resolveSeo(pathname, pageTitle);
  if (typeof document === 'undefined') return seo;

  const path = normalizeRoutePath(pathname);
  const canonical = `${siteOrigin()}${seo.canonicalPath ?? (path ? `/${path}` : '/')}`;
  const image = `${siteOrigin()}/og-image.png`;

  document.title = seo.title;
  setMeta('name', 'description', seo.description);
  setMeta('name', 'robots', seo.robots);
  setLink('canonical', canonical);
  setMeta('property', 'og:title', seo.title);
  setMeta('property', 'og:description', seo.description);
  setMeta('property', 'og:url', canonical);
  setMeta('property', 'og:image', image);
  setMeta('name', 'twitter:title', seo.title);
  setMeta('name', 'twitter:description', seo.description);
  setMeta('name', 'twitter:image', image);

  return seo;
}

function siteOrigin(): string {
  const origin = typeof location === 'undefined' ? '' : location.origin;
  return origin && origin !== 'null' ? origin : SITE_URL;
}

function setMeta(attribute: 'name' | 'property', key: string, content: string): void {
  let el = document.head.querySelector<HTMLMetaElement>(`meta[${attribute}="${key}"]`);
  if (!el) {
    el = document.createElement('meta');
    el.setAttribute(attribute, key);
    document.head.appendChild(el);
  }
  el.setAttribute('content', content);
}

function setLink(rel: string, href: string): void {
  let el = document.head.querySelector<HTMLLinkElement>(`link[rel="${rel}"]`);
  if (!el) {
    el = document.createElement('link');
    el.setAttribute('rel', rel);
    document.head.appendChild(el);
  }
  el.setAttribute('href', href);
}
