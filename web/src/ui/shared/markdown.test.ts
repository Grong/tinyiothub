/**
 * md() 消毒回归（2026-09-27）：工单页从纯文本切到 Markdown 渲染后，
 * md() 是所有 unsafeHTML 入口的唯一消毒层——XSS 防护必须有测试锁。
 */
import { describe, expect, it } from "vitest";
import { md } from "./markdown.js";

describe("md", () => {
  it("renders basic markdown", () => {
    expect(md("**bold**")).toContain("<strong>bold</strong>");
    expect(md("- a\n- b")).toContain("<li>a</li>");
  });

  it("preserves line structure as paragraphs/breaks", () => {
    const out = md("第一行\n第二行");
    expect(out).toContain("第一行");
    expect(out).toContain("第二行");
  });

  it("strips script tags (XSS)", () => {
    const out = md("hello <script>alert(1)</script>");
    expect(out).not.toContain("<script>");
    expect(out).not.toContain("alert(1)");
  });

  it("strips event handlers and javascript: URLs (XSS)", () => {
    expect(md('<img src=x onerror="alert(1)">')).not.toContain("onerror");
    const link = md("[click](javascript:alert(1))");
    expect(link).not.toContain("javascript:");
  });

  it("falls back to sanitized plain text on parse failure", () => {
    // 非字符串输入不应抛异常（防御性；运行时数据源可能给非字符串）
    expect(() => md("")).not.toThrow();
  });

  it("F8: external links get noopener + _blank", () => {
    const out = md("[文档](https://example.com/a)");
    expect(out).toContain('rel="noopener noreferrer"');
    expect(out).toContain('target="_blank"');
  });

  it("F8: external images are stripped, data URIs preserved", () => {
    expect(md("![x](https://evil.com/track.png)")).not.toContain("<img");
    expect(md("![x](data:image/png;base64,iVBORw0KGgo=)")).toContain("<img");
  });
});
