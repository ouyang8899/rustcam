/**
 * 取出内容模块在指定语言下的数据，缺失时回退到中文。
 * @param {{ zh?: object, en?: object } | undefined} module
 * @param {string} locale
 * @returns {object}
 */
export function pickLocale(module, locale) {
  if (!module || typeof module !== 'object') return {}
  return module[locale] ?? module.zh ?? {}
}
