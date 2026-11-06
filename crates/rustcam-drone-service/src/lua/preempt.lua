-- KEYS[1] = control:{device_id}:{control_type}
-- ARGV[1] = holder, ARGV[2] = priority, ARGV[3] = now_ms, ARGV[4] = lease_ms
-- 返回 {granted(0|1), 被抢占者}
local key    = KEYS[1]
local holder = ARGV[1]
local prio   = tonumber(ARGV[2])
local now    = tonumber(ARGV[3])
local lease  = tonumber(ARGV[4])

local cur = redis.call('HMGET', key, 'holder', 'priority')
local old_holder = cur[1]
local old_prio   = tonumber(cur[2] or '0')

if not old_holder or prio > old_prio then
  redis.call('HSET', key,
    'holder', holder,
    'priority', prio,
    'acquired_at', now,
    'expire_at', now + lease)
  redis.call('PEXPIRE', key, lease)
  return {1, old_holder or ''}
end

return {0, old_holder}
