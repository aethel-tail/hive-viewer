import { ref } from "vue";
import { zhCN } from "./zh-CN";
import { zhTW } from "./zh-TW";
import { en } from "./en";
import { ja } from "./ja";

export type Locale = "zh-CN" | "zh-TW" | "en" | "ja";
export type MessageKey = keyof typeof zhCN;

const messages: Record<Locale, Record<string, string>> = { "zh-CN": zhCN, "zh-TW": zhTW, en, ja };

export const locale = ref<Locale>("zh-CN");

export function setLocale(l: string) {
  if (l === "zh") {
    l = "zh-CN";
  } // 兼容旧版持久化值
  if (l === "zh-CN" || l === "zh-TW" || l === "en" || l === "ja") {
    locale.value = l;
  }
}

// 查找失败回退到 zh-CN，再回退到 key 本身；{name} 形式的占位符用 vars 替换
export function t(key: MessageKey, vars?: Record<string, string | number>): string {
  const s = messages[locale.value][key] ?? zhCN[key] ?? key;
  return vars ? s.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? "")) : s;
}

// 判断某语言下是否存在该 key（用于 EXIF 这类「无翻译则显示原 tag 名」的场景）
export function te(key: string): boolean {
  return key in messages[locale.value] || key in zhCN;
}
