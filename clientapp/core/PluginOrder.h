#pragma once

#include <QStringList>
#include "CoreExport.h"

namespace rustcam {

/// 按用户配置的插件顺序排列插件 id。
///
/// configured 中出现的 id 按配置顺序排在前面；未在配置中出现的插件保持
/// available 里的原有相对顺序，追加在后面。配置中重复出现的 id 只保留一次，
/// 配置中不存在的 id 直接忽略。
RUSTCAM_CORE_API QStringList orderPluginIds(const QStringList& configured,
                                            const QStringList& available);

/// 插件没有配置过顺序时使用的默认顺序。
RUSTCAM_CORE_API QStringList defaultPluginIds();

/// 从插件文件路径推断插件 id：
/// rustcam_plugins/<插件 id>/rustcam-<插件 id>.dll 取目录名，
/// 放在插件目录根下的文件取去掉 "rustcam-" 前缀的文件名。
RUSTCAM_CORE_API QString pluginIdFromPath(const QString& filePath);

/// 按插件 id 的配置顺序排列插件文件路径。
/// 没有配置顺序的插件保持原有相对顺序，排在最后。
RUSTCAM_CORE_API QStringList sortPluginFiles(const QStringList& files,
                                             const QStringList& configured);

}
