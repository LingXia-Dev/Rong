import type { Locale } from "./ui";

// All translatable copy for the landing page. Code samples stay in
// Home.astro — they are shared verbatim across locales.

type Feature = { icon: string; title: string; body: string };
type Engine = { name: string; tag: string; body: string; accent: string };

const FEATURE_ICONS = [
  "M4 7h16M4 12h16M4 17h10",
  "M12 2l3 3-3 3-3-3 3-3zM12 16l3 3-3 3-3-3 3-3zM2 12l3-3 3 3-3 3-3-3zM16 12l3-3 3 3-3 3-3-3z",
  "M13 2L3 14h7l-1 8 10-12h-7l1-8z",
  "M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z",
  "M12 2a10 10 0 100 20 10 10 0 000-20zM2 12h20",
  "M12 5v14M5 12h14",
];

const ENGINE_ACCENTS = ["var(--jade)", "var(--celadon)", "var(--gold)"];

export interface HomeStrings {
  hero: {
    badge: string;
    title1: string;
    title2: string;
    subHtml: string;
    getStarted: string;
    viewOnGitHub: string;
    enginesLabel: string;
  };
  stats: [string, string][];
  featuresHead: { eyebrow: string; title: string; body: string };
  features: Feature[];
  enginesHead: { eyebrow: string; title: string; body: string };
  engines: Engine[];
  archHead: { eyebrow: string; title: string; body: string };
  arch: { core: string; services: string[]; modulesTitle: string };
  startHead: { eyebrow: string; title: string; body: string };
  codeTabs: string[];
  modulesHead: { eyebrow: string; title: string; body: string };
  ecoHead: { eyebrow: string; title: string; body: string };
  eco: { crate: string; types: string; skill: string; cli: string };
  skillsHead: { eyebrow: string; title: string; bodyHtml: string };
  skills: { runtimeDev: string; moduleAuthor: string; noteHtml: string };
  cta: { title: string; body: string; start: string; moduleGuide: string };
}

