#pragma once

#include <QString>

namespace rustcam {
namespace browser {

/// 每个浏览器进程独享的 Chromium profile 目录。
///
/// CEF 120+ 对 `root_cache_path` 启用进程单例锁：共用同一个 profile 的第二个
/// 进程会打印 "Opening in existing browser session." 并让 `CefInitialize()`
/// 返回 false——命令行还会被转发给已有实例，被当成 URL 打开
/// （用户看到的就是凭空冒出来的 http://rustcam-browser-<pid>/ 窗口）。
///
/// 目录按进程 pid 命名，因此任何时刻都不会有两个进程写同一个 profile。
/// profile 有几十兆，所以必须及时回收：
///   - 正常退出：浏览器自己删掉自己的目录；
///   - 被 clientapp 杀掉：clientapp 删掉对应 pid 的目录；
///   - 崩溃残留：下次启动（或运行期定时清扫）按 pid 是否存活删掉。
namespace instance_profile {

/// profile 根目录（由 clientapp 用 --cache-root 指定，或默认取当前目录下的
/// browser-cache）。
QString cacheRoot();

/// 本进程 profile 目录的绝对路径（尚未创建）。
QString profileDir();

/// 本进程 profile 目录下的临时目录（Chromium 从 TMP/TEMP 取）。
QString tempDir();

/// 创建本进程目录，并清扫上次崩溃残留的实例目录。
void prepare(const QString& cacheRoot);

/// 清扫残留的实例目录：目录名是进程 pid，进程没了就可以删。
void sweepStale();

/// 删除本进程目录（正常退出时调用）。
void cleanup();

} // namespace instance_profile
} // namespace browser
} // namespace rustcam
