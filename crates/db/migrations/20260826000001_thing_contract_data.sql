-- 契约数据迁移(device→thing PR-2 Task 1)。
-- 将数据库中的契约值从 device 系归并为 thing 系:
--   1. tag_bindings.target_type 的 'device' 归并为 'thing'('app' 等非 device 行不动);
--   2. permissions 的 device 系行更名:id perm-device-*→perm-thing-*、
--      name device:*→thing:*、resource_type 'device'→'thing';
--   3. role_permissions / user_permissions 的 permission_id 引用同步更名。
-- 顺序:子表先、permissions 主表后(迁移器以 FK OFF 运行,顺序仅为逻辑清晰)。
UPDATE tag_bindings SET target_type = 'thing' WHERE target_type = 'device';
UPDATE role_permissions SET permission_id = 'perm-thing-' || substr(permission_id, 13) WHERE permission_id LIKE 'perm-device-%';
UPDATE user_permissions SET permission_id = 'perm-thing-' || substr(permission_id, 13) WHERE permission_id LIKE 'perm-device-%';
UPDATE permissions SET id = 'perm-thing-' || substr(id, 13),
                       name = 'thing:' || substr(name, 8),
                       resource_type = 'thing'
WHERE resource_type = 'device';
