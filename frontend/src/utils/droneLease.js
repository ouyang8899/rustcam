// @ts-check
/**
 * 控制权租约时长选项（秒）。
 *
 * 后端 `lease_secs` 会把取值夹在 5..3600 秒之间；这里只提供常用档位。
 */
export const LEASE_OPTIONS = [
  { label: '1 分钟', value: 60 },
  { label: '3 分钟', value: 180 },
  { label: '5 分钟', value: 300 },
  { label: '10 分钟', value: 600 },
  { label: '30 分钟', value: 1800 },
  { label: '1 小时', value: 3600 },
]

export const DEFAULT_LEASE_SECONDS = 300