export const HOME: Record<Locale, HomeStrings> = {
  en: {
    hero: {
      badge: "See what's new",
      title1: "One JavaScript API.",
      title2: "Every engine, in Rust.",
      subHtml:
        "<strong>Rong</strong> (融) is a JavaScript runtime for Rust with a unified API over QuickJS, JavaScriptCore, and ArkJS — built for embedding, Rust-driven JS APIs, and long-lived worker runtimes.",
      getStarted: "Get started",
      viewOnGitHub: "View on GitHub",
      enginesLabel: "Supported engines",
    },
    stats: [
      ["3", "JavaScript engines"],
      ["20", "built-in modules"],
      ["1.90+", "Rust toolchain (2024 edition)"],
      ["MIT / Apache-2.0", "dual licensed"],
    ],
    featuresHead: {
      eyebrow: "Why Rong",
      title: "Fusion, harmony, and flow — by design.",
      body: "In Chinese, 融 means to merge and harmonize. Rong fuses JavaScript engines with Rust native code, unifying diverse runtimes under a single, elegant API.",
    },
    features: [
      {
        icon: FEATURE_ICONS[0],
        title: "Unified API",
        body: "Write once, run anywhere. The same Rust code drives QuickJS, JavaScriptCore, and ArkJS — engines are selected at build time, with no engine-specific branches in your code.",
      },
      {
        icon: FEATURE_ICONS[1],
        title: "Declarative class bindings",
        body: "Expose Rust structs to JavaScript with #[js_class] and #[js_method] — constructors, getters, setters, and static methods, all type-checked by Rust.",
      },
      {
        icon: FEATURE_ICONS[2],
        title: "Async / await",
        body: "First-class Promise and async iterator integration so JavaScript and Rust futures interleave naturally across the engine boundary.",
      },
      {
        icon: FEATURE_ICONS[3],
        title: "Worker pools",
        body: "Choose your execution model explicitly: shared() workers for stateless work, pinned() workers for keyed state that must live on the same long-lived runtime.",
      },
      {
        icon: FEATURE_ICONS[4],
        title: "Cross-platform CI",
        body: "Continuously tested on Windows, Linux, and macOS — QuickJS everywhere, system JavaScriptCore on macOS, and source-built JSC consumers on all three.",
      },
      {
        icon: FEATURE_ICONS[5],
        title: "TypeScript & tooling",
        body: "Type definitions ship as @rongjs/rong on npm, and @rongjs/rong-skill packages an installable agent skill with generated API references.",
      },
    ],
    enginesHead: {
      eyebrow: "Multi-engine support",
      title: "Three engines, one codebase.",
      body: "Engines are mutually exclusive and chosen at build time — if multiple engines are enabled, the build fails fast. The library ships no default; downstream crates select an engine and TLS backend explicitly.",
    },
    engines: [
      {
        name: "QuickJS",
        tag: "Default · Desktop",
        body: "Lightweight and fast. The default engine for the Rong CLI on desktop hosts, paired with the aws-lc TLS backend.",
        accent: ENGINE_ACCENTS[0],
      },
      {
        name: "JavaScriptCore",
        tag: "Apple system + source builds",
        body: "Links the system JavaScriptCore.framework on macOS and iOS, or pinned source-built WebKit/JSCOnly artifacts on macOS, Linux, and Windows.",
        accent: ENGINE_ACCENTS[1],
      },
      {
        name: "ArkJS",
        tag: "HarmonyOS / OpenHarmony",
        body: "The HarmonyOS JavaScript engine, for aarch64 OpenHarmony targets with the ring TLS backend.",
        accent: ENGINE_ACCENTS[2],
      },
    ],
    archHead: {
      eyebrow: "Architecture",
      title: "A unified core over swappable engines.",
      body: "The Rong core provides the unified API, type system, memory management, and async layer. Engines and built-in modules plug in beneath it.",
    },
    arch: {
      core: "Rong Core",
      services: ["Unified API", "Type System", "Memory Management", "Async / Await"],
      modulesTitle: "Built-in Modules & Extensions",
    },
    startHead: {
      eyebrow: "Quick start",
      title: "From zero to evaluating JS in seconds.",
      body: "Add the dependency, pick an engine, and run JavaScript from Rust — or expose Rust classes to JavaScript.",
    },
    codeTabs: ["Embed & eval", "Worker pool", "Class bindings", "Cargo.toml", "CLI"],
    modulesHead: {
      eyebrow: "Batteries included",
      title: "Twenty built-in modules.",
      body: "Common runtime tasks ship in the box — timers, HTTP, file system, storage, workers, Redis, SQLite, S3, and more. Click a module to read its API reference.",
    },
    ecoHead: {
      eyebrow: "Ecosystem",
      title: "Beyond the crate.",
      body: "Rong ships to crates.io and npm, with tooling for TypeScript users and AI agents.",
    },
    eco: {
      crate:
        "The runtime itself, plus per-module crates published in dependency order from a single release workflow.",
      types:
        "TypeScript type definitions for the Rong runtime, so JS authored for Rong gets full editor support.",
      skill:
        "An installable agent skill with self-contained docs and generated API references for AI coding agents.",
      cli: "Local runtime execution and REPL workflows, with engine selection via Cargo features.",
    },
    skillsHead: {
      eyebrow: "Agent skills",
      title: "Teach your AI agent Rong.",
      bodyHtml:
        '<code class="inline-code">@rongjs/rong-skill</code> bundles two installable agent skills — self-contained <code class="inline-code">SKILL.md</code> documents with generated API references, for any agent runtime that supports file-based skills.',
    },
    skills: {
      runtimeDev:
        'Write Rong JavaScript scripts, choose the right public APIs, adapt examples, run <code class="inline-code">rong_cli</code>, and compile bytecode.',
      moduleAuthor:
        "Write or edit Rust modules that expose Rong APIs, classes, functions, type conversions, and JavaScript errors.",
      noteHtml:
        'Use <code class="inline-code">--project</code> for a project-local install, or <code class="inline-code">--skill &lt;name&gt;</code> to install just one. The skills share their source with the module API docs on this site — one source of truth.',
    },
    cta: {
      title: "Bring JavaScript into your Rust application.",
      body: "Embed a runtime, expose Rust-driven APIs, and scale with worker pools — across every supported engine.",
      start: "Start building",
      moduleGuide: "Module guide",
    },
  },
  zh: {
    hero: {
      badge: "查看更新内容",
      title1: "一套 JavaScript API。",
      title2: "贯通所有引擎，尽在 Rust。",
      subHtml:
        "<strong>Rong</strong>（融）是一个面向 Rust 的 JavaScript 运行时，以统一的 API 覆盖 QuickJS、JavaScriptCore 和 ArkJS —— 专为嵌入式场景、Rust 驱动的 JS API 以及长生命周期的 worker 运行时而设计。",
      getStarted: "快速开始",
      viewOnGitHub: "在 GitHub 上查看",
      enginesLabel: "支持的引擎",
    },
    stats: [
      ["3", "种 JavaScript 引擎"],
      ["20", "个内置模块"],
      ["1.90+", "Rust 工具链（2024 edition）"],
      ["MIT / Apache-2.0", "双重许可"],
    ],
    featuresHead: {
      eyebrow: "为什么选择 Rong",
      title: "融合、和谐、流动 —— 源于设计。",
      body: "「融」意为交融与和谐。Rong 将 JavaScript 引擎与 Rust 原生代码融为一体，以单一而优雅的 API 统一各式运行时。",
    },
    features: [
      {
        icon: FEATURE_ICONS[0],
        title: "统一 API",
        body: "一次编写，到处运行。同一份 Rust 代码即可驱动 QuickJS、JavaScriptCore 和 ArkJS —— 引擎在构建时选定，代码中没有任何引擎相关的分支。",
      },
      {
        icon: FEATURE_ICONS[1],
        title: "声明式类绑定",
        body: "使用 #[js_class] 和 #[js_method] 将 Rust 结构体暴露给 JavaScript —— 构造函数、getter、setter 与静态方法，全部经过 Rust 类型检查。",
      },
      {
        icon: FEATURE_ICONS[2],
        title: "Async / await",
        body: "一流的 Promise 与异步迭代器集成，让 JavaScript 与 Rust 的 Future 跨越引擎边界自然交织。",
      },
      {
        icon: FEATURE_ICONS[3],
        title: "Worker 池",
        body: "显式选择执行模型：shared() worker 处理无状态任务，pinned() worker 让按键关联的状态始终驻留在同一个长生命周期运行时上。",
      },
      {
        icon: FEATURE_ICONS[4],
        title: "跨平台 CI",
        body: "在 Windows、Linux 和 macOS 上持续测试 —— QuickJS 覆盖全平台，macOS 上使用系统 JavaScriptCore，三大平台均有源码构建的 JSC 消费者。",
      },
      {
        icon: FEATURE_ICONS[5],
        title: "TypeScript 与工具链",
        body: "类型定义以 @rongjs/rong 发布到 npm；@rongjs/rong-skill 则打包了带生成式 API 参考的可安装智能体技能。",
      },
    ],
    enginesHead: {
      eyebrow: "多引擎支持",
      title: "三个引擎，一套代码。",
      body: "引擎彼此互斥，在构建时选定 —— 若同时启用多个引擎，构建会立即失败。库本身不预设默认引擎；由下游 crate 显式选择引擎与 TLS 后端。",
    },
    engines: [
      {
        name: "QuickJS",
        tag: "默认 · 桌面端",
        body: "轻量且快速。Rong CLI 在桌面主机上的默认引擎，搭配 aws-lc TLS 后端。",
        accent: ENGINE_ACCENTS[0],
      },
      {
        name: "JavaScriptCore",
        tag: "Apple 系统 + 源码构建",
        body: "在 macOS 和 iOS 上链接系统 JavaScriptCore.framework，或在 macOS、Linux、Windows 上使用固定版本、源码构建的 WebKit/JSCOnly 产物。",
        accent: ENGINE_ACCENTS[1],
      },
      {
        name: "ArkJS",
        tag: "HarmonyOS / OpenHarmony",
        body: "HarmonyOS 的 JavaScript 引擎，面向 aarch64 OpenHarmony 目标，搭配 ring TLS 后端。",
        accent: ENGINE_ACCENTS[2],
      },
    ],
    archHead: {
      eyebrow: "架构",
      title: "统一内核，引擎可换。",
      body: "Rong 内核提供统一 API、类型系统、内存管理与异步层，引擎和内置模块在其下方接入。",
    },
    arch: {
      core: "Rong 内核",
      services: ["统一 API", "类型系统", "内存管理", "Async / Await"],
      modulesTitle: "内置模块与扩展",
    },
    startHead: {
      eyebrow: "快速上手",
      title: "从零到运行 JS，只需几秒。",
      body: "添加依赖、选择引擎，即可在 Rust 中运行 JavaScript —— 或将 Rust 类暴露给 JavaScript。",
    },
    codeTabs: ["嵌入与求值", "Worker 池", "类绑定", "Cargo.toml", "CLI"],
    modulesHead: {
      eyebrow: "开箱即用",
      title: "二十个内置模块。",
      body: "常见的运行时任务尽在其中 —— 定时器、HTTP、文件系统、存储、worker、Redis、SQLite、S3 等。点击模块即可阅读其 API 参考。",
    },
    ecoHead: {
      eyebrow: "生态",
      title: "不止于 crate。",
      body: "Rong 同时发布到 crates.io 和 npm，并为 TypeScript 用户与 AI 智能体提供配套工具。",
    },
    eco: {
      crate: "运行时本体，以及由单一发布工作流按依赖顺序发布的各模块 crate。",
      types: "Rong 运行时的 TypeScript 类型定义，让面向 Rong 编写的 JS 获得完整的编辑器支持。",
      skill: "可安装的智能体技能，内含自洽的文档与生成式 API 参考，服务 AI 编码智能体。",
      cli: "本地运行时执行与 REPL 工作流，通过 Cargo features 选择引擎。",
    },
    skillsHead: {
      eyebrow: "智能体技能",
      title: "让你的 AI 智能体学会 Rong。",
      bodyHtml:
        '<code class="inline-code">@rongjs/rong-skill</code> 打包了两个可安装的智能体技能 —— 自洽的 <code class="inline-code">SKILL.md</code> 文档与生成式 API 参考，适用于任何支持文件式技能的智能体运行时。',
    },
    skills: {
      runtimeDev:
        '编写 Rong JavaScript 脚本、选择正确的公共 API、改编示例、运行 <code class="inline-code">rong_cli</code> 并编译字节码。',
      moduleAuthor: "编写或修改 Rust 模块，暴露 Rong API、类、函数、类型转换及 JavaScript 错误。",
      noteHtml:
        '使用 <code class="inline-code">--project</code> 进行项目级安装，或用 <code class="inline-code">--skill &lt;name&gt;</code> 只安装其中一个。这些技能与本站的模块 API 文档同源 —— 单一事实来源。',
    },
    cta: {
      title: "把 JavaScript 带进你的 Rust 应用。",
      body: "嵌入运行时、暴露 Rust 驱动的 API，并以 worker 池扩展 —— 覆盖所有受支持的引擎。",
      start: "开始构建",
      moduleGuide: "模块开发指南",
    },
  },
};
