# 换物集小程序功能与后端接口需求

> 给接手后端开发者的业务导览。内容根据 `miniprogram/` 当前页面、事件处理和演示数据整理；接口名与请求/响应字段是建议的服务端契约，不代表目前已经存在。当前后端只有 `system.health` 示例接口，尚未实现下文业务接口。

## 1. 产品是什么

「换换」是一个以物换物平台。用户发布自己的物品，用估值区间匹配另一件物品，向物主发出交换请求；对方同意后形成锁定快照的契约，双方当面或通过快递完成交换。平台目前的设计原则是不收保证金、不代收款，重点是估值比对、身份核验、履约记录、信用和争议留证。

它不是二手交易付款平台：当前没有购买、收款、退款、平台托管资金或微信支付流程。社区里的「买不买」是独立的消费讨论区，与物品交换决策分开。

## 2. 当前工程状态与名词

- 小程序注册了 15 个页面，5 个 tab：换物、买不买、发布、请求、我的，见 `miniprogram/app.json`。
- 页面数据主要来自 `miniprogram/utils/data.js` 的固定样例以及 `app.js` 的内存状态；用户、信用分、城市会写入本地 storage。重启后物品、请求、帖子、评论和纪念册会回到样例数据。
- `miniprogram/api/system/system.js` 是之前生成的健康检查客户端。小程序页面还没有调用业务后端接口。
- 业务术语：
  - **物品**：用户发布、希望交换出去的东西。
  - **换物请求**：发起人拿一件自己的物品，向另一位用户的物品提出交换。请求待对方通过时还不是生效契约。
  - **契约**：请求被对方接受后形成的交换记录；双方物品与估值等关键信息应做不可单方修改的快照。
  - **履约**：按契约选择当面或快递方式，完成核验、验货、留证和双方确认。
  - **信用分**：平台自己的分值，不是微信支付分。它影响能否交易、跨城、快递及高价值物品权限。

## 3. 建议开发顺序

### P0：先让核心换物闭环成立

1. 微信登录、服务端 token、用户资料。
2. 物品列表/详情、图片上传、搜索与筛选、发布物品、我的物品。
3. 发起请求、收到请求列表、接受/拒绝、冷静期撤回。
4. 接受请求时用数据库事务校验并占用物品，生成契约快照，防止同一物品被多个请求同时成交。

### P1：交易履约与信用

1. 契约详情、面交地点、一次性核验码、验货检查项、证据上传。
2. 双方完成确认、异议和人工判定流程。
3. 信用摘要和不可由客户端直接修改的信用流水。
4. 快递单号、寄出/揽收/签收快照及 48 小时异议窗口。真实称重与物流状态需要物流/快递能力接入，不能由小程序按钮自报为“验真通过”。

### P2：社区与体验功能

1. 买不买帖子、投票、评论、举报/审核。
2. 收藏、降价提醒、订阅消息。
3. 纪念册、个人统计、设置、通知和黑名单。
4. 估值样本、行情截图 OCR 与人工复核。

可以先只交付 P0 的最小闭环；不要为了让页面显示数据而把信用、交易状态或审核结论交给小程序决定。

## 4. API 约定建议

小程序当前使用 AFast 二进制协议的 HTTP transport 与生成的 `wx.request` 客户端通信。建议按 service 分组，Rust `#[handler]` 的输入输出都定义为 AFast 可序列化类型，并生成到对应的 `miniprogram/api/<service>/` 目录。

建议的 service 边界：

| Service | 负责范围 |
|---|---|
| `auth` | 微信登录、token、手机号绑定、实名状态 |
| `item` | 物品浏览、详情、搜索、发布、我的物品、收藏 |
| `swap` | 换物请求、请求处理、契约生成与查询 |
| `fulfillment` | 当面/快递履约、核验、验货、证据、异议、完成 |
| `community` | 买不买帖子、投票、评论、举报 |
| `member` | 我的资料、信用摘要/流水、交换记录、纪念册、设置 |
| `region` | 省市/偏远地区与面交点查询 |

接口字段是业务层草案；后端写模型时可按 AFast handler 实际序列化限制和数据库类型调整。常见规范：

- 读列表统一支持 `page`、`page_size`（服务端强制上限，例如 50）；数据量增加后可改 cursor。不要要求一次把所有 337 个城市的全部物品下载到客户端。
- ID 用服务端生成的稳定 ID；金额区间用整数分/整数元，不用浮点金额。物品列表默认只返回可交换状态，且排除本人已下架、已交换、已锁定的物品。
- 所有写操作要求登录 token；服务端从 token 识别用户，不能相信客户端传来的 `user_id`、昵称、信用分、角色、物主 ID 或“实名通过”布尔值。
- 业务拒绝（无权限、状态不允许、估值不公平、资源已被占用）应使用可区分的业务错误；数据库/第三方服务错误不能伪装成成功。
- 对创建请求、接受请求、上传完成、履约完成、信用流水等关键命令考虑幂等键。请求状态变更和物品占用必须在数据库事务内完成。
- 下文 `created_at`、`updated_at` 等建议统一为服务端 UTC 时间；用户展示时再按本地时区转换。

## 5. 接口清单（按业务功能）

### 5.1 登录与账号（`auth`）

| 建议接口 | 请求 | 响应/作用 | 备注 |
|---|---|---|---|
| `login_mini` | `code`, `nickname`, `agreement_version` | `token`, `expires_at_ms`, `is_new_user`, `user`（平台 user_id、昵称、脱敏手机号和实名状态） | 前端在用户点击登录后通过 `wx.getUserProfile()` 获取授权的微信昵称并随请求发送；服务端登录时同步平台昵称。服务端通过 AFaster `state.wxlogin.mini_login()` 调微信 `code2session`；openid 仅用于微信身份映射，平台签发随机不透明 token 并只存其哈希。code 一次性使用，不能由前端构造 openid。当前不接收头像临时路径，也不返回尚未实现的信用摘要。 |
| `get_me` | 无（token） | 当前用户资料、手机号和实名状态 | 登录态恢复后拉取，作为“我的”页真实数据源。 |
| `update_profile` | `nickname?`, `avatar_upload_key?` | 更新后的公开资料 | 头像临时路径不是可供其他用户访问的 URL；需要先上传对象存储。 |
| `bind_phone` | 微信手机号授权 `code`, `confirm_transfer` | 脱敏手机号、绑定状态、是否从旧账号转移 | 需符合小程序主体/权限要求；由后端调用 AFaster `state.wxlogin.get_user_phone_number()` 换取号码。号码已绑定其他账号且 `confirm_transfer=false` 时返回 `41006`；前端须向用户说明原账号不会删除、仅解除手机号绑定，用户确认后重新获取一次性手机号 code 并以 `confirm_transfer=true` 重试。解绑旧账号与绑定当前账号在同一数据库事务中完成。当前账号若已绑定其他手机号则返回 `41007`。不信任客户端传入的手机号明文。 |
| `get_auth_status` | 无（token） | `phone_bound`, `real_name_status` | 实名功能目前只有演示占位。后续接合规实名服务时只存业务所需结果与第三方流水号，敏感证件数据按服务商方案处理。 |
| `logout` | 无（token） | `revoked` | 撤销当前服务端会话；客户端同时删除本地 token。 |

登录页说明：未登录可以浏览；发起交换、投票、发帖、查看个人纪念册等需要登录。手机号和实名在页面文案中被描述为交易前补充，但代码当前并未真正强制。最终是否强制手机号/实名、在哪个步骤强制，需要结合产品合规要求定稿。

### 5.2 物品、图片、估值与搜索（`item`）

| 建议接口 | 请求 | 响应/作用 |
|---|---|---|
| `list_items` | `city_id`, `nationwide`, `category_code`, `keyword`, `page`, `page_size`, `sort_code` | `items[]`, `total`, `page`; 项含名称、品类、成色、估值区间、城市、封面、卖家公开摘要、履约方式、状态 |
| `get_item` | `item_id` | 完整可公开详情：图片、瑕疵/维修描述、估值来源和有效期、城市/区域、履约限制、卖家公开信用统计、脱敏序列号状态 |
| `search_items` | 与 `list_items` 相同，`keyword` 必填 | 匹配结果 | 当前仅按标题搜索；不检索序列号。 |
| `publish_item` | `title`, `category_code`, `condition_code`, `description`, `serial_number`, `valuation_method`, `value_low_fen`, `value_high_fen`, `city_id`, `city_name`, `fulfillment_modes[]`, `photo_file_md5s[]` | `item_id`, `item_status`, `created_at_ms` | 登录；图片必须属于当前用户且已确认上传。至多 9 张实拍图；标题最多 30 字，瑕疵说明最多 120 字。 |
| `list_my_items` | `status?`, `page`, `page_size` | 当前用户物品与可编辑状态 | “我的”页当前点击“我发布的物品”只是演示 toast。 |
| `update_item` | `item_id`, 可修改字段, `version` | 更新结果 | 已进入待交换请求/契约快照的关键字段不得静默修改；应通过撤销/重新确认等状态流程处理。 |
| `change_item_status` | `item_id`, `action`（如下架/重新上架）, `version` | 新状态 | 有生效契约时不允许下架破坏契约。 |
| `add_favorite` / `remove_favorite` | `item_id` | 收藏状态 | 详情页爱心目前仅改本地 `fav`。 |
| `set_price_alert` | `item_id`, `enabled`, `subscription_template?` | 提醒状态 | 页面承诺估值下跌 8% 或同款更低时提醒；当前模板 ID 是占位符，还没有订阅/通知实现。 |

