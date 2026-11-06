## Parent

https://github.com/Duqiyan2/token-market-cli/issues/39

## What to build

用户通过 CLI 或桌面首次接入一个没有历史 Key 的 Tool Connection 时，平台按
Bound Workspace 创建正确命名的 Agent API Key，客户端安全保存并完成配置。
个人空间只显示工具名字，企业空间显示工具名字与可信用户标识；之后两个入口
复用同一新规则 Key。这是新范围从签发到实际工具配置的完整纵向切片。

## Acceptance criteria

- [ ] 个人空间的 Key Display Name 仅含工具产品展示名，例如 `Claude Code`，无论用户有无手机号，都不附加手机号或用户 ID。
- [ ] 企业空间使用 `工具名-手机号`；主资料无手机号时读取已绑定手机号，仍无手机号时使用稳定用户 ID，不使用昵称。
- [ ] 企业空间国内手机号去掉 `+86` 及分隔符，其他国际区号保留；两类空间均不带 Token Market 前缀。
- [ ] 根据平台验证过的 Bound Workspace 及已有个人空间兼容语义判定分支；只有一个成员的企业空间仍附加用户标识，空间名称、成员数和企业认证状态不决定命名。
- [ ] 所有已注册 Token Market 工具使用产品展示名，原有 Tool 标识、Key Source、归属和权限保持不变。
- [ ] 从首次签发开始持久化明确的新规则采用状态，防止后续功能将新范围误认为旧规则；不通过可编辑名称判断版本。
- [ ] CLI 和桌面均可完成首次连接，先加密保存凭据再提交配置；同设备、账号、空间、Tool 和服务地址范围随后复用同一个 Key。
- [ ] 同一用户不同设备允许同名但 Key ID 不同；不同企业用户的手机号或用户 ID 标识可区分。
- [ ] 切换 Current Workspace 不修改已有 Key 的名称或 Bound Workspace；既有历史连接在本切片中不被后台改写或批量迁移。
- [ ] 复用既有客户端连接协议及平台服务测试边界，以实际 Key 名称、保存记录和最终工具配置验证上述结果，不新建专用测试 API。

## Blocked by

None (can start immediately).
