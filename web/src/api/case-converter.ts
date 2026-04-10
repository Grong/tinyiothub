/**
 * 命名格式转换工具
 * 在 API 层自动处理 snake_case 和 camelCase 的转换
 */

// 将 snake_case 转换为 camelCase
export function toCamelCase(str: string): string {
  return str.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
}

// 将 camelCase 转换为 snake_case
export function toSnakeCase(str: string): string {
  return str.replace(/[A-Z]/g, letter => `_${letter.toLowerCase()}`);
}

// 递归转换对象的所有键名为 camelCase
export function keysToCamelCase<T = any>(obj: any): T {
  if (obj === null || obj === undefined) {
    return obj;
  }

  if (Array.isArray(obj)) {
    return obj.map(keysToCamelCase) as T;
  }

  if (typeof obj === 'object' && obj.constructor === Object) {
    const converted: any = {};
    for (const [key, value] of Object.entries(obj)) {
      converted[toCamelCase(key)] = keysToCamelCase(value);
    }
    return converted as T;
  }

  return obj;
}

// 递归转换对象的所有键名为 snake_case
export function keysToSnakeCase<T = any>(obj: any): T {
  if (obj === null || obj === undefined) {
    return obj;
  }

  if (Array.isArray(obj)) {
    return obj.map(keysToSnakeCase) as T;
  }

  if (typeof obj === 'object' && obj.constructor === Object) {
    const converted: any = {};
    for (const [key, value] of Object.entries(obj)) {
      converted[toSnakeCase(key)] = keysToSnakeCase(value);
    }
    return converted as T;
  }

  return obj;
}

// 类型转换辅助
export type KeysToCamelCase<T> = T extends Array<infer U>
  ? Array<KeysToCamelCase<U>>
  : T extends object
    ? { [K in keyof T as K extends string ? (K extends `${infer P}_${infer S}` ? `${P}${Capitalize<S>}` : K) : K]: KeysToCamelCase<T[K]> }
    : T;