**图片上传**：小程序调用 `storage.get_upload_file_link` 申请一次性授权后直传对象存储，再调用 `storage.complete_upload` 确认。发布请求仅提交 MD5；后端按登录用户查询 `uploaded_assets` 并绑定已上传对象，不接受客户端 URL 或他人的 MD5。列表和详情响应中的图片 URL 是服务端按对象 ID 申请的短期访问地址，不作为永久字段保存。当前尚未记录/校验对象大小。

**服务端校验**：

- `value_low < value_high` 且均为正整数；当前后端按 `(low + high) / 2 > 3000`（元）拒绝高价值物品，因为尚无服务端信用分能力，不能信任客户端分数。
- 前端要求序列号为 10–12 位字母数字或 15 位数字。后端只保存脱敏尾号、不保存完整号码；格式检查不代表品牌/IMEI 真伪验证。
- 前端类别为数码、潮玩、图书、家居、服饰；类别表应由服务端枚举/配置或保证与客户端常量版本一致。
- 当前仅开放手动估值，物品发布后立即进入可浏览的 `active` 状态；城市 ID 暂用 `省份:城市`，待地区服务提供稳定 ID 后迁移。
- 上架、编辑、删除均检查物品所有权、审核状态、请求/契约锁定状态。

**当前已落地范围**：物品列表/标题搜索/详情/发布已实现；`items` 和 `item_images` 的 MySQL migration 尚需部署时执行。发布即 `active`，暂无审核、编辑、下架、收藏、个人物品列表、信用分或换物请求 API。浏览图片时后端为每张图片单独申请短期访问地址；批量 CloudRun 下载接口暂未由 AFaster 暴露。

### 5.3 换物匹配与请求（`swap`）

| 建议接口 | 请求 | 响应/作用 |
|---|---|---|
| `preview_swap` | `target_item_id`, `offered_item_id` | 公平性检查（估值重叠、中心价差、可履约方式、权限拒绝原因） | 只是预览，不代替创建时再次检查。 |
| `create_swap_request` | `target_item_id`, `offered_item_id`, `message?`, `idempotency_key` | 请求 ID、状态、`cancel_until`、摘要 | 留言最多 80 字。页面要求双方物品估值区间有交集**或**中心价差不超过 20%；中心价差示例算法为 `abs(midA-midB)/max(midA,midB)`。必须由服务端重算。 |
| `list_swap_requests` | `direction`（all/incoming/outgoing）, `status?`, `page`, `page_size` | 请求卡片列表 | 卡片需显示对方、两件物品快照摘要、城市、留言、待处理/已接受/拒绝/撤回状态、冷静期及是否可履约。 |
| `preview_matches`（可选） | `city?`, `page`, `page_size` | 推荐物品、对应的本人出物、估值重叠、同城/信用标签和排序分 | 目前在客户端比较所有样例物品；推荐分只是排序，不是成功率。若物品池很大，由服务端先按可交换状态和权限筛选再排序。 |
| `respond_swap_request` | `request_id`, `decision`（accept/reject）, `expected_version`, `idempotency_key` | 请求新状态及新建的 `contract_id?` | **前端缺口**：收件箱当前没有接受/拒绝按钮或对应 handler。后端需要定义此接口，同时后续前端必须加入口，否则收到的请求无法真正确认。接受应事务化检查请求仍有效、双方权限、物品未被占用，再锁定物品和估值/图片/描述/序列号快照并建契约。 |
| `cancel_swap_request` | `request_id`, `expected_version` | 撤回结果 | 前端给发起者 10 分钟冷静期；样例逻辑只允许 10 分钟内撤回且不扣分。超时后应按产品规则拒绝或进入取消/违约流程。 |
| `get_contract` | `contract_id` | 契约双方、快照、履约方式、地点、状态、时间线、权限 | 已生效契约在用户退出登录后仍需履行；重新认证后按契约参与方授权读取。 |
| `list_my_contracts` | `status?`, `page`, `page_size` | 当前用户契约列表 | 对应“换物记录”和履约入口；目前为固定 3 条演示数。 |

**推荐状态机**（最终状态名以产品确认）：请求 `pending → accepted / rejected / cancelled / expired`；接受后只生成一个契约。契约可 `awaiting_fulfillment → fulfilling → completed`，异议进入 `disputed`，审核后再 `completed / cancelled / resolved`。拒绝、过期、撤回不得再接受；同一请求不能重复生成多份契约。

**并发和一致性**：创建请求时检查两件物品都可交换；接受时在事务内再次检查并原子占用两件物品，避免两名用户同时接受包含相同物品的请求。状态更新需要 version/幂等检查。发送请求不应只靠小程序本地 `Date.now()` 确定 10 分钟截止点。

### 5.4 履约、核验、证据与争议（`fulfillment`）

| 建议接口 | 请求 | 响应/作用 |
|---|---|---|
| `get_fulfillment` | `contract_id` | 履约状态、双方快照、履约方式、面交/快递进度、双方检查项和已上传证据 |
| `set_fulfillment_mode` | `contract_id`, `mode`（face/express） | 可用方式与状态 | 只有契约允许且双方条件都满足才可切换；偏远地区当前规则仅同城面交，信用分低于 80 不开放快递/全国。不能只因页面显示可选就信任前端。 |
| `set_meeting_place` | `contract_id`, `place_id`, `scheduled_at?`, 安全开关 | 保存的面交安排 | 前端展示的 3 个面交点是演示样例，无坐标/真实距离；需要一份真实安全地点数据来源。紧急联系人地点共享应明确授权、可见范围和自动失效时机。 |
| `create_verification_code` | `contract_id` | 一次性短时二维码/核验 token、过期时间 | 当前页面用随机数在本地画 17×17 格子，不能作为身份或防重放凭据。生成码须由服务端产生，绑定契约和用户、短有效期、一次使用。 |
| `verify_meeting_code` | `contract_id`, 对方扫码得到的 opaque token | 通过/拒绝及原因 | 服务端校验码有效期、使用状态、契约双方身份及所需的同城规则；不能仅靠客户端说“扫码成功”。前端的 action-sheet 三态是模拟。 |
| `submit_inspection` | `contract_id`, 检查项结果（成色、瑕疵、序列号、配件）, `expected_version` | 检查结果和可否继续 | 前端要求四项全部勾选；记录谁在何时确认，检查项应与契约快照对应。序列号不要返回完整敏感值。 |
| `upload_evidence` | `contract_id`, `evidence_type`, `upload_key`, `captured_at?` | 证据 ID、服务端记录时间 | 面交目前支持“任意一张”或“双方各上传一张”，后者演示加 1 分；照片带时间地点水印的承诺尚未实现。客户端时间/地点只作辅助元数据，不能当作可信证明。 |
| `complete_contract` | `contract_id`, `expected_version`, `idempotency_key` | 完成状态、双方最新信用摘要 | 前端要求核身通过、四项验货完成及留证齐全后双方确认；必须校验双方都完成确认。服务端以幂等方式结算积分、归档记录、释放物品占用。 |
| `open_dispute` | `contract_id`, `reason`, `description`, `evidence_ids[]` | 异议 ID、处理状态、举证截止时间 | 当前前端点击异议会本地直接扣 8 分，但页面规则又说异议期间冻结加分、判定后才按规则扣分。这是逻辑冲突；建议提异议先冻结该契约的待结算奖励，不立即扣分，判定后由服务端记入信用流水。 |
| `list_disputes` / `get_dispute` | 分页或 `dispute_id` | 用户可见的异议进度与判定 | 后台判定接口/后台权限属于管理端需求，不要暴露给普通小程序用户。 |

#### 快递履约（同属 `fulfillment`）

