// @ts-check

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

export function fenceRule(type) {
  const t = String(type || '')
    .trim()
    .toLowerCase()
  return t === 'work' || t === 'allowed' ? 'allowed' : 'no_fly'
}

export function fenceColors(type) {
  return FENCE_COLORS[fenceRule(type)]
}

export function fenceRuleLabel(type) {
  return fenceColors(type).label
}
