-- KEYS[1] = control:{device_id}:{control_type}
-- ARGV[1] = holder, ARGV[2] = priority, ARGV[3] = now_ms, ARGV[4] = lease_ms
-- 返回 {granted(0|1), 当前持有者}
local key    = KEYS[1]
local holder = ARGV[1]
local prio   = tonumber(ARGV[2])
local now    = tonumber(ARGV[3])
local lease  = tonumber(ARGV[4])

local cur = redis.call('HMGET', key, 'holder', 'priority', 'expire_at')
local cur_holder = cur[1]
local cur_expire = tonumber(cur[3] or '0')

-- 无人持有或已过期才允许获取；有人持有且未过期一律失败（抢占走 preempt.lua）
if not cur_holder or now >= cur_expire then
  redis.call('HSET', key,
    'holder', holder,
    'priority', prio,
    'acquired_at', now,
    'expire_at', now + lease)
  redis.call('PEXPIRE', key, lease)
  return {1, ''}
end

return {0, cur_holder}