前端文案描述寄出前、揽收称重、签收三道快照；同一快递单号不可绑定两份契约；签收后 48 小时验货；首次互寄或信用低于 90 时演示“以物代押”。这些目前均未接真实物流或验货服务。

建议接口：

- `submit_shipping_snapshot(contract_id, side, stage, upload_key, weight_grams?, description?)`：记录寄出前/揽收/签收证据。重量应由可信快递接口或有审计依据的来源产生。
- `bind_tracking_number(contract_id, side, carrier_code, tracking_number)`：服务端验证归属与单号唯一约束；单号是否对双方公开按隐私规则处理。
- `get_tracking_status(contract_id)`：读取物流轨迹与当前验货期限；需接入物流服务或快递查询能力。
- `confirm_package_verification(contract_id, side, result, evidence_ids[])`：记录验真/异议，由服务端推进状态。前端当前“模拟空包/序列号不符/验真通过”不能作为实际判定。
- `raise_shipping_dispute(...)`：签收后 48 小时内申诉，服务端时间为准。

以物代押/验货点如何运营、运输费用和责任、物流服务来源、保险、72 小时人工判定 SLA 都是页面规则文案或演示，并不是现成能力。需要先确定现实履约流程和合作方，再做接口与表结构。

### 5.5 信用分与权限（`member` / 服务端内部）

| 建议接口 | 请求 | 响应/作用 |
|---|---|---|
| `get_credit_summary` | 无（token） | 当前分数、档位、权限、各门槛原因 |
| `list_credit_ledger` | `page`, `page_size` | 最近信用变化流水：变化值、前后分、原因、关联契约/物品、发生时间 |

前端 `utils/score.js` 的演示规则：

- 初始 80，范围 0–100；`<60` 冻结新请求，60–79 仅同城当面，80–89 开放快递和全国，`>=90` 开放高价值物品。
- 奖励：完成且双方好评 +3；双方各上传成功图 +1；连续 5 次成功且无违约 +2（每 30 天最多一次）；补齐实拍/序列号/瑕疵并审核通过 +1（每自然月最多 +2）。
- 惩罚：串单/代交 -10；描述不符且拒不处理 -15；空包/调包 -30；超时未回现场照片 -3；成交后无故爽约 -8。

这些规则目前写在客户端，`score` 页面甚至有切换到任意分数、增减分和重置按钮，仅用于演示。正式版应移除演示操作或只在开发构建开放。客户端只读分数和权限；**不存在普通用户可调用的 `set_score` / `adjust_score` 接口**。由服务端根据已完成交易/管理员判定生成不可重复的信用流水，检查月度/连续履约上限并写审计记录。

### 5.6 社区讨论、投票、评论（`community`）

| 建议接口 | 请求 | 响应/作用 |
|---|---|---|
| `list_posts` | `page`, `page_size`, `sort?` | 帖子、劝买/劝不买数、当前用户投票、评论数、预览评论 |
| `create_post` | `title`, `description?`, `price_text?`, `emoji?` | 新帖与待审核/公开状态 | 标题最多 60 字、补充最多 120 字；“参考价格”是自由文本。需内容安全和敏感联系方式检查。 |
| `list_my_posts` | `page`, `page_size` | 当前用户帖子、审核/公开状态与互动统计 | “我的”页当前进入的是全部帖子流，不是真正的我的帖子列表；需要前端补我的内容筛选/专页或由接口返回 `mine` 标识。 |
| `vote_post` | `post_id`, `choice`（buy/nobuy/none）, `expected_version?` | 当前选择及更新后的票数 | 页面支持切换、再次点击撤销；后台应保存用户唯一投票，投票数由记录聚合或事务维护，不能直接让客户端提交计数。 |
| `list_comments` | `post_id`, `page`, `page_size` | 评论列表与用户立场 | 页面弹层需要全部/分页评论、评论者昵称首字及评论关联投票立场。 |
| `create_comment` | `post_id`, `content` | 新评论及审核状态 | 最多 60 字；页面不允许空评论；立场从当前用户在此帖的投票推导，不接受任意用户伪造。 |
| `report_content` | `target_type`, `target_id`, `reason`, `description?` | 举报受理状态 | 当前页面只提示上线需要内容审核/举报，没有举报按钮或后端流程；正式上线前应补齐审核、屏蔽和处置闭环。 |

社区与换物估值/权限分离。投票选择只代表社区观点，不改变公平性计算，不自动发起交易。新用户和未登录用户是否可读社区内容可公开；发帖、投票、评论当前要求登录。

### 5.7 地区、面交点、个人中心与提醒（`region` / `member`）

| 建议接口 | 请求 | 响应/作用 |
|---|---|---|
| `list_regions` | `parent_id?` | 省/市列表、稳定 ID、拼音索引、是否偏远、是否可快递 | 当前 `utils/region.js` 内置省市和拼音，偏远按省份硬编码为新疆、西藏、青海、宁夏、内蒙古、海南；注释已说明正式规则应按快递区县表维护。可先保留静态城市目录，交易资格需服务端权威判断。 |
| `list_meeting_places` | `city_id`, `nearby?` | 公共安全地点、地址、坐标、营业/安全属性 | 目前 3 个地点、距离和介绍都是静态假数据；若做附近排序须先取得用户定位授权，并说明精度/用途。 |
| `list_my_contracts` | `status?`, `page`, `page_size` | 我的交换记录 | 供“换物记录”、履约中心、纪念册使用。 |
| `get_my_album` | `page`, `page_size` | 已完成契约形成的纪念册条目、信用变化、分享摘要 | 纪念册数据应从已完成契约派生，不能由前端自己添；分享卡不要泄露对方手机号、地址或完整序列号。 |
| `get_member_overview` | 无（token） | 我的物品数、已完成数、社区帖子数、信用与实名状态 | 个人中心目前这些数量/实名/好评率多为硬编码。 |
| `update_notification_preferences` | 订阅类型与开关 | 当前偏好 | 微信订阅消息须由用户授权；后端按模板和业务事件发送，不能把订阅弹窗成功当成消息已送达。 |
| `get_settings` / `update_settings`（可选） | 用户可编辑的通知、隐私偏好 | 更新后的设置 | “设置”页面目前只有 toast，黑名单、账号注销、通知开关均未实现；账号注销还需定义契约/争议未结时的限制及个人数据处置。 |
| `get_platform_rules`（可选） | 规则版本/语言 | 保障条款和信用规则版本 | 保障中心当前完全是静态文案。若后端使用同一份规则，不应出现前后端不同步。 |

### 5.8 二进制接口的具体请求/响应格式

本节给出可落实到 handler DTO 的 v1 协议草案。它不是 JSON REST API：小程序调用生成的 JS client 方法时传入一个 JS 对象，生成器将其编码为 AFast 二进制 payload；成功响应解码回 JS 对象。下列 Rust 风格结构用于定义字段和类型：请求 DTO 派生 `AFastDeserialize + Tag`，响应 DTO 派生 `AFastSerialize + Tag`；同一结构兼作请求/响应时两种序列化 derive 都要有。按项目代码生成约定导出所有出现在 handler 签名中的结构。

#### 通用类型规则

| 约定 | 格式 |
|---|---|
| ID | `String`，由服务端生成的稳定不透明 ID；不要使用 JS number 表示可能超过安全整数范围的数据库 ID。 |
| 金额 | `i64`，单位为人民币分（fen）；例如 ¥12.34 表示 `1234`。估值区间上下限均为分。 |
| 时间 | `i64`，Unix 毫秒，UTC；前端负责格式化为本地时间。 |
| 可空字段 | `Option<T>`。输入结构固定，`None` 表示未提供/无值；对“清空字段”和“保持不变”有区别的更新接口必须另带 `has_xxx: bool`。 |
| 列表 | `Vec<T>`；空列表编码为空数组。 |
| 布尔 | `bool`。权限结果是服务端计算后返回，只读。 |
| 枚举 | DTO 中用 `String` code，不传中文展示文案。收到未知 code 时服务端拒绝写请求；响应同时可提供可读 label。 |
| 分页 | `page` 从 1 开始，`page_size` 范围 1–50；响应返回 `total: i64`、规范化后的 `page/page_size` 和本页 `items`。 |
| 认证 | 二进制 handler 使用 AFast `Custom<AuthCustom>`，不是把 token 塞进业务 Data。JS client 的 `customs.AuthCustom` 回调在每次调用时从小程序登录态存储读取当前 token。公开读取接口不提取此 custom；需要登录的接口未认证返回 `AUTH_REQUIRED`。 |
| 成功 | 响应就是表中所列返回 DTO，不再包一层 `{code,message,data}`。AFast 协议自身负责成功/错误帧。 |
| 错误 | 失败返回 AFast error frame（数字 code + message），不返回伪装成成功的空对象。建议业务 code 分组见本节末尾。 |

