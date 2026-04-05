import{a as e,i as t,n,r,t as i}from"./lit-CIqhFczO.js";(function(){let e=document.createElement(`link`).relList;if(e&&e.supports&&e.supports(`modulepreload`))return;for(let e of document.querySelectorAll(`link[rel="modulepreload"]`))n(e);new MutationObserver(e=>{for(let t of e)if(t.type===`childList`)for(let e of t.addedNodes)e.tagName===`LINK`&&e.rel===`modulepreload`&&n(e)}).observe(document,{childList:!0,subtree:!0});function t(e){let t={};return e.integrity&&(t.integrity=e.integrity),e.referrerPolicy&&(t.referrerPolicy=e.referrerPolicy),e.crossOrigin===`use-credentials`?t.credentials=`include`:e.crossOrigin===`anonymous`?t.credentials=`omit`:t.credentials=`same-origin`,t}function n(e){if(e.ep)return;e.ep=!0;let n=t(e);fetch(e.href,n)}})();function a(e,t,n,r){var i=arguments.length,a=i<3?t:r===null?r=Object.getOwnPropertyDescriptor(t,n):r,o;if(typeof Reflect==`object`&&typeof Reflect.decorate==`function`)a=Reflect.decorate(e,t,n,r);else for(var s=e.length-1;s>=0;s--)(o=e[s])&&(a=(i<3?o(a):i>3?o(t,n,a):o(t,n))||a);return i>3&&a&&Object.defineProperty(t,n,a),a}var o=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Home Page - Placeholder</div>`}};o=a([n(`home-page`)],o);var s=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Signin Page - Placeholder</div>`}};s=a([n(`signin-page`)],s);var c=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Register Page - Placeholder</div>`}};c=a([n(`register-page`)],c);var l=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Dashboard Page - Placeholder</div>`}};l=a([n(`dashboard-page`)],l);var u=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Devices Page - Placeholder</div>`}};u=a([n(`devices-page`)],u);var d=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Device Detail Page - Placeholder</div>`}};d=a([n(`device-detail-page`)],d);var f=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Alarms Page - Placeholder</div>`}};f=a([n(`alarms-page`)],f);var p=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Monitoring Page - Placeholder</div>`}};p=a([n(`monitoring-page`)],p);var m=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Settings Page - Placeholder</div>`}};m=a([n(`settings-page`)],m);var h=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Tags Page - Placeholder</div>`}};h=a([n(`tags-page`)],h);var g=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Templates Page - Placeholder</div>`}};g=a([n(`templates-page`)],g);var _=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Marketplace Page - Placeholder</div>`}};_=a([n(`marketplace-page`)],_);var v=class extends r{static{this.styles=e`
    :host {
      display: block;
      padding: 24px;
    }
  `}render(){return t`<div>Installed Marketplace Page - Placeholder</div>`}};v=a([n(`installed-marketplace-page`)],v);var y=(e,t)=>({path:e,render:()=>document.createElement(t)}),b=[y(`/`,`home-page`),y(`/signin`,`signin-page`),y(`/tenant/register`,`register-page`),y(`/dashboard`,`dashboard-page`),y(`/devices`,`devices-page`),y(`/device-detail/:id`,`device-detail-page`),y(`/alarms`,`alarms-page`),y(`/monitoring`,`monitoring-page`),y(`/settings`,`settings-page`),y(`/tags`,`tags-page`),y(`/templates`,`templates-page`),y(`/marketplace`,`marketplace-page`),y(`/installed-marketplace`,`installed-marketplace-page`)],x=null;function S(e){return x=new i(e,b),x}function C(){x=null}var w=class extends r{createRenderRoot(){return this}static{this.styles=e`
    :host {
      display: block;
      min-height: 100vh;
    }
    .app-shell {
      display: flex;
      min-height: 100vh;
    }
    .sidebar {
      width: 240px;
      background: var(--bg-secondary, #1a1a1a);
      flex-shrink: 0;
    }
    .main-content {
      flex: 1;
      display: flex;
      flex-direction: column;
      min-width: 0;
    }
    .topbar {
      height: 56px;
      background: var(--bg-primary, #0a0a0a);
      border-bottom: 1px solid var(--border-color, #2a2a2a);
      display: flex;
      align-items: center;
      padding: 0 24px;
      font-weight: 600;
    }
    .content {
      flex: 1;
      padding: 24px;
    }
  `}disconnectedCallback(){super.disconnectedCallback(),C()}render(){return t`
      <div class="app-shell">
        <div class="sidebar">Sidebar</div>
        <div class="main-content">
          <header class="topbar">TinyIoTHub</header>
          <main class="content"></main>
        </div>
      </div>
    `}};w=a([n(`tinyiothub-app`)],w);var T=document.getElementById(`app`);if(T){let e=new w;T.appendChild(e),S(e)}
//# sourceMappingURL=index-BYSSx-ZC.js.map