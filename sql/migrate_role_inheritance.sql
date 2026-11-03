-- 角色继承（RBAC 层级）与授权关系索引
-- 幂等脚本：可重复执行

CREATE TABLE IF NOT EXISTS role_inheritance (
    parent_role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    child_role_id  UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (parent_role_id, child_role_id),
    CHECK (parent_role_id <> child_role_id)
);

CREATE INDEX IF NOT EXISTS idx_role_inheritance_child ON role_inheritance (child_role_id);
CREATE INDEX IF NOT EXISTS idx_role_permissions_permission ON role_permissions (permission_id);
CREATE INDEX IF NOT EXISTS idx_user_roles_role ON user_roles (role_id);