基础结构：

```rust,ignore
struct EmptyRequest {}

struct PageRequest {
    page: i32,
    page_size: i32,
}

struct PageInfo {
    page: i32,
    page_size: i32,
    total: i64,
}

#[derive(AFastDeserialize, Tag)]
#[tag("认证上下文")]
struct AuthCustom {
    token: String,
}

struct UserSummary {
    user_id: String,
    nickname: String,
    avatar_url: String,
    real_name_verified: bool,
    credit_score: i32,
    completed_swap_count: i32,
    positive_rate_bp: i32, // 万分比；10000 = 100.00%
}

struct CreditPermission {
    can_trade: bool,
    can_nationwide: bool,
    can_express: bool,
    can_high_value: bool,
}

struct CreditSummary {
    score: i32,             // 0..=100
    tier_code: String,      // A/B/C/D
    tier_name: String,      // 展示用
    permissions: CreditPermission,
    reason: String,
}

struct UploadPlan {
    upload_url: String,     // 一次性签名 URL，不是永久公开 URL
    object_key: String,     // 上传成功后交给业务接口绑定
    upload_token: String,   // 一次性校验值
    expires_at_ms: i64,
    max_bytes: i64,
    content_type: String,
}
```

#### 接口路由表（handler 名 → 输入 DTO → 输出 DTO）

所有路由名为建议名；`Service` 使用小写分组名，生成客户端后调用形式类似 `client.apis.item.list_items(request)`。同一服务内每个 handler 请求和响应都使用各自明确命名的结构体。

| Service.handler | 输入 DTO | 输出 DTO | 认证 |
|---|---|---|---|
| `auth.login_mini` | `LoginMiniRequest` | `LoginMiniResponse` | 公开 |
| `auth.get_me` | `EmptyRequest` | `GetMeResponse` | 登录 |
| `auth.update_profile` | `UpdateProfileRequest` | `UserProfile` | 登录 |
| `auth.bind_phone` | `BindPhoneRequest` | `BindPhoneResponse` | 登录 |
| `auth.get_auth_status` | `EmptyRequest` | `AuthStatusResponse` | 登录 |
| `auth.logout` | `EmptyRequest` | `LogoutResponse` | 登录，可选 |
| `item.list_items` | `ListItemsRequest` | `ListItemsResponse` | 公开 |
| `item.get_item` | `GetItemRequest` | `ItemDetail` | 公开 |
| `item.search_items` | `SearchItemsRequest` | `ListItemsResponse` | 公开 |
| `item.create_upload_plan` | `CreateUploadPlanRequest` | `UploadPlan` | 登录 |
| `item.publish_item` | `PublishItemRequest` | `PublishItemResponse` | 登录 |
| `item.list_my_items` | `ListMyItemsRequest` | `ListMyItemsResponse` | 登录 |
| `item.update_item` | `UpdateItemRequest` | `ItemDetail` | 登录 |
| `item.change_item_status` | `ChangeItemStatusRequest` | `ChangeItemStatusResponse` | 登录 |
| `item.set_favorite` | `SetFavoriteRequest` | `SetFavoriteResponse` | 登录 |
| `item.set_price_alert` | `SetPriceAlertRequest` | `SetPriceAlertResponse` | 登录 |
| `swap.preview_swap` | `PreviewSwapRequest` | `PreviewSwapResponse` | 登录 |
| `swap.create_swap_request` | `CreateSwapRequest` | `CreateSwapResponse` | 登录 |
| `swap.list_swap_requests` | `ListSwapRequestsRequest` | `ListSwapRequestsResponse` | 登录 |
| `swap.preview_matches` | `PreviewMatchesRequest` | `PreviewMatchesResponse` | 登录 |
| `swap.respond_swap_request` | `RespondSwapRequest` | `RespondSwapResponse` | 登录 |
| `swap.cancel_swap_request` | `CancelSwapRequest` | `CancelSwapResponse` | 登录 |
| `swap.get_contract` | `GetContractRequest` | `ContractDetail` | 登录且为契约参与方 |
| `swap.list_my_contracts` | `ListMyContractsRequest` | `ListMyContractsResponse` | 登录 |
| `fulfillment.get_fulfillment` | `GetFulfillmentRequest` | `FulfillmentDetail` | 登录且为契约参与方 |
| `fulfillment.set_fulfillment_mode` | `SetFulfillmentModeRequest` | `FulfillmentDetail` | 登录且为契约参与方 |
| `fulfillment.set_meeting_place` | `SetMeetingPlaceRequest` | `MeetingPlan` | 登录且为契约参与方 |
| `fulfillment.create_verification_code` | `CreateVerificationCodeRequest` | `VerificationCodeResponse` | 登录且为契约参与方 |
| `fulfillment.verify_meeting_code` | `VerifyMeetingCodeRequest` | `VerifyMeetingCodeResponse` | 登录且为契约参与方 |
| `fulfillment.submit_inspection` | `SubmitInspectionRequest` | `InspectionResult` | 登录且为契约参与方 |
| `fulfillment.upload_evidence` | `UploadEvidenceRequest` | `EvidenceRecord` | 登录且为契约参与方 |
| `fulfillment.complete_contract` | `CompleteContractRequest` | `CompleteContractResponse` | 登录且为契约参与方 |
| `fulfillment.open_dispute` | `OpenDisputeRequest` | `DisputeSummary` | 登录且为契约参与方 |
| `fulfillment.list_disputes` | `ListDisputesRequest` | `ListDisputesResponse` | 登录 |
| `fulfillment.get_dispute` | `GetDisputeRequest` | `DisputeDetail` | 登录且为异议参与方 |
| `fulfillment.bind_tracking_number` | `BindTrackingNumberRequest` | `ShipmentDetail` | 登录且为契约参与方 |
| `fulfillment.submit_shipping_snapshot` | `SubmitShippingSnapshotRequest` | `ShippingSnapshot` | 登录且为契约参与方 |
| `fulfillment.get_tracking_status` | `GetTrackingStatusRequest` | `ShipmentDetail` | 登录且为契约参与方 |
| `fulfillment.confirm_package_verification` | `ConfirmPackageVerificationRequest` | `PackageVerificationResponse` | 登录且为契约参与方 |
| `fulfillment.raise_shipping_dispute` | `RaiseShippingDisputeRequest` | `DisputeSummary` | 登录且为契约参与方 |
| `member.get_credit_summary` | `EmptyRequest` | `CreditSummary` | 登录 |
| `member.list_credit_ledger` | `ListCreditLedgerRequest` | `ListCreditLedgerResponse` | 登录 |
| `community.list_posts` | `ListPostsRequest` | `ListPostsResponse` | 公开；登录时附带我的投票 |
| `community.create_post` | `CreatePostRequest` | `CreatePostResponse` | 登录 |
| `community.list_my_posts` | `ListMyPostsRequest` | `ListPostsResponse` | 登录 |
| `community.vote_post` | `VotePostRequest` | `VotePostResponse` | 登录 |
| `community.list_comments` | `ListCommentsRequest` | `ListCommentsResponse` | 公开 |
| `community.create_comment` | `CreateCommentRequest` | `CreateCommentResponse` | 登录 |
| `community.report_content` | `ReportContentRequest` | `ReportContentResponse` | 登录 |
| `region.list_regions` | `ListRegionsRequest` | `ListRegionsResponse` | 公开 |
| `region.list_meeting_places` | `ListMeetingPlacesRequest` | `ListMeetingPlacesResponse` | 公开 |
| `member.get_member_overview` | `EmptyRequest` | `MemberOverview` | 登录 |
| `member.get_my_album` | `GetAlbumRequest` | `GetAlbumResponse` | 登录 |
| `member.get_notification_preferences` | `EmptyRequest` | `NotificationPreferences` | 登录 |
| `member.update_notification_preferences` | `UpdateNotificationPreferencesRequest` | `NotificationPreferences` | 登录 |
| `member.get_settings` | `EmptyRequest` | `MemberSettings` | 登录，可选 |
| `member.update_settings` | `UpdateMemberSettingsRequest` | `MemberSettings` | 登录，可选 |
| `member.get_platform_rules` | `GetPlatformRulesRequest` | `GetPlatformRulesResponse` | 公开，可选 |

以下各节给出这些 DTO 的字段定义。表中 `Option<T>` 字段可以为空；未标为可选的字段必须提供。结构中的字符串均有服务端长度上限，后端应校验，不应只依赖客户端表单限制。

#### 登录与用户 DTO

