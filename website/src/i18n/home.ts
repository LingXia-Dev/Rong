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
  "M12 2v3M12 19v3M4.93 4.93l2.12 2.12M16.95 16.95l2.12 2.12M2 12h3M19 12h3M4.93 19.07l2.12-2.12M16.95 7.05l2.12-2.12M12 9v3l2 2",
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
    copyInstall: string;
  };
  stats: [string, string][];
  featuresHead: { eyebrow: string; title: string; body: string };
  features: Feature[];
  enginesHead: { eyebrow: string; title: string; body: string };
  engines: Engine[];
  archHead: { eyebrow: string; title: string; body: string };
  arch: { core: string; services: string[]; modulesTitle: string };
  startHead: { eyebrow: string; title: string; body: string; tabsLabel: string };
  codeTabs: string[];
  modulesHead: { eyebrow: string; title: string; body: string };
  ecoHead: { eyebrow: string; title: string; body: string };
  eco: { crate: string; test: string; types: string; skill: string; cli: string };
  skillsHead: { eyebrow: string; title: string; bodyHtml: string };
  skills: {
    runtimeEmbedder: string;
    runtimeDev: string;
    moduleAuthor: string;
    noteHtml: string;
    copyInstall: string;
  };
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
      copyInstall: "Copy install command",
    },
    stats: [
      ["3", "JavaScript engines"],
      ["20", "built-in modules"],
      ["1.95+", "Rust toolchain (2024 edition)"],
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
        title: "Bounded execution",
        body: "Apply task deadlines across queueing and execution, interrupt non-yielding JavaScript where the engine supports it, and reuse workers safely after timeout.",
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
        body: "Links the system JavaScriptCore.framework on macOS and iOS, or uses pinned source-built WebKit/JSCOnly artifacts on supported targets. Windows source artifacts are currently unavailable.",
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
      tabsLabel: "Quick start examples",
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
      body: "Rong ships to crates.io and npm, with focused packages for TypeScript, JavaScript testing, and AI agents.",
    },
    eco: {
      crate:
        "The runtime itself, plus per-module crates published in dependency order from a single release workflow.",
      test:
        "A zero-dependency test framework with sequential async cases, nested hooks, strict matchers, and structured reports across Rong engines.",
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
        '<code class="inline-code">@rongjs/rong-skill</code> bundles three installable agent skills — self-contained <code class="inline-code">SKILL.md</code> documents with generated API references, for any agent runtime that supports file-based skills.',
    },
    skills: {
      runtimeEmbedder:
        "Build Rust hosts around Rong runtimes, choose module capabilities, supervise worker lifecycles, and apply cancellation, deadlines, and interruption correctly.",
      runtimeDev:
        'Write Rong JavaScript scripts, choose the right public APIs, adapt examples, run <code class="inline-code">rong_cli</code>, and compile bytecode.',
      moduleAuthor:
        "Write or edit Rust modules that expose Rong APIs, classes, functions, type conversions, and JavaScript errors.",
      noteHtml:
        'Use <code class="inline-code">--project</code> for a project-local install, or <code class="inline-code">--skill &lt;name&gt;</code> to install just one. The skills share their source with the module API docs on this site — one source of truth.',
      copyInstall: "Copy skill install command",
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
      copyInstall: "复制安装命令",
    },
    stats: [
      ["3", "种 JavaScript 引擎"],
      ["20", "个内置模块"],
      ["1.95+", "Rust 工具链（2024 edition）"],
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
        title: "有界执行",
        body: "让任务截止时间覆盖排队与执行；在引擎支持时中断不让出执行权的 JavaScript，并在超时后安全复用 worker。",
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
        body: "在 macOS 和 iOS 上链接系统 JavaScriptCore.framework，或在受支持目标上使用固定版本、源码构建的 WebKit/JSCOnly 产物。Windows 源码产物目前不可用。",
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
      tabsLabel: "快速上手示例",
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
      body: "Rong 同时发布到 crates.io 和 npm，并为 TypeScript、JavaScript 测试与 AI 智能体提供专用软件包。",
    },
    eco: {
      crate: "运行时本体，以及由单一发布工作流按依赖顺序发布的各模块 crate。",
      test: "零依赖测试框架，在所有 Rong 引擎上提供顺序异步用例、嵌套 hooks、严格 matchers 与结构化报告。",
      types: "Rong 运行时的 TypeScript 类型定义，让面向 Rong 编写的 JS 获得完整的编辑器支持。",
      skill: "可安装的智能体技能，内含自洽的文档与生成式 API 参考，服务 AI 编码智能体。",
      cli: "本地运行时执行与 REPL 工作流，通过 Cargo features 选择引擎。",
    },
    skillsHead: {
      eyebrow: "智能体技能",
      title: "让你的 AI 智能体学会 Rong。",
      bodyHtml:
        '<code class="inline-code">@rongjs/rong-skill</code> 打包了三个可安装的智能体技能 —— 自洽的 <code class="inline-code">SKILL.md</code> 文档与生成式 API 参考，适用于任何支持文件式技能的智能体运行时。',
    },
    skills: {
      runtimeEmbedder: "围绕 Rong runtime 构建 Rust host，选择模块能力，管理 worker 生命周期，并正确应用取消、截止时间与执行中断。",
      runtimeDev:
        '编写 Rong JavaScript 脚本、选择正确的公共 API、改编示例、运行 <code class="inline-code">rong_cli</code> 并编译字节码。',
      moduleAuthor: "编写或修改 Rust 模块，暴露 Rong API、类、函数、类型转换及 JavaScript 错误。",
      noteHtml:
        '使用 <code class="inline-code">--project</code> 进行项目级安装，或用 <code class="inline-code">--skill &lt;name&gt;</code> 只安装其中一个。这些技能与本站的模块 API 文档同源 —— 单一事实来源。',
      copyInstall: "复制技能安装命令",
    },
    cta: {
      title: "把 JavaScript 带进你的 Rust 应用。",
      body: "嵌入运行时、暴露 Rust 驱动的 API，并以 worker 池扩展 —— 覆盖所有受支持的引擎。",
      start: "开始构建",
      moduleGuide: "模块开发指南",
    },
  },
};
