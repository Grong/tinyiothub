-- E4a 结转：迁移期声明强制开关。默认 1（fail-closed）；存量工作区在
-- rematerialize 完成前可临时置 0，让缺声明的旧物继续可调用。
ALTER TABLE workspaces ADD COLUMN declaration_enforcement INTEGER NOT NULL DEFAULT 1;