```rust,ignore
struct LoginMiniRequest {
    code: String,                   // wx.login 返回的一次性 code
    nickname: String,               // wx.getUserProfile 授权返回的微信昵称
    agreement_version: String,      // 用户同意的协议版本
}
struct LoginMiniResponse {
    token: String,
    expires_at_ms: i64,
    is_new_user: bool,
    user: UserProfile,
}
struct UserProfile {
    user_id: String,
    nickname: String,
    avatar_url: String,
    phone_masked: String,           // 未绑定为空字符串
    phone_bound: bool,
    real_name_status: String,       // not_started/pending/verified/failed
}
struct GetMeResponse { user: UserProfile }
struct UpdateProfileRequest {
    has_nickname: bool, nickname: String,
    has_avatar_upload_key: bool, avatar_upload_key: String,
}
struct BindPhoneRequest {
    code: String,                   // wx.getPhoneNumber 返回的一次性 code
    confirm_transfer: bool,         // 用户确认从旧账号解绑并转移时为 true
}
struct BindPhoneResponse {
    phone_masked: String,
    phone_bound: bool,
    transferred: bool,              // 是否已从旧账号转移
}
struct AuthStatusResponse {
    phone_bound: bool,
    phone_masked: String,
    real_name_status: String,
    can_create_swap_request: bool,
    blocked_reason: String,
}
struct LogoutResponse { revoked: bool }
```

受保护 handler 的 Rust 参数约定示例：

```rust,ignore
#[handler(desc("读取当前用户"))]
async fn get_me(
    Custom(auth): Custom<AuthCustom>,
) -> afast::Result<GetMeResponse> {
    let user_id = verify_access_token(&auth.token)?;
    // ...
}
```

`AuthCustom` 需派生 `AFastDeserialize` 和 `Tag`，使二进制 handler 能解码且生成器能识别它。生成 JS client 时传入 `customs: { AuthCustom: async () => ({ token: readCurrentToken() }) }`；不要把 token 静态捕获在登录前创建的旧 client 中，token 刷新/退出时回调应立即返回最新值或空值。客户端不得把 custom 内容写入业务日志。

```javascript
const client = new ItemClient({
  host: API_HOST,
  port: 6080,
  tls: true,
  transport: 'wxrequest',
  customs: {
    AuthCustom: () => ({ token: wx.getStorageSync('hh_access_token') || '' }),
  },
});
const page = await client.apis.list_items({
  city_id: 'city_hangzhou',
  nationwide: false,
  category_code: '',
  keyword: '',
  page: 1,
  page_size: 20,
  sort_code: 'newest',
});
```

真实开发/部署地址必须使用 HTTPS 和小程序 request 合法域名；上例 `tls: true` 是部署建议。开发环境若使用明文 HTTP，只限本机/开发工具调试，不能作为线上配置。

#### 物品、上传、估值 DTO

```rust,ignore
struct ItemSummary {
    item_id: String,
    title: String,
    category_code: String,          // digital/toy/book/home/clothing/other
    condition_code: String,         // new/like_new/excellent/good/fair/poor
    value_low_fen: i64,
    value_high_fen: i64,
    valuation_source: String,
    valuation_expires_at_ms: i64,
    city_id: String,
    city_name: String,
    remote_only_face_to_face: bool,
    fulfillment_modes: Vec<String>, // face/express
    cover_url: String,
    owner: UserSummary,
    item_status: String,            // active/pending_review/paused/locked/swapped/rejected
}
struct ItemImage {
    image_id: String,
    url: String,
    sort_order: i32,
}
struct ItemDetail {
    summary: ItemSummary,
    description: String,
    serial_masked: String,
    serial_verified: bool,
    images: Vec<ItemImage>,
    created_at_ms: i64,
    updated_at_ms: i64,
    can_edit: bool,
    is_favorite: bool,
    price_alert_enabled: bool,
}
struct ListItemsRequest {
    city_id: String,                // 空字符串表示使用 nationwide
    nationwide: bool,
    category_code: String,          // 空字符串表示全部
    keyword: String,                // 空字符串表示不搜索
    page: i32,
    page_size: i32,
    sort_code: String,              // newest/matching
}
struct ListItemsResponse {
    items: Vec<ItemSummary>,
    page: i32, page_size: i32, total: i64,
}
struct GetItemRequest { item_id: String }
struct SearchItemsRequest {
    keyword: String,
    city_id: String,
    nationwide: bool,
    category_code: String,
    page: i32, page_size: i32,
}
struct CreateUploadPlanRequest {
    purpose: String,                // item_image/evidence/avatar/valuation_proof
    file_name: String,
    content_type: String,           // image/jpeg/image/png/image/webp
    size_bytes: i64,
}
struct PublishItemRequest {
    title: String,                  // 1..=30 字符
    category_code: String,
    condition_code: String,
    description: String,            // 至多 120 字符
    serial_number: String,          // 没填则空；仅经 HTTPS/TLS 传输，服务端计算后四位并加密保存
    valuation_method: String,       // platform/market_screenshot/manual
    value_low_fen: i64,
    value_high_fen: i64,
    city_id: String,
    fulfillment_modes: Vec<String>,
    photo_upload_tokens: Vec<String>, // 1..=9；上传完成后的一次性 token
}
struct PublishItemResponse { item: ItemDetail }
struct ListMyItemsRequest {
    status_code: String,            // 空表示全部；active/paused/locked/swapped
    page: i32, page_size: i32,
}
struct ListMyItemsResponse {
    items: Vec<ItemSummary>,
    page: i32, page_size: i32, total: i64,
}
struct UpdateItemRequest {
    item_id: String,
    expected_version: i64,
    has_title: bool, title: String,
    has_description: bool, description: String,
    has_value_range: bool, value_low_fen: i64, value_high_fen: i64,
    has_city: bool, city_id: String,
    has_serial_number: bool, serial_number: String,
}
struct ChangeItemStatusRequest {
    item_id: String,
    action: String,                 // pause/resume
    expected_version: i64,
}
struct ChangeItemStatusResponse { item_id: String, item_status: String, version: i64 }
struct SetFavoriteRequest { item_id: String, favorited: bool }
struct SetFavoriteResponse { item_id: String, favorited: bool }
struct SetPriceAlertRequest { item_id: String, enabled: bool }
struct SetPriceAlertResponse { item_id: String, enabled: bool }
```

上传流程：先调用 `create_upload_plan`，客户端根据返回 URL 直接上传文件；上传成功后将 `upload_token` 放入发布/证据 DTO。后端验证 token 的用户、用途、过期时间和对象大小后，才能把对象绑定为正式图片。不能把 `wx.chooseMedia` 的临时路径或客户端自报的永久 URL 当作已上传文件。

#### 换物请求与契约 DTO

```rust,ignore
struct SwapFairness {
    allowed: bool,
    has_value_overlap: bool,
    center_difference_bp: i32,      // 万分比，2000 = 20%
    fulfillment_modes: Vec<String>,
    blocked_reason_code: String,    // 空串表示通过
}
struct PreviewSwapRequest { target_item_id: String, offered_item_id: String }
struct PreviewSwapResponse {
    target_item: ItemSummary,
    offered_item: ItemSummary,
    fairness: SwapFairness,
}
struct CreateSwapRequest {
    target_item_id: String,
    offered_item_id: String,
    message: String,                // 最多 80 字
    idempotency_key: String,        // 客户端一次提交生成 UUID，重试复用
}
struct SwapRequestSummary {
    request_id: String,
    direction: String,              // incoming/outgoing
    status: String,                 // pending/accepted/rejected/cancelled/expired
    other_user: UserSummary,
    target_item: ItemSummary,
    offered_item: ItemSummary,
    message: String,
    created_at_ms: i64,
    cancel_until_ms: i64,           // 不能撤回时为 0
    can_cancel: bool,
    can_respond: bool,
    contract_id: String,            // 尚未接受时为空串
    version: i64,
}
struct CreateSwapResponse { request: SwapRequestSummary }
struct ListSwapRequestsRequest {
    direction: String,              // all/incoming/outgoing
    status_code: String,            // 空表示全部
    page: i32, page_size: i32,
}
struct ListSwapRequestsResponse {
    requests: Vec<SwapRequestSummary>,
    page: i32, page_size: i32, total: i64,
}
struct PreviewMatchesRequest { city_id: String, page: i32, page_size: i32 }
struct MatchCandidate {
    item: ItemSummary,
    offered_item: ItemSummary,
    matching_score: i32,            // 0..99 的排序分，不是成功概率
    overlap_bp: i32,
    tags: Vec<String>,              // same_city/high_credit/value_overlap
}
struct PreviewMatchesResponse {
    candidates: Vec<MatchCandidate>,
    page: i32, page_size: i32, total: i64,
}
struct RespondSwapRequest {
    request_id: String,
    decision: String,               // accept/reject
    expected_version: i64,
    idempotency_key: String,
}
struct RespondSwapResponse {
    request: SwapRequestSummary,
    contract_id: String,            // reject 时为空串
}
struct CancelSwapRequest { request_id: String, expected_version: i64 }
struct CancelSwapResponse { request: SwapRequestSummary }
struct GetContractRequest { contract_id: String }
struct ListMyContractsRequest {
    status_code: String,            // 空/all/awaiting_fulfillment/fulfilling/completed/disputed
    page: i32, page_size: i32,
}
struct ContractItemSnapshot {
    item_id: String,
    owner_user_id: String,
    title: String,
    category_code: String,
    condition_code: String,
    description: String,
    serial_masked: String,
    value_low_fen: i64,
    value_high_fen: i64,
    city_id: String,
    city_name: String,
    image_urls: Vec<String>,
}
struct ContractDetail {
    contract_id: String,
    request_id: String,
    status: String,
    my_item: ContractItemSnapshot,
    other_item: ContractItemSnapshot,
    other_user: UserSummary,
    allowed_fulfillment_modes: Vec<String>,
    fulfillment_mode: String,
    created_at_ms: i64,
    completed_at_ms: i64,           // 未完成为 0
    version: i64,
}
struct ListMyContractsResponse {
    contracts: Vec<ContractDetail>,
    page: i32, page_size: i32, total: i64,
}
```

