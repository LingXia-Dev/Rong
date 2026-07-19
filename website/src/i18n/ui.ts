// Locales and UI strings shared by the layout chrome and module pages.
// The default locale (en) lives at the site root; zh is served under /zh/.

export type Locale = "en" | "zh";
export const LOCALES: Locale[] = ["en", "zh"];
export const DEFAULT_LOCALE: Locale = "en";

/** BCP 47 tag for the <html lang> attribute and hreflang links. */
export function htmlLang(lang: Locale): string {
  return lang === "zh" ? "zh-CN" : "en";
}

/**
 * Given the current pathname and the deploy base (e.g. "/Rong"), return the
 * equivalent URL for every locale, so the switcher keeps you on the same page.
 */
export function localeUrls(pathname: string, base: string): Record<Locale, string> {
  let rel = pathname.startsWith(base) ? pathname.slice(base.length) : pathname;
  if (!rel.startsWith("/")) rel = `/${rel}`;
  if (rel === "/zh" || rel.startsWith("/zh/")) rel = rel.slice(3) || "/";
  return {
    en: `${base}${rel}`,
    zh: `${base}/zh${rel === "/" ? "/" : rel}`,
  };
}

/** Locale-aware prefix for site-internal links ("/Rong" or "/Rong/zh"). */
export function homeBase(lang: Locale, base: string): string {
  return lang === "zh" ? `${base}/zh` : base;
}

export const LAYOUT = {
  en: {
    skipToContent: "Skip to content",
    title: "Rong (融) — JavaScript Runtime for Rust",
    description:
      "Rong is a JavaScript runtime for Rust with a unified API over multiple engines — QuickJS, JavaScriptCore, and ArkJS. Built for embedding, Rust-driven JS APIs, and long-lived worker runtimes.",
    nav: {
      features: "Features",
      engines: "Engines",
      architecture: "Architecture",
      modules: "Modules",
      skills: "Agent Skills",
      start: "Quick Start",
    },
    footer: {
      tagline: "Fusing JavaScript engines with Rust, creating harmony in diversity.",
      project: "Project",
      repository: "Repository",
      contributing: "Contributing",
      changelog: "Changelog",
      docs: "Docs",
      moduleDev: "Module Development",
      workerModel: "Worker Model",
      valueSystem: "Value System",
      errorHandling: "Error Handling",
      jsTesting: "JavaScript Testing",
      packages: "Packages",
      cratesIo: "rong on crates.io",
      license: "License",
      copyright: "© 2026 Rong. Licensed under MIT or Apache-2.0.",
      builtWith: "Built with Astro.",
    },
  },
  zh: {
    skipToContent: "跳到主要内容",
    title: "Rong（融）— Rust 的 JavaScript 运行时",
    description:
      "Rong 是一个面向 Rust 的 JavaScript 运行时，以统一的 API 覆盖多种引擎 —— QuickJS、JavaScriptCore 和 ArkJS。专为嵌入式场景、Rust 驱动的 JS API 以及长生命周期的 worker 运行时而设计。",
    nav: {
      features: "特性",
      engines: "引擎",
      architecture: "架构",
      modules: "模块",
      skills: "智能体技能",
      start: "快速上手",
    },
    footer: {
      tagline: "融汇 JavaScript 引擎与 Rust，于多样中见和谐。",
      project: "项目",
      repository: "代码仓库",
      contributing: "参与贡献",
      changelog: "更新日志",
      docs: "文档",
      moduleDev: "模块开发",
      workerModel: "Worker 模型",
      valueSystem: "值系统",
      errorHandling: "错误处理",
      jsTesting: "JavaScript 测试",
      packages: "软件包",
      cratesIo: "crates.io 上的 rong",
      license: "许可证",
      copyright: "© 2026 Rong。基于 MIT 或 Apache-2.0 许可发布。",
      builtWith: "由 Astro 构建。",
    },
  },
} as const;

export const MODULE_PAGE = {
  en: {
    title: (slug: string) => `${slug} · Rong module API`,
    description: (slug: string, desc: string) =>
      `API reference for the ${slug} built-in module of the Rong JavaScript runtime: ${desc}.`,
    allModules: "← All modules",
    home: "Home",
    modules: "Modules",
    source: "Source",
    cratesIo: "crates.io",
    editPage: "Edit this page",
    englishOnlyNote: "",
  },
  zh: {
    title: (slug: string) => `${slug} · Rong 模块 API`,
    description: (slug: string, desc: string) =>
      `Rong JavaScript 运行时内置模块 ${slug} 的 API 参考：${desc}。`,
    allModules: "← 全部模块",
    home: "首页",
    modules: "模块",
    source: "源码",
    cratesIo: "crates.io",
    editPage: "编辑此页",
    englishOnlyNote: "本页 API 参考由仓库内的英文文档生成，暂仅提供英文版。",
  },
} as const;
