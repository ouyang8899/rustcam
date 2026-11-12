// @ts-check

/**
 * 电子围栏配色 —— 禁飞区（no_fly）红色，允许区（allowed）绿色。
 *
 * 电子围栏 / 实时飞行 / 航线规划三处都引用这里的常量，
 * 保证同一条围栏在任何页面都是同一种颜色。
 */
export const FENCE_COLORS = {
  no_fly: {
    fill: '#ef4444',
    fillRgba: 'rgba(239,68,68,0.25)',
    outline: '#ef4444',
    label: '禁飞区',
  },
  allowed: {
    fill: '#22c55e',
    fillRgba: 'rgba(34,197,94,0.25)',
    outline: '#22c55e',
    label: '允许区',
  },
}

/**
 * 规则归一：后端规则（no_fly / work / height）与前端展示规则（no_fly / allowed）。
 * 除允许区外一律按禁飞区展示（前端表单只有这两种规则）。
 * @param {unknown} type
 * @returns {'no_fly' | 'allowed'}
 */
export function fenceRule(type) {
  const t = String(type || '')
    .trim()
    .toLowerCase()
  return t === 'work' || t === 'allowed' ? 'allowed' : 'no_fly'
}

/**
 * 取围栏配色
 * @param {unknown} type 规则（no_fly / work / allowed）
 * @returns {{ fill: string, fillRgba: string, outline: string, label: string }}
 */
export function fenceColors(type) {
  return FENCE_COLORS[fenceRule(type)]
}

/**
 * 取围栏规则文案
 * @param {unknown} type
 * @returns {string}
 */
export function fenceRuleLabel(type) {
  return fenceColors(type).label
}