#### 履约、面交、证据与争议 DTO

```rust,ignore
struct GetFulfillmentRequest { contract_id: String }
struct SetFulfillmentModeRequest {
    contract_id: String,
    mode: String,                   // face/express
    expected_version: i64,
}
struct MeetingPlace {
    place_id: String,
    name: String,
    place_type: String,             // police_station/mall/convenience_store/other
    address: String,
    latitude: f64,
    longitude: f64,
    distance_meters: i32,           // 无定位/不计算时为 -1
    safety_tags: Vec<String>,
    description: String,
}
struct SetMeetingPlaceRequest {
    contract_id: String,
    place_id: String,
    scheduled_at_ms: i64,           // 未确定填 0
    check_in_enabled: bool,
    late_reminder_enabled: bool,
    emergency_location_share_enabled: bool,
    expected_version: i64,
}
struct MeetingPlan {
    place: MeetingPlace,
    scheduled_at_ms: i64,
    check_in_enabled: bool,
    late_reminder_enabled: bool,
    emergency_location_share_enabled: bool,
}
struct CreateVerificationCodeRequest { contract_id: String }
struct VerificationCodeResponse {
    code_token: String,             // opaque 一次性内容，客户端生成二维码/网格
    expires_at_ms: i64,
    ttl_seconds: i32,
}
struct VerifyMeetingCodeRequest { contract_id: String, code_token: String }
struct VerifyMeetingCodeResponse {
    verified: bool,
    verified_user_id: String,
    verified_at_ms: i64,
    reason_code: String,            // verified/expired/used/wrong_contract/not_member
}
struct InspectionCheck {
    check_code: String,             // condition/flaws/serial/accessories
    passed: bool,
    note: String,
}
struct SubmitInspectionRequest {
    contract_id: String,
    checks: Vec<InspectionCheck>,   // 恰好四项；服务端校验当前用户已完成本人核验
    expected_version: i64,
}
struct InspectionResult {
    inspection_id: String,
    user_id: String,
    passed: bool,
    checks: Vec<InspectionCheck>,
    submitted_at_ms: i64,
}
struct UploadEvidenceRequest {
    contract_id: String,
    evidence_type: String,          // face_exchange/sender_before/parcel_pickup/delivery/other
    upload_token: String,
    note: String,
}
struct EvidenceRecord {
    evidence_id: String,
    evidence_type: String,
    url: String,
    uploader_user_id: String,
    created_at_ms: i64,
}
struct FulfillmentDetail {
    contract: ContractDetail,
    mode: String,
    meeting_plan: Option<MeetingPlan>,
    my_verification_passed: bool,
    other_verification_passed: bool,
    my_inspection: Option<InspectionResult>,
    other_inspection: Option<InspectionResult>,
    evidence: Vec<EvidenceRecord>,
    shipment: Option<ShipmentDetail>,
    can_complete: bool,
    can_dispute: bool,
    version: i64,
}
struct CompleteContractRequest {
    contract_id: String,
    expected_version: i64,
    idempotency_key: String,
}
struct CompleteContractResponse {
    contract_id: String,
    status: String,
    my_credit: CreditSummary,
    completed_at_ms: i64,
}
struct OpenDisputeRequest {
    contract_id: String,
    reason_code: String,            // item_mismatch/no_show/empty_parcel/wrong_serial/other
    description: String,
    evidence_ids: Vec<String>,
}
struct DisputeSummary {
    dispute_id: String,
    contract_id: String,
    status: String,                 // submitted/evidence_requested/reviewing/resolved/rejected
    reason_code: String,
    created_at_ms: i64,
    evidence_deadline_ms: i64,
}
struct ListDisputesRequest { page: i32, page_size: i32 }
struct ListDisputesResponse {
    disputes: Vec<DisputeSummary>,
    page: i32, page_size: i32, total: i64,
}
struct GetDisputeRequest { dispute_id: String }
struct DisputeDetail {
    summary: DisputeSummary,
    description: String,
    evidence: Vec<EvidenceRecord>,
    resolution_code: String,
    resolution_note: String,
    resolved_at_ms: i64,
}
struct BindTrackingNumberRequest {
    contract_id: String,
    carrier_code: String,
    tracking_number: String,
    expected_version: i64,
}
struct SubmitShippingSnapshotRequest {
    contract_id: String,
    stage: String,                  // sender_before/picked_up/delivered
    upload_token: String,
    weight_grams: i64,              // 0 表示没有可信重量；不能接受客户端自报作为验证依据
    carrier_event_id: String,
}
struct ShippingSnapshot {
    snapshot_id: String,
    stage: String,
    evidence: EvidenceRecord,
    weight_grams: i64,
    recorded_at_ms: i64,
    source: String,                 // carrier/platform/user
}
struct GetTrackingStatusRequest { contract_id: String }
struct ShipmentDetail {
    shipment_id: String,
    carrier_code: String,
    tracking_number_masked: String,
    shipment_status: String,
    tracking_events: Vec<ShippingSnapshot>,
    inspection_deadline_ms: i64,
}
struct ConfirmPackageVerificationRequest {
    contract_id: String,
    result: String,                 // pass/issue
    reason_code: String,
    evidence_ids: Vec<String>,
    expected_version: i64,
}
struct PackageVerificationResponse {
    accepted: bool,
    contract_status: String,
    reason_code: String,
}
struct RaiseShippingDisputeRequest {
    contract_id: String,
    reason_code: String,
    description: String,
    evidence_ids: Vec<String>,
}
```

`FulfillmentDetail` 中当前用户侧的 `my_*` 字段仅返回调用者自己的记录；另一方信息只返回履约所需状态，不返回其私人资料。`Option<T>` 为 `None` 表示流程尚未产生该记录。

#### 信用 DTO

```rust,ignore
struct ListCreditLedgerRequest { page: i32, page_size: i32 }
struct CreditLedgerEntry {
    ledger_id: String,
    delta: i32,
    score_before: i32,
    score_after: i32,
    reason_code: String,
    reason_text: String,
    reference_type: String,         // contract/item/manual_review
    reference_id: String,
    created_at_ms: i64,
}
struct ListCreditLedgerResponse {
    entries: Vec<CreditLedgerEntry>,
    page: i32, page_size: i32, total: i64,
}
```

`get_credit_summary` 的响应即通用 `CreditSummary`。不定义普通用户写入信用分的 handler；信用变化由真实业务事件和受审计的后台操作触发。

#### 社区 DTO

