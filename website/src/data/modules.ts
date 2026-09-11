// The 21 built-in modules (workspace members under modules/).
// Module API pages are generated from the repo's docs/api/*.md at build
// time — see src/content.config.ts. rong_cron has no docs/api entry yet and
// falls back to modules/rong_cron/README.md.
import type { Locale } from "../i18n/ui";

export interface ModuleEntry {
  name: string;
  desc: Record<Locale, string>;
}

export const MODULES: ModuleEntry[] = [
  { name: "rong_timer", desc: { en: "setTimeout, setInterval, async timers", zh: "setTimeout、setInterval 与异步定时器" } },
  { name: "rong_http", desc: { en: "HTTP client/server, fetch API", zh: "HTTP 客户端/服务端，fetch API" } },
  { name: "rong_fs", desc: { en: "File system operations", zh: "文件系统操作" } },
  { name: "rong_console", desc: { en: "Console logging & debugging", zh: "控制台日志与调试" } },
  { name: "rong_url", desc: { en: "URL parsing & manipulation", zh: "URL 解析与处理" } },
  { name: "rong_buffer", desc: { en: "Binary data handling", zh: "二进制数据处理" } },
  { name: "rong_event", desc: { en: "Event emitter & handling", zh: "事件派发与处理" } },
  { name: "rong_abort", desc: { en: "AbortController & signals", zh: "AbortController 与信号" } },
  { name: "rong_encoding", desc: { en: "Text encoding / decoding", zh: "文本编码 / 解码" } },
  { name: "rong_crypto", desc: { en: "Web Cryptography API (crypto.subtle)", zh: "Web Cryptography API（crypto.subtle）" } },
  { name: "rong_assert", desc: { en: "Assertion utilities", zh: "断言工具" } },
  { name: "rong_exception", desc: { en: "Exception handling", zh: "异常处理" } },
  { name: "rong_storage", desc: { en: "Storage APIs", zh: "存储 API" } },
  { name: "rong_stream", desc: { en: "Stream APIs", zh: "流 API" } },
  { name: "rong_compression", desc: { en: "Compression & decompression", zh: "压缩与解压" } },
  { name: "rong_command", desc: { en: "Subprocess & shell execution", zh: "子进程与 shell 执行" } },
  { name: "rong_worker", desc: { en: "JavaScript worker threads", zh: "JavaScript worker 线程" } },
  { name: "rong_cron", desc: { en: "Cron parsing & scheduled jobs", zh: "Cron 解析与定时任务" } },
  { name: "rong_redis", desc: { en: "Redis client APIs", zh: "Redis 客户端 API" } },
  { name: "rong_sqlite", desc: { en: "SQLite APIs", zh: "SQLite API" } },
  { name: "rong_s3", desc: { en: "S3-compatible object storage", zh: "S3 兼容对象存储" } },
];
