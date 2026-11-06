-- KEYS[1] = control:{device_id}:{control_type}
-- ARGV[1] = holder
-- 返回 1 = 已释放，0 = 非持有人
local cur_holder = redis.call('HGET', KEYS[1], 'holder')
if cur_holder == ARGV[1] then
  redis.call('DEL', KEYS[1])
  return 1
end
return 0