```rust,ignore
struct PostSummary {
    post_id: String,
    author: UserSummary,
    title: String,
    description: String,
    price_text: String,
    emoji: String,
    buy_count: i64,
    nobuy_count: i64,
    my_vote: String,                // 空/buy/nobuy
    comment_count: i64,
    preview_comments: Vec<CommentSummary>,
    moderation_status: String,      // visible/pending/hidden
    created_at_ms: i64,
}
struct CommentSummary {
    comment_id: String,
    author: UserSummary,
    stance: String,                 // 空/buy/nobuy
    content: String,
    created_at_ms: i64,
}
struct ListPostsRequest { page: i32, page_size: i32, sort_code: String }
struct ListPostsResponse {
    posts: Vec<PostSummary>,
    page: i32, page_size: i32, total: i64,
}
struct CreatePostRequest {
    title: String,                  // 1..=60 字
    description: String,            // 最多 120 字，可空
    price_text: String,             // 最多 40 字，可空
    emoji: String,
}
struct CreatePostResponse { post: PostSummary }
struct ListMyPostsRequest {
    page: i32, page_size: i32, status_code: String,
}
struct VotePostRequest {
    post_id: String,
    choice: String,                 // buy/nobuy/none；none 表示撤销
}
struct VotePostResponse {
    post_id: String,
    my_vote: String,
    buy_count: i64,
    nobuy_count: i64,
}
struct ListCommentsRequest { post_id: String, page: i32, page_size: i32 }
struct ListCommentsResponse {
    comments: Vec<CommentSummary>,
    page: i32, page_size: i32, total: i64,
}
struct CreateCommentRequest { post_id: String, content: String } // 1..=60 字
struct CreateCommentResponse { comment: CommentSummary }
struct ReportContentRequest {
    target_type: String,            // post/comment
    target_id: String,
    reason_code: String,            // spam/abuse/contact_info/fraud/other
    description: String,
}
struct ReportContentResponse { report_id: String, status: String }
```

#### 地区、面交点、个人中心与设置 DTO

```rust,ignore
struct ListRegionsRequest {
    parent_region_id: String,       // 空表示省级
}
struct RegionEntry {
    region_id: String,
    parent_region_id: String,
    name: String,
    level: String,                  // province/city/district
    pinyin: String,
    remote_only_face_to_face: bool,
    express_available: bool,
}
struct ListRegionsResponse { regions: Vec<RegionEntry> }
struct ListMeetingPlacesRequest {
    city_id: String,
    latitude: f64, longitude: f64,  // 不请求定位则均为 0
    radius_meters: i32,
    page: i32, page_size: i32,
}
struct ListMeetingPlacesResponse {
    places: Vec<MeetingPlace>,
    page: i32, page_size: i32, total: i64,
}
struct MemberOverview {
    user: UserProfile,
    active_item_count: i32,
    completed_swap_count: i32,
    open_request_count: i32,
    post_count: i32,
}
struct GetAlbumRequest { page: i32, page_size: i32 }
struct AlbumEntry {
    contract_id: String,
    received_item_title: String,
    received_item_cover_url: String,
    given_item_title: String,
    city_name: String,
    fulfillment_mode: String,
    completed_at_ms: i64,
    credit_delta: i32,
}
struct GetAlbumResponse {
    entries: Vec<AlbumEntry>,
    page: i32, page_size: i32, total: i64,
    completed_swap_count: i32,
    dispute_count: i32,
    credit_score: i32,
}
struct NotificationPreferences {
    swap_request_enabled: bool,
    fulfillment_reminder_enabled: bool,
    price_alert_enabled: bool,
}
struct UpdateNotificationPreferencesRequest {
    swap_request_enabled: bool,
    fulfillment_reminder_enabled: bool,
    price_alert_enabled: bool,
    subscription_template_codes: Vec<String>, // 仅提交微信授权成功的模板 code
}
struct MemberSettings {
    block_user_ids: Vec<String>,
    allow_emergency_location_share: bool,
}
struct UpdateMemberSettingsRequest {
    block_user_ids: Vec<String>,
    allow_emergency_location_share: bool,
}
struct GetPlatformRulesRequest { locale: String, rule_version: String }
struct PlatformRule {
    rule_code: String,
    title: String,
    content: String,
    sort_order: i32,
}
struct GetPlatformRulesResponse {
    rule_version: String,
    rules: Vec<PlatformRule>,
}
```

#### 示例：一次创建请求的完整调用

JS 业务代码调用生成器，不自行拼装二进制、不直接 `wx.request`：

```javascript
const result = await swapClient.apis.create_swap_request({
  target_item_id: 'item_01J...',
  offered_item_id: 'item_01K...',
  message: '周末可以在商场服务台面交吗？',
  idempotency_key: 'f4baf12a-...',
});
// result: {
//   request: {
//     request_id: 'swapreq_01M...',
//     direction: 'outgoing',
//     status: 'pending',
//     cancel_until_ms: 1795500000000,
//     can_cancel: true,
//     ...
//   }
// }
```

响应中省略号代表该 DTO 定义的其余必需字段，真实代码应以生成客户端的 JSDoc 类型为准。若 handler 要返回错误（如目标物品已锁定），client promise reject，错误含 `code` 和 `message`；前端按业务 code 展示提示并允许用户刷新，不应将错误响应当作成功数据读取。

#### 建议业务错误码

错误码最终须符合 AFaster/AFast 的 error code 约束，以下是稳定语义名草案，数字分配时避免与框架保留码冲突：

| 错误语义 | 触发场景 |
|---|---|
| `AUTH_REQUIRED` / `AUTH_INVALID` | 无 token、token 过期/签名错误 |
| `ACCOUNT_DISABLED` | 账号封禁或注销中 |
| `PROFILE_REQUIRED` | 未满足产品要求的手机号/实名状态 |
| `VALIDATION_FAILED` | 字段格式、长度、枚举、金额区间错误 |
| `NOT_FOUND` | 资源不存在或对调用者不可见 |
| `FORBIDDEN` | 非资源所有者/非契约参与方 |
| `ITEM_NOT_AVAILABLE` | 物品下架、审核未通过或已占用 |
| `CREDIT_GATE_FAILED` | 信用分不满足交易、跨城、快递或高价值门槛 |
| `SWAP_NOT_FAIR` | 估值规则不通过 |
| `INVALID_STATE` | 请求/契约/履约当前状态不允许该动作 |
| `VERSION_CONFLICT` | `expected_version` 与数据库版本不一致 |
| `IDEMPOTENCY_CONFLICT` | 相同幂等键对应不同命令内容 |
| `CODE_EXPIRED` / `CODE_ALREADY_USED` | 核验码过期或已消费 |
| `UPLOAD_TOKEN_INVALID` | 上传凭证过期、用途或用户不匹配 |
| `RATE_LIMITED` | 登录、发帖、评论、扫码等操作触发限流 |

AFast 二进制响应失败帧携带数值 code 与 message；建议维护一个服务端错误码到上述语义的映射，message 供调试/展示，客户端业务分支优先判断 code，不要解析 message 文本。

## 6. 15 个页面逐页对照

