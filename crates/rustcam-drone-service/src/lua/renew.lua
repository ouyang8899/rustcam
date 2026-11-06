-- KEYS[1] = control:{device_id}:{control_type}
-- ARGV[1] = holder, ARGV[2] = now_ms, ARGV[3] = lease_ms
-- 返回 1 = 已续约，0 = 非持有人
local cur_holder = redis.call('HGET', KEYS[1], 'holder')
if cur_holder == ARGV[1] then
  redis.call('HSET', KEYS[1], 'expire_at', tonumber(ARGV[2]) + tonumber(ARGV[3]))
  redis.call('PEXPIRE', KEYS[1], ARGV[3])
  return 1
end
return 0
