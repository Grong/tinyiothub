/**
 * E3 模板=场景包超集（spec 2026-10-21-e3 §5）：
 * watch_rules/composites 引用解析校验与整包应用。
 */

import { apiPost } from './client.js';

export interface TemplateReferenceIssue {
  path: string;
  refId: string | null;
  /** missing_device | unresolved | invalid */
  status: string;
  message: string;
}

export interface TemplateConflict {
  kind: string;
  refId: string;
  message: string;
}

export interface TemplateRulesReport {
  ok: boolean;
  references: TemplateReferenceIssue[];
  conflicts: TemplateConflict[];
  resolved: { watchRules: number; composites: number };
}

export interface TemplateRulesResult {
  applied: boolean;
  report: TemplateRulesReport;
}

function unwrap<T>(res: { result: T | null; msg?: string; code: number }): T {
  if (res.result === null) throw new Error(res.msg || '空响应');
  return res.result;
}

export const templateRulesApi = {
  async validate(template: unknown, mapping: Record<string, string>): Promise<TemplateRulesReport> {
    return unwrap(
      await apiPost<TemplateRulesReport>('/marketplace/template-rules/validate', {
        template,
        mapping,
      }),
    );
  },

  async apply(template: unknown, mapping: Record<string, string>): Promise<TemplateRulesResult> {
    return unwrap(
      await apiPost<TemplateRulesResult>('/marketplace/template-rules/apply', {
        template,
        mapping,
      }),
    );
  },
};