| 页面 | 页面现在做什么 | 应调用/需要的数据 | 当前差距与注意点 |
|---|---|---|---|
| `home` 首页换物 | 按类别和城市过滤卡片；显示估值、成色、卖家城市、信用门槛和匹配入口 | `item.list_items`（分页、category/city/keyword）；`member.get_credit_summary`；匹配结果可复用 `item` 查询或另做 `swap.preview_matches` | 现在过滤 10 条静态 `items`；搜索只有 toast；“请求数”标题实际展示物品数。不可把完整物品池和卖家隐私给所有用户。 |
| `detail` 物品详情 | 展示物品、估值来源/有效期、瑕疵、地区、履约方式、卖家、脱敏序列号；收藏/提醒；发起交换 | `item.get_item`；`item.add_favorite/remove_favorite`；`item.set_price_alert`；点击发起进入请求预览 | 页面从本地数组按 ID 查找，找不到还回退显示第一件样例；卖家实名/换物次数/好评率是固定文案。估值区间和交易门槛必须后端重算。 |
| `request` 发起换物 | 选择本人一件物品，计算公平阈值，填 80 字留言，确认后发送 | `item.list_my_items`；`swap.preview_swap`；`swap.create_swap_request` | 本人物品目前来自 4 件硬编码 `myItems`；仅本地添加请求。服务端重新确认所有权、状态、估值和信用权限，且快照最终在接受时锁定。 |
| `inbox` 换物请求 | 全部/收到/发出筛选；显示请求状态；撤回和已接受请求的履约入口 | `swap.list_swap_requests`；`swap.cancel_swap_request`；`swap.respond_swap_request`；`swap.get_contract` | **重要 UI 缺口：**当前模板只有撤回、去履约、看物品，没有接受/拒绝按钮；“收到的待处理请求”无法被处理。后端接口应先定义，前端后续要加操作与确认。 |
| `publish` 发布物品 | 最多 9 张图；标题、品类、成色、瑕疵、序列号、估值方式/区间、地区、履约方式；高价值权限门槛 | 图片上传流程；`item.publish_item`；`region.list_regions`；`member.get_credit_summary` | 只把临时图片路径、表单值放进内存；估值方式选择没有分支实现，行情截图/OCR 不存在；当前只手输区间。前端序列号正则不可信。 |
| `vote` 买不买 | 帖子流、投劝买/劝不买/撤票、评论列表弹层、发帖、评论 | `community.list_posts`；`community.vote_post`；`community.list_comments/create_comment`；`community.create_post` | 所有票、评论、帖子只改内存。没有举报入口/审核 UI；正式开放 UGC 前需加内容治理和联系方式拦截。 |
| `trust` 保障中心 | 展示 8 条规则：不碰钱、双向确认、估值、面交、快递、违约、隐私、申诉 | 可先静态，不强制 API；规则动态维护时用 `member.get_platform_rules` | 文案不是已实现能力。尤其以物代押、短信/通知、实名、争议 72 小时处理与资金不托管声明应在产品确定可兑现后再对外承诺。 |
| `profile` 我的 | 登录入口、信用卡片、雷达、纪念册、面交安全包、常用语；我的物品/记录/帖子/设置 | `auth.get_me`；`member.get_member_overview`；`item.list_my_items`；`swap.list_my_contracts`；`community.list_my_posts`；设置/通知接口 | “我的物品”“换物记录”“设置”当前只是 toast；帖子数量含硬编码 +2；退出登录目前只清本地数据。 |
| `score` 信用分 | 档位、权限、流水、奖惩规则；演示切档/加减分/重置 | `member.get_credit_summary`；`member.list_credit_ledger` | 演示按钮可以任意改本地信用分，必须移除/隔离；后端不提供普通用户改分接口。扣分、奖励均需关联真实事件和审计。 |
| `fulfill` 履约中心 | 契约快照；面交核验码、扫码、4 项验货、现场图、完成/异议；快递快照、单号、验真 | `fulfillment.get_fulfillment`; 面交地点/码/验货/证据/完成/异议接口；快递快照/运单/物流接口 | 当前二维码是随机绘制；模拟扫码可直接通过；现场图只保存在临时路径；完成/分数和申诉结果都本地更新；快递按钮只是模拟结果。 |
| `region` 选择交易地区 | 337 城列表、拼音索引、偏远标记、全国选项；分数低于 80 禁全国 | `region.list_regions`；确认选择只需写客户端偏好，获取列表时把 city_id 传给 `item.list_items` | 城市目录现在静态。服务端必须验证城市 ID/信用/全国权限/实际快递范围，不能因为前端隐藏全国选项就认为用户没传全国。 |
| `meet` 面交安全包 | 选公共场所、到场打卡/迟到提醒/紧急联系人地点共享开关、见面卡、导航 | `region.list_meeting_places`；`fulfillment.set_meeting_place`；未来增加 `check_in`/联系人授权和提醒接口 | 地点/距离是假数据；导航仅 toast；开关只写全局内存，不进入契约。位置与紧急联系人属于敏感信息，需明确授权、时效和双方可见边界。 |
| `login` 微信登录/账号 | 微信登录、微信昵称授权、手机号授权、同意协议、退出 | `auth.login_mini`；`auth.bind_phone`；`auth.logout` | 微信登录和手机号绑定已接后端；手机号冲突时由用户确认后重新授权，并在后端事务中转移绑定；实名核身入口位于登录后的“账号与登录”页，但认证服务尚未接入。 |
| `album` 换物纪念册 | 已完成交换条目、信用、统计分享卡 | `member.get_my_album` / `swap.list_my_contracts` | 现在静态三条样例；分享文案和次数不等于服务端确认的成功交易数。 |
| `match` 智能换物雷达 | 将本人的物品与在架物品两两比较，展示估值重叠、同城、对方信用标签和推荐分 | `swap.preview_matches` 或 `item.list_items` + 服务端匹配逻辑 | 当前算法在 `app.matches()` 用样例数据，最高取 5 条；页面显示“换得成概率”但不是实际概率。可先作为排序推荐，不能当成保证成功率或绕过正式公平校验。 |

## 7. 建议的最小数据对象

这是理解关系用的初始实体清单，不要求一次全部建完：

1. **User / WeChatIdentity**：用户资料、openid 唯一映射、账号状态、实名/手机号状态；微信 `session_key` 不返回客户端，除业务必需外不长期保存。
2. **Item / ItemImage**：物主、状态、品类、成色、瑕疵、估值区间与来源、城市、履约方式、序列号脱敏/加密值、图片排序。
3. **SwapRequest**：发起人、接收人、目标物品、本人出物、留言、状态、冷静期截止、版本号。
4. **SwapContract / ContractItemSnapshot**：请求接受后固定双方信息和物品快照、估值快照、交易方式与状态时间线。
5. **Fulfillment / Meeting / Verification / Evidence / Shipment**：履约方式、一次性码使用记录、验货结果、证据文件、面交安排、运单和快递节点。
6. **CreditLedger**：用户、变化值、前后分、唯一业务事件键、规则版本、关联契约、人工判定人（如有）、创建时间。
7. **DiscussionPost / Vote / Comment / Report**：帖子、一个用户一帖一票、评论、举报、审核状态与审计。
8. **Favorite / NotificationPreference / Outbox**：收藏、提醒授权/偏好和待发送通知。异步通知可用 outbox 避免“业务已成功但消息记录丢失”。

物品 ID 和用户 ID 关联；请求接受后形成契约快照，不能只在契约里保存物品当前表的引用，否则后来编辑物品会改变历史交易事实。

## 8. 不应由客户端决定的事项

- 真实用户身份、openid 绑定、token 有效性、用户状态、物主权限。
- 信用分、实名状态、角色、信用门槛、全国/快递/高价值资格。
- 估值公平判断、物品是否可用、是否被其他请求占用、请求过期时间。
- 请求是否可接受/撤回，契约是否生效、核验码是否有效或已使用。
- 物品快照、证据归档时间、争议结论、处罚或奖励、契约完成。
- 投票数、评论数、收藏总数、卖家成功次数与好评率。

客户端字段用于展示和发起意图；所有关键条件应从服务端数据库状态重新加载并校验。

## 9. 产品/实现待确认问题

开始建核心数据表前，建议和产品/原作者确认：

1. 接受请求的双方实名/手机号要求，以及未实名用户能否先发请求。
2. 对方接受时是否自动生成契约，还是还要双方二次确认地点/履约方式。
3. 两件物品出现多个并发请求时，是接受一个后自动拒绝其他，还是先锁定后限时确认。
4. 估值区间的币种/单位、重叠边界（`hi == lo` 算不算重叠）、中心价差计算口径和高价值判断用中心值还是上限。
5. 地区是省/市/区县哪一级；六个“偏远省份仅面交”的硬编码是否准确，实际快递范围由谁维护。
6. 物品行情截图 OCR、平台估值来源、复核人员和估值 7 天有效期如何实现。
7. “以物代押/验货点”是否真的有合作履约方。页面现有规则无法仅靠数据库与小程序实现真实的托管物流和防空包。
8. 信用流水具体触发、申诉证据、管理员审核权限、月度奖励上限，以及异议期如何冻结分数。
9. 社区内容审核、举报处理、敏感词/联系方式检测及小程序上线类目资质。
10. 图片对象存储、保留期限/删除流程、用户定位及紧急联系人数据的隐私告知与授权。

## 10. AFaster / MySQL 8 项目基线

当前项目使用 MySQL 8，AFaster 和 afastorm ORM feature 均启用 `orm-mysql`。MySQL 专用 schema 迁移位于 `migrations/mysql/`；SQLite 旧迁移保留但不用于当前配置。AFaster `wx-login-mini` 的 `mini_login()` 用于 code2session；手机号绑定使用扩展后的 `get_user_phone_number()`，其 access_token 在 WxLogin 实例内缓存。当前项目依赖本机开发源码：`C:\Users\ahrik\Documents\afaster` 和 `C:\Users\ahrik\Documents\afastorm`。

认证 service 模型通过 afastorm `Model`/`Repository` 执行读写；数据库 schema 由生成并审查过的迁移 SQL 管理。应用启动不自动执行迁移，先注册模型、检查 `migrations/mysql/` 下的 up/down SQL，再显式运行 `orm migrate --dir migrations/mysql`。

## 11. 代码索引

- 页面注册与 tab：`miniprogram/app.json`
- 演示数组：`miniprogram/utils/data.js`
- 当前本地状态和演示写操作：`miniprogram/app.js`
- 信用分阈值、奖惩规则：`miniprogram/utils/score.js`
- 城市及偏远地区规则：`miniprogram/utils/region.js`
- 后端 Cargo feature 与依赖：`backend/Cargo.toml`
- MySQL/微信登录本地配置模板：`backend/config.example.toml`
