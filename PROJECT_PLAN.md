# OpenList CDN 预热桌面应用：项目完整计划（Nuxt 4 + Tauri 2）

> 目标：基于 OpenList（AList/OpenList）API 遍历某站点某目录的所有文件，并对每个文件发起“完整下载读取”（不落盘，仅消费完响应数据流），以刷新/预热在本地代理与 CDN 场景下的缓存，从而提升后续真实下载速度。

## 0. 范围与成功标准

### 核心成功标准（可验收）
- 能配置一个 OpenList 站点与一个起始目录，递归枚举所有文件（支持分页）。
- 对枚举出的每个文件发起预热：**服务端完整发送、客户端完整读取到 EOF**；记录成功/失败与耗时、读取字节数。
- 预热请求尽量保持“可缓存友好”：使用稳定 URL（不带一次性 query）、尽量少且稳定的 header，从而提高 CDN 命中率。
- 支持 **总速度限速**（全局带宽上限，bytes/s）与 **最大并发连接数**（同时下载任务上限）。
- 当实际总速度低于目标（带宽上限）且并发未达上限时，**自动增加并发任务**以尽力逼近目标。
- 展示整体进度（文件数、完成数、失败数、总字节、滚动平均速度、ETA）以及任务列表。
- 支持定时任务：按 cron/间隔启动预热任务；可启用/停用；可查看下次运行时间。

### 明确不做（第一阶段）
- 不做文件内容保存与校验（hash 校验可作为后续增强）。
- 不做多站点同时跑（先支持一个站点配置；后续可扩展）。
- 不做“分布式预热”（多机器协同）——仅本机 GUI。

## 1. OpenList API 依赖与关键接口（来自 llms.txt）

### 1.1 鉴权
- `POST /api/auth/login`
  - 请求：`{ username, password, otp_code? }`
  - 响应：JWT token
  - **注意**：文档注明 `Authorization` 头里直接放 token（不加 `Bearer` 前缀）。

### 1.2 目录遍历
- `POST /api/fs/list`
  - 请求：`{ path, password?, refresh?, page?, per_page? }`，`per_page <= 100`
  - 响应：`data.content: FsObject[]`，`data.total` 等
  - `FsObject` 关键字段：`name, is_dir, size, modified, created, path(系统路径)`
  - 备注：文档里确实存在 `sign` 字段（下载鉴权签名），但**本项目默认不使用**，并建议用户关闭/避免此特性（见 1.4）。

### 1.3 文件信息
- `POST /api/fs/get`
  - 请求：`{ path, password? }`
  - 响应：`data: FsObject`

### 1.4 下载/直链（无签名、对 CDN 更友好）
很多 OpenList/AList 的“签名直链”（如 `?sign=...`）会导致 URL **随请求变化**，从 CDN 的角度看更难复用缓存（命中率差/容易被视为不同资源）。

因此本项目的默认建议是：
- **建议用户关闭/避免 `sign` 签名下载**（如果站点允许），尽量让同一文件始终对应稳定 URL。
- 预热时优先使用**稳定 URL**（不含一次性签名参数），并让用户自行选择“预热用下载域名”（通常是 CDN 域名）。

实现上把“下载 URL 生成”做成可配置策略，常见可用形态包括（以你的站点实际为准）：
- 形态 A（最常见、稳定）：`GET {downloadBase}/d{path}`
- 形态 B：通过某个接口拿到 `raw_url`（若存在），但要求 `raw_url` 本身稳定
- 形态 C：TS 旧接口 `GET /@file/link/path/<path>` 获取下载 URL（仅当返回稳定链接时推荐）

因此第一版实现建议：
1) **优先走可配置的下载 URL 模板**（用户可选/自定义），比如 `"{downloadBase}/d{path}"`；
2) 提供“连接测试”按钮：对某个文件尝试预热并显示最终命中的 URL 与 HTTP 状态；
3) 若站点必须启用签名/鉴权，建议改用“分享链接/公开目录/稳定鉴权方式”，否则 CDN 预热收益会显著下降（本项目仍可工作，但效果不保证）。

补充（已确认的实际站点形态）：
- 存在可直接访问的最终文件 URL，形如：`https://<host>/d/<path...>/<file>`
- 下载侧无需鉴权、无需额外 header（这对 CDN 预热非常友好）
- URL 需要正确处理中文与特殊字符：实现时必须对每个路径段进行 percent-encoding，避免拼接出非法 URL。

#### 1.4.1 为什么“无 sign + 稳定 URL”对 CDN 预热更重要
CDN 通常以“缓存键（cache key）”区分资源。缓存键常见由以下元素组成（具体取决于你的 CDN 配置）：
- URL 路径（path）
- QueryString（`?a=b`）是否参与缓存键
- 选择性 header（如 `Range`、`Accept-Encoding`、`User-Agent` 等是否参与缓存键）

补充（已确认的实际环境）：你的 CDN **不把 Header 纳入缓存键**，因此本项目不把“通过调 Header 来提升命中”作为优化方向，默认只发送必要且稳定的 header。

一次性 `sign` 参数会让同一文件对应成**不同的 URL**，极易导致：
- 每次预热都“像在预热一个新资源”，命中率低
- 同一路径的缓存被切碎，浪费带宽

所以本项目默认策略是：**强烈建议用户关闭签名直链**，并使用稳定的 `/d/...` 链接进行预热。

#### 1.4.2 链接验证（建议用命令行，不在应用内抓取示例链接）
如果你要在本机快速确认某个 `/d/...` 链接确实“无需鉴权且能完整下载”，推荐以下方式：
- Windows PowerShell：用 `Invoke-WebRequest` 发起请求并观察状态码、响应头（注意不要真正落盘大文件）。
- Windows 原生 `curl.exe`：请求头检查、跟随重定向、以及（可选）限制下载速度做模拟。

> 在应用内我们也会提供“连接测试”按钮，但它用于你自己的站点配置，不需要也不应该去抓取文档/示例外链来验证。

## 2. 产品形态与模块划分

### 2.1 为什么用 Tauri 后端做预热引擎
- 高并发下载 + 丢弃数据流更适合 Rust `reqwest`/`hyper`：内存可控、吞吐更稳。
- 可更容易实现全局 token bucket 限速与任务调度器。
- 可绕开浏览器环境 CORS/混合内容限制（尤其当下载域名是 CDN 域名）。

> Nuxt（前端）负责 UI/配置/状态展示；Tauri（Rust）负责：遍历、任务队列、下载流、限速、定时任务、持久化。

### 2.2 子系统
1) **站点配置（Settings）**
   - OpenList API Base URL
   - 下载域名（可选，与 API 域名不同，用于命中 CDN）
   - 起始目录（path）
   - 账号/密码/OTP（可选；或直接粘贴 token）
   - 目录密码（OpenList 的 `password` 字段，用于受保护路径）
   - 下载 URL 策略/模板
  - 代理设置（HTTP/HTTPS proxy，必要时支持 PAC/系统代理：后续）
  - 重要提示（面向用户）：你指定要预热的目录应当在“启用本机代理”的访问方式下可直接下载（即**不发生 302 跳转**），以确保预热请求落到期望的 CDN 链路。

2) **遍历器（Crawler）**
   - 以 `/api/fs/list` 做 BFS/DFS 递归枚举
   - 处理分页：按 `page/per_page` 拉满 `total`
  - 产出：文件条目列表（path、size、相对路径展示用）

3) **预热引擎（Preheater）**
   - 任务队列：待下载文件 -> 下载 worker
   - 连接池/并发控制：最大并发 N
   - 全局限速：token bucket（bytes/sec）
   - 速度控制策略：当滚动速度 < 目标且并发 < N 时自动补 worker
   - 对每个文件：打开 HTTP 请求，读取 stream 到 EOF，丢弃 chunk，仅计数

4) **进度与事件（Progress/Event Bus）**
   - Rust -> 前端事件推送（Tauri event）
   - UI 展示实时速度、完成数、错误

5) **定时任务（Scheduler）**
   - cron 表达式/固定间隔
   - 任务互斥：同一站点同一目录避免重复同时跑（可配置）

6) **本地存储（Persist）**
   - 使用 tauri store（模板已有示例页）保存设置、最近任务、调度器配置

## 3. 数据模型（建议）

### 3.1 Settings
- `apiBaseUrl: string`
- `downloadBaseUrl?: string`
- `startPath: string`
- `auth: { mode: 'token'|'password', token?: string, username?: string, password?: string, otp?: string }`
- `dirPassword?: string`
- `downloadStrategy: { kind: 'template'|'d-endpoint'|'ts-file-link'|'custom', template?: string }`
- `downloadPathPrefix?: string`  // 默认 `/d`，保留为可配置（少数站点会改）
- `rateLimit: { bytesPerSec: number }`  // 0 表示不限速
- `concurrency: { max: number, min?: number }`
- `network: { timeoutMs: number, retry: { maxAttempts: number, backoffMs: number }, proxy?: string }`
- `preheatMode?: { kind: 'full'|'partial', maxBytesPerFile?: number }`  // 默认 full；partial 仅作为“超大文件降低成本”的可选项

#### 3.1.1 表单校验规则（建议用 zod 写在前端，也可后端二次校验）
- `apiBaseUrl`：必填；必须是 `http://` 或 `https://`；去除结尾 `/`。
- `downloadBaseUrl`：必填；必须是 `http://` 或 `https://`；去除结尾 `/`。
- `startPath`：必填；必须以 `/` 开头（或自动补 `/`）；不允许包含 `..`。
- `auth.mode=token`：token 允许为空（用于公开站点），但若 `/api/fs/list` 返回 401/403 应提示切换为 token 登录。
- `downloadPathPrefix`：默认 `/d`；必须以 `/` 开头且不以 `/` 结尾。
- `rateLimit.bytesPerSec`：0=不限速；最小建议值例如 256KB/s（UI 可给提示）；最大不限制。
- `concurrency.max`：建议 1~256；UI 可限制上限避免误触（例如 256）。
- `network.timeoutMs`：建议默认 60000；最小 1000。
- `retry.maxAttempts`：建议默认 2~3。
- `preheatMode.kind`：默认 `full`；若为 `partial` 则 `maxBytesPerFile` 必填且 >0。

### 3.2 FileItem
- `id: string`（可用相对路径 hash）
- `path: string`（OpenList path，如 `/folder/file.ext`）
- `name: string`
- `size?: number`
- `isDir: boolean`

### 3.3 TaskRun / TaskState
- `runId: string`
- `startedAt, finishedAt`
- `status: 'running'|'paused'|'completed'|'canceled'|'failed'`
- counters:
  - `totalFiles, queued, active, done, failed`
  - `bytesReadTotal`
  - `speedBpsRolling`
- per file:
  - `state: queued|running|done|failed|canceled`
  - `httpStatus?`
  - `bytesRead`
  - `error?`

### 3.4 Schedule
- `enabled: boolean`
- `type: 'cron'|'interval'`
- `cron?: string`
- `intervalMinutes?: number`
- `jitterSeconds?: number`（避免整点洪峰）
- `lastRunAt, nextRunAt`

## 4. 关键流程设计

### 4.1 登录与 token 管理
- UI 提供两种模式：
  - 粘贴 token（推荐，最少权限/最少敏感信息）
  - 用户名/密码登录（调用 `/api/auth/login`，拿 token 后只保存 token，尽量不落盘密码）
- 每次请求 `/api/fs/list`、`/api/fs/get` 带 `Authorization: <token>`。

### 4.2 递归遍历目录（带分页）
- 输入：`startPath`。
- 使用队列 BFS：
  1) `list(path, page=1, per_page=100)`
  2) 收到 `total` 后继续请求 page=2..直到 `content` 拉满
  3) 对 `content`：
     - `is_dir=true`：入队继续
     - 否则：加入文件列表
- 关键点：
  - OpenList `FsObject.path` 在文档里是“系统路径”（如 `D:\files\...`），但请求用的是 OpenList 逻辑路径（如 `/document.pdf`）。
  - 因此应当以请求参数 `path` + 返回的 `name` 拼接出逻辑路径：
    $$ childPath = joinPath(parentPath, name) $$
  - `refresh` 默认 false；UI 提供“强制刷新缓存”选项。

### 4.3 构建“预热下载 URL”
- 仅使用 `FileItem.path`（逻辑路径）构建稳定 URL（不使用 `sign`）。
- 提供三种策略：
  1) **模板**：用户定义，例如：`{downloadBase}/d{path}`
  2) **默认 d-endpoint**：`{downloadBase}/d{path}`
  3) **TS link**：调用 `GET {apiBase}/@file/link/path{path}` 拿到直链后再 GET（仅当返回链接稳定时推荐）

实现细节（建议写进代码注释，避免踩坑）：
- `path` 是 OpenList 逻辑路径（以 `/` 开头）；构建下载 URL 时应生成 `/d` 前缀：`{downloadBase}/d{path}`。
- `path` 可能包含中文、空格、全角符号等：需要对**每个路径段**分别编码，保留 `/` 作为分隔符。

编码建议（把坑提前写死）：
- 不要对整个 path 一次性编码（会把 `/` 也编码掉，导致路径结构丢失）。
- 应对每个 segment 单独做 percent-encoding：
  - 输入：`/A B/结构.pdf`
  - 输出：`/A%20B/%E7%BB%93%E6%9E%84.pdf`
- 对于全角括号等字符同样需要编码（你的示例链接中就包含全角符号）。

> 计划里把它抽象为 `resolveDownloadUrl(file) -> url`，并在 UI 提供“测试解析”。

#### 4.3.1 URL 生成的确定性（对 CDN 预热很关键）
对同一 `FileItem.path`：
- 多次运行生成的下载 URL 必须完全一致（除非用户改了 `downloadBaseUrl` / `downloadPathPrefix`）。
- 不应自动附加时间戳、随机 query、动态 header。

建议将 URL 生成结果缓存到本次 run 的 `file.urlResolved` 字段中，用于：
- 重试时复用
- UI 展示
- 302 警告中记录原始/最终 URL

### 4.4 预热下载（不落盘）
- 对每个文件发起 HTTP GET：
  - 必须读取 `response.body` 直到 EOF
  - 对每个 chunk：
    - 计数 `bytesRead += chunk.len`
    - 不写入磁盘
- 成功条件：
  - HTTP 2xx（可配置接受 3xx 跟随）
  - Stream 正常结束（无中断错误）
  - 若已知 `Content-Length`/file size，可做一致性检查（可选）

CDN 预热实践建议：
- **默认用 full-body GET**：读取到 EOF，最符合“让 CDN 把整文件缓存下来”的目标。
- `Range` 请求是否使用：
  - 默认不使用（很多 CDN 的缓存键会把 `Range`/分片策略考虑进去，可能导致只缓存部分或产生多份缓存）。
  - 仅在“超大文件成本不可接受”时才允许 `partial` 模式（例如只读前 N MB），并在 UI 里明确提示：这不保证提升全量下载命中。
- Header 策略：已确认 CDN 不把 Header 纳入缓存键，因此默认只使用必要 header（例如固定的 `User-Agent`），不把 header 作为“命中优化旋钮”。

链路校验（与“本机代理”强相关）：
- “连接测试”应展示：是否发生重定向、最终 URL 的 host、以及实际命中域名。
- 若出现 302/跳转到非预期域名，提示用户优先检查：是否启用了本机代理、下载域名是否填写为 CDN 域名。

#### 4.4.1 302/跳转处理（策略已定）
- 发现 302/跳转：**记录 warning 并继续**。
- 记录字段建议：
  - `originalUrl`
  - `finalUrl`（若跟随重定向）
  - `finalHost`
  - `httpStatus`（例如 302）
  - `filePath`

> 注：虽然目标目录应当“启用本机代理且无 302”，但现实环境可能存在个别文件异常（或用户配置错误）。因此采取“警告 + 继续”可避免单点阻塞。

### 4.5 全局限速 + 并发自适应

#### 4.5.1 全局 token bucket 限速
- 目标：全局吞吐不超过 `bytesPerSec`。
- 设计：
  - bucket 容量：`bytesPerSec`（或 *2 作为 burst）
  - 每 100ms เติม tokens：`bytesPerSec / 10`
  - 每个下载任务读取 chunk 前先 `acquire(chunkSize)`；不足则 await。

#### 4.5.2 并发控制与“补连接”策略
- 最大并发 `maxConn`；启动时可先起 `minConn`（默认为 1 或 2）。
- 每 1s 计算滚动速度 $v$：
  $$ v = \frac{\Delta bytes}{\Delta t} $$
- 若 `bytesPerSec > 0` 且 `v < bytesPerSec * 0.95` 且 `active < maxConn` 且 `queue not empty`：
  - 启动 1 个新 worker（或按差距启动多 1-2 个，避免抖动）
- 若 `bytesPerSec == 0`（不限速）：
  - 直接跑到 `maxConn`（或仍用自适应：看失败率与 429/503）

#### 4.5.4 自适应并发的更具体规则（避免抖动）
建议采用“缓慢加、必要时才减”的策略：
- 每个采样周期（例如 500ms~1s）计算滚动速度 `vRolling`。
- 当 `rateLimit > 0`：
  - 若 `vRolling < target * 0.95` 且 `active < maxConn` 且 `queue > 0`：`desired += 1`
  - 若 `vRolling > target * 1.10`：不主动降（交给 token bucket 自然限速即可）
- 当 `rateLimit == 0`：
  - 直接将 `desired = maxConn`。

并发“减”的触发建议仅在异常情况下启用（可选）：
- 连续出现大量 429/503 或错误率超过阈值（例如 10%）时，暂时降低 `desired` 并指数退避。

#### 4.5.3 错误与退避
- 常见错误：403（目录/文件密码不对）、401（token 失效）、404、5xx、连接超时。
- 重试策略：
  - 对网络错误/5xx：指数退避重试 `maxAttempts` 次
  - 对 4xx（除 408/429）：默认不重试
  - 对 429：按 `Retry-After` 或退避

## 5. UI/交互设计（NuxtUI v4 + Tailwind v4）

### 5.1 页面建议
- `/`：仪表盘（最近一次任务、快速开始）
- `/site`（新建）：站点配置（URL、鉴权、目录、下载策略、代理）
- `/preheat`：预热控制台（开始/暂停/继续/取消、实时图表、任务列表）
- `/schedules`：定时任务列表与编辑
- `/logs`（可选）：导出日志/错误统计

### 5.4 关键用户流程（UX Flow）

#### 5.4.1 首次使用（最短路径）
1) 打开应用 → “新建站点/任务”
2) 填写：`apiBaseUrl`、`startPath`（`downloadBaseUrl` 可选：默认与 `apiBaseUrl` 相同；如需走 CDN 则改填 CDN 域名）
3) （可选）填写 token 或选择公开访问
4) 设置：最大并发、总速度上限
5) 点击“连接测试”
6) 点击“开始预热”

#### 5.4.2 连接测试（建议拆成两个子测试）
- API 侧测试：调用 `/api/fs/list` 列出 `startPath` 第一页，验证：
  - 401/403（token/目录密码问题）
  - 404（路径问题）
- 下载侧测试：选择一个文件（或让后端随机挑一个小文件）生成 `/d/...` URL 并 GET：
  - 是否 2xx
  - 是否发生 302（记录 warning）
  - 速度是否能通过本机代理跑起来

#### 5.4.3 运行中可控项
- 暂停/继续：暂停仅停止拉取新任务，允许当前连接自然结束（更简单更稳）；或提供“硬暂停”中断连接（后续增强）。
- 动态调整：允许修改 `rateLimit` 与 `maxConn` 并在下一采样周期生效（后端需要读最新配置）。

### 5.2 组件建议
- `SiteForm`：站点与目录配置表单
- `RateLimitForm`：速度/并发/重试配置
- `RunControls`：Start/Pause/Resume/Cancel
- `ProgressSummary`：总进度、速度、ETA、完成率
- `FileTaskTable`：文件任务表（支持筛选：失败/进行中）
- `SpeedChart`：最近 60s 速度曲线（可选）

### 5.3 状态管理
- 简化方案：Nuxt `useState` + 事件驱动（Tauri event）。
- 若复杂：Pinia（后续再引入）。

## 6. Tauri/Rust 实现设计

### 6.1 Tauri commands（前端调用）
- `login(settings) -> token`
- `crawl(startPath, options) -> crawlId`（或直接返回文件列表，注意大目录需要流式/分页）
- `startRun(settings) -> runId`
- `pauseRun(runId)` / `resumeRun(runId)` / `cancelRun(runId)`
- `getRunSnapshot(runId)`（UI 重连/刷新）
- `saveSettings(...)` / `loadSettings()`
- `scheduleUpsert(...)` / `scheduleList()` / `scheduleDelete(id)`

### 6.5 命令/事件契约（建议先写成文档，减少前后端对接摩擦）

#### 6.5.1 Commands（建议参数/返回）
- `startRun(settings) -> { runId }`
- `cancelRun(runId) -> void`
- `getRunSnapshot(runId) -> RunSnapshot`
- `testConnection(settings) -> ConnectionTestResult`
  - 返回建议包含：
    - `apiOk` / `apiError`
    - `downloadOk` / `downloadError`
    - `sampleFilePath` / `sampleUrl`
    - `redirected: boolean`、`finalUrl?`

#### 6.5.2 Events（建议 payload）
- `run:started`：`RunSnapshot`
- `run:progress`：`RunSnapshot`
- `run:fileUpdate`：
  - `runId`
  - `path`
  - `status: queued|running|done|failed|canceled`
  - `bytesRead`
  - `httpStatus?`
  - `warning?`（例如 302 重定向）
  - `error?`
- `run:warning`（可选，若不想塞进 fileUpdate）：
  - `code: "REDIRECT"|...`
  - `message`
  - `context`（originalUrl/finalUrl/path 等）
- `run:finished`：`RunSnapshot`

### 6.2 Tauri events（后端推送）
- `run:started`, `run:progress`, `run:fileUpdate`, `run:finished`, `run:error`
- UI 以 runId 过滤。

### 6.3 下载实现要点
- HTTP client：`reqwest`（启用 gzip/br/timeout/redirect）
- 代理：reqwest `Proxy::all(proxyUrl)`
- 丢弃 body：循环 `while let Some(chunk) = stream.next().await { ... }`
- 内存：chunk 不缓存，不拼接。

### 6.4 大规模任务的性能与内存
- 不把所有文件一次性塞进 UI：
  - Rust 内部存队列即可
  - UI 只展示最近 N 条 or 分页
- 统计数据（bytes/s）用原子计数 + 采样线程。

## 7. 定时任务设计
- 支持两类：
  - cron（例如 `0 3 * * *` 每天凌晨 3 点）
  - interval（每 N 分钟）
- 任务执行策略：
  - 若正在运行且 `mutex=true`：跳过或排队（用户可选）
  - 记录 lastRun / nextRun / lastResult

  ### 7.1 定时任务与手动任务的互斥策略
  建议默认：同一站点同一目录“互斥运行”。
  - 若手动启动时已有定时任务正在跑：提示用户“正在运行”，可选择取消当前或等待。
  - 若定时触发时已有手动任务正在跑：跳过本次定时（记录一条 skipped 日志）。

  ### 7.2 Cron 与 Interval 的“抖动（jitter）”
  为避免整点/整分触发造成瞬时带宽峰值，建议提供 `jitterSeconds`：
  - Interval：每次触发在 `[-jitter,+jitter]` 范围内随机偏移
  - Cron：在命中 cron 后随机延后 `0..jitterSeconds`

## 8. 测试与验证

### 8.1 本地验证清单
- 用一个小目录（10-50 文件）验证：
  - 递归遍历正确
  - 下载 URL 解析正确
  - 预热读完 EOF 且统计 bytes 正确
- 限速：设置 5MB/s，看总速度是否稳定在目标附近。
- 并发补偿：设置较高目标速度 + 较低初始并发，观察是否自动加任务。

### 8.3 “可缓存友好”验收（建议人工检查）
- 随机抽样同一文件，在不同时间运行多次：生成的预热 URL 应完全一致。
- 若 CDN 控制台可查：观察该 URL 的命中率变化与回源量下降。
- 对出现 302 的文件：应能在日志/警告中定位到原始与最终 URL。

### 8.2 自动化测试（建议逐步补齐）
- Rust：
  - token bucket 单测
  - URL 模板解析单测
  - 重试策略单测
- 前端：
  - 表单校验（必填项、URL 格式）

## 9. 里程碑（推荐按 4 个迭代交付）

### M1：跑通最小闭环（1-2 天）
- 配置页（apiBaseUrl、token、startPath）
- 调用 `/api/fs/list` 递归拿到文件列表
- 预热：按固定并发（如 4）下载并丢弃数据
- UI：展示总进度 + 任务列表

### M2：限速 + 自适应并发（2-4 天）
- token bucket 全局限速
- 滚动速度统计 + 自动补连接
- 错误重试/退避

### M3：定时任务 + 持久化（1-3 天）
- schedule CRUD
- 后台定时启动 run
- 设置与 schedule 落盘

### M4：工程化与体验（持续）
- 日志/导出
- 多站点配置
- 更强的下载策略探测（自动识别 d-endpoint/直链）
- 更细粒度 UI（图表、筛选、失败重跑）

## 10. 风险点与预案
- **下载 endpoint 不统一**：用“策略可配置 + 一键测试”规避。
- **路径字段歧义**：返回 `FsObject.path` 可能是系统路径；遍历必须用逻辑路径拼接。
- **CDN/代理影响**：提供 `downloadBaseUrl` 与 proxy 配置；并提供“命中域名显示”。
- **签名 URL 影响缓存命中**：若站点启用一次性 `sign` 参数，CDN 预热收益可能很差；建议关闭签名或改用稳定可缓存的直链/分享链接。
- **中文/特殊字符路径导致 404**：如果 URL 编码做错，会出现“同一个文件浏览器能下、应用下不了”的问题；用“逐段编码 + 连接测试”规避。
- **大目录性能**：UI 分页/只展示部分；后端流式派发进度。

## 11. 持久化、日志与可观测性（更具体的设计）

### 11.1 本地持久化（tauri store）建议键结构
- `settings.current`：当前站点配置
- `settings.history[]`：最近 N 条配置快照（便于切换站点/目录）
- `schedules[]`：定时任务列表
- `runs.recent[]`：最近 N 次运行摘要（runId、时间、总文件数、失败数、总字节、平均速度）

### 11.2 日志与导出（面向排障）
建议支持两种导出：
- 导出“运行摘要”（JSON）：用于上传/分享排查
- 导出“失败/警告列表”（CSV/JSON）：方便用户重跑失败项

字段建议（每条 file 结果）：
- `path`
- `url`
- `status`
- `httpStatus`
- `bytesRead`
- `durationMs`
- `warningCode?`（例如 REDIRECT）
- `warningMessage?`
- `error?`

## 12. 安全与隐私（约束写清楚，避免后续返工）
- 若用户使用 token：
  - 默认只在本机持久化 token（tauri store），并提供“一键清除本地凭据”。
  - UI 显示 token 时默认打码。
- 不记录文件内容，仅记录必要元信息与统计。
- 对导出文件：提示其中可能包含下载 URL 与路径信息，用户自行评估分享范围。

## 13. 数据契约（Schema 级规范，便于前后端对接与隐藏测试）

> 说明：这里把关键结构“写死”，后续实现时尽量按本节落地（字段名/类型/含义一致），避免 UI 与 Rust 来回返工。

### 13.1 Settings（站点配置 + 运行配置）

#### 13.1.1 `SiteSettings`

| 字段 | 类型 | 必填 | 示例 | 说明 |
|---|---:|:---:|---|---|
| `id` | `string` | 是 | `site_01` | 本地唯一 ID（UUID/时间戳均可） |
| `name` | `string` | 是 | `我的站点` | UI 展示名 |
| `apiBaseUrl` | `string` | 是 | `https://oplist.example.com` | OpenList API 根地址（不带 `/api` 也可，但实现需统一拼接） |
| `downloadBaseUrl` | `string` | 否 | `https://cdn.example.com` | **用于拼接稳定 `/d/...` 的域名**。若不填则默认使用 `apiBaseUrl`（host 相同）；需要走 CDN 时再填写 CDN 域名。 |
| `startPath` | `string` | 是 | `/some/folder` | 预热起始目录（逻辑路径） |
| `token` | `string` | 否 | `xxxxxxxx` | OpenList token；若下载侧公开可为空（但目录遍历可能仍需） |
| `dirPassword` | `string` | 否 | `1234` | 目录密码（OpenList `fs/list` 的 `password` 字段） |
| `proxyUrl` | `string` | 否 | `http://127.0.0.1:7890` | 本机代理（强相关：避免 302、确保走期望链路） |
| `userAgent` | `string` | 否 | `olist-cdn-preheat/0.1` | 统一 UA，便于日志识别（已确认 Header 不进缓存键） |
| `timeoutMs` | `number` | 否 | `60000` | 单请求超时；建议区分 API 与下载（实现可拆分） |
| `followRedirects` | `boolean` | 否 | `true` | 下载请求是否跟随 3xx；即使跟随也要记录 302 warning |

校验规则（前端与后端都应执行一次）：
- `apiBaseUrl` 必须是 `http(s)://`。
- `downloadBaseUrl` 若填写，必须是 `http(s)://`；若为空则视为 `apiBaseUrl`。
- `startPath` 必须以 `/` 开头；不允许空字符串。
- `proxyUrl` 若填写，必须是 `http(s)://` 或 `socks5://`（按实现能力）。

#### 13.1.2 `RunSettings`

| 字段 | 类型 | 必填 | 默认 | 说明 |
|---|---:|:---:|---:|---|
| `minConn` | `number` | 否 | `2` | 初始并发 |
| `maxConn` | `number` | 是 | `16` | 最大并发（连接数上限） |
| `rateLimitBytesPerSec` | `number` | 否 | `0` | 0 表示不限速；否则全局 token bucket 目标 |
| `chunkBytes` | `number` | 否 | `65536` | 每次读流的 chunk 大小（影响 token bucket 粒度） |
| `maxAttempts` | `number` | 否 | `3` | 单文件最大尝试次数 |
| `backoffBaseMs` | `number` | 否 | `500` | 指数退避 base |
| `backoffMaxMs` | `number` | 否 | `10000` | 指数退避上限 |
| `partialMode` | `{ enabled: boolean; maxBytes?: number }` | 否 | `enabled=false` | 默认 full-body；仅在超大文件成本不可接受时使用 |
| `warnOnRedirect` | `boolean` | 否 | `true` | 发现 302/跳转时是否产出 warning（策略已定：继续跑，但可关闭警告） |

### 13.2 Connection Test 契约

#### 13.2.1 `ConnectionTestResult`

| 字段 | 类型 | 必填 | 说明 |
|---|---:|:---:|---|
| `api` | `ApiTestResult` | 是 | API 子测试结果（`/api/fs/list`） |
| `download` | `DownloadTestResult` | 是 | 下载子测试结果（对 `/d/...` GET） |
| `sampleFile` | `{ path: string; url: string } \| null` | 是 | 被选中的样本文件（无则为 null） |
| `warnings` | `WarningItem[]` | 是 | 例如 302/域名不一致 |

`ApiTestResult`：
- `ok: boolean`
- `httpStatus?: number`
- `latencyMs?: number`
- `error?: string`

`DownloadTestResult`：
- `ok: boolean`
- `httpStatus?: number`
- `latencyMs?: number`
- `bytesRead?: number`（应读到 EOF 或读到 partial 上限）
- `redirected?: boolean`
- `finalUrl?: string`
- `finalHost?: string`
- `error?: string`

### 13.3 Run 运行态契约

#### 13.3.1 `RunSnapshot`（用于 `run:progress` 与 `getRunSnapshot`）

| 字段 | 类型 | 必填 | 说明 |
|---|---:|:---:|---|
| `runId` | `string` | 是 | 运行 ID |
| `siteId` | `string` | 是 | 关联站点 |
| `state` | `"idle"\|"running"\|"paused"\|"canceling"\|"finished"\|"failed"` | 是 | 运行状态 |
| `startedAt` | `string` | 是 | ISO 时间 |
| `updatedAt` | `string` | 是 | ISO 时间 |
| `totalFiles` | `number` | 是 | 总文件数（未知则为 0，并标记 `totalKnown=false`） |
| `totalKnown` | `boolean` | 是 | 是否已确定 total（大目录可先 false） |
| `queued` | `number` | 是 | 队列待处理数量 |
| `active` | `number` | 是 | 当前活跃连接/worker 数 |
| `desired` | `number` | 是 | 自适应期望并发 |
| `done` | `number` | 是 | 已完成文件数 |
| `failed` | `number` | 是 | 失败文件数 |
| `warnings` | `number` | 是 | 警告计数（例如 302） |
| `bytesReadTotal` | `number` | 是 | 累计读取字节（不落盘） |
| `speedBps` | `number` | 是 | 滚动速度（bytes/sec） |
| `etaSec` | `number \| null` | 是 | 预计剩余秒数（无法估计时 null） |

#### 13.3.2 `FileUpdate`（用于 `run:fileUpdate`）

| 字段 | 类型 | 必填 | 说明 |
|---|---:|:---:|---|
| `runId` | `string` | 是 | 关联 run |
| `path` | `string` | 是 | 文件逻辑路径（用于拼 `/d/...`） |
| `status` | `"queued"\|"running"\|"done"\|"failed"\|"canceled"` | 是 | 状态 |
| `attempt` | `number` | 否 | 当前尝试次数 |
| `url` | `string` | 否 | 实际请求 URL（导出/排障用） |
| `bytesRead` | `number` | 是 | 已读字节（running/done 时更新） |
| `httpStatus` | `number` | 否 | HTTP 状态 |
| `durationMs` | `number` | 否 | 本次请求耗时 |
| `warning` | `WarningItem \| null` | 否 | 例如 302 跳转（策略：warning + continue） |
| `error` | `string \| null` | 否 | 失败原因（可读字符串，避免仅错误码） |

`WarningItem`：
- `code: "REDIRECT"\|"UNEXPECTED_HOST"\|"PARTIAL_MODE"\|"OTHER"`
- `message: string`
- `context?: Record<string, unknown>`（建议包含 `originalUrl/finalUrl/finalHost/httpStatus` 等）

### 13.4 导出格式（可被 Excel/脚本消费）

#### 13.4.1 运行摘要 `run-summary.json`
- 顶层：`{ run: RunSnapshot, settings: { site: SiteSettings, run: RunSettings }, startedAt, finishedAt?, totals: { files, bytes, durationMs }, errorsTopN?: string[] }`

#### 13.4.2 文件结果 `file-results.csv`
列建议（按顺序）：
- `path,url,status,httpStatus,bytesRead,durationMs,warningCode,warningMessage,error`

## 14. UI 细化（字段、表格列、交互细节）

### 14.1 站点配置页（`/site`）
建议分成 3 个卡片区域：
1) **站点信息**：`name`、`apiBaseUrl`、`token`（打码 + 一键清除）
2) **预热目标**：`downloadBaseUrl`、`startPath`、`dirPassword`
3) **网络/行为**：`proxyUrl`、`timeoutMs`、`followRedirects`（开关）

`downloadBaseUrl` 交互建议：
- 默认自动填充为 `apiBaseUrl`（或留空但展示占位提示“默认与 apiBaseUrl 相同”）。
- 当用户输入 `apiBaseUrl` 后，若 `downloadBaseUrl` 尚未被用户手动编辑（dirty=false），则自动同步更新。
- 一旦用户手动编辑了 `downloadBaseUrl`，后续不再自动覆盖（提供“重置为 apiBaseUrl”按钮更友好）。

表单底部按钮：
- “连接测试”（输出结构按 `ConnectionTestResult` 展示）
- “保存”
- “保存并开始预热”（成功保存后跳转控制台并 `startRun`）

连接测试结果 UI：
- API：显示 `httpStatus`、`latencyMs`、错误文本
- 下载：显示 `httpStatus`、`finalHost`、`bytesRead`、`redirected`（若 true 高亮 warning）
- 若出现 warning：显示可复制的 `originalUrl` 与 `finalUrl`

### 14.2 预热控制台（`/preheat`）

#### 14.2.1 顶部摘要（`ProgressSummary`）
- 进度：`done/totalFiles`（若 `totalKnown=false`，显示“已发现 X 个文件”）
- 速度：`speedBps`（自动转换 MB/s）
- ETA：`etaSec`（无法估计时显示 `--`）
- 计数：失败数、警告数、活跃连接数/期望连接数

#### 14.2.2 控制区（`RunControls`）
- Start / Pause / Resume / Cancel
- 运行中允许调整：`maxConn`、`rateLimitBytesPerSec`（即时生效）

#### 14.2.3 文件表格（`FileTaskTable`）
列建议：
- `path`（可复制）
- `status`
- `bytesRead`
- `speed`（可选：仅 running 时）
- `httpStatus`
- `warning`（简短文案 + “查看详情”弹窗）
- `error`

筛选/视图：
- 全部 / 进行中 / 失败 / 警告
- 搜索：按 `path` 关键字

展示策略（避免大目录卡 UI）：
- 默认仅展示“最近 N 条变更”（如 200-1000），并提供“仅显示失败/警告”以便排障
- 全量导出由后端生成（避免前端持有全量 list）

### 14.3 定时任务页（`/schedules`）
字段建议：
- `enabled`（开关）
- `type: cron|interval`
- `cronExpr` 或 `intervalMinutes`
- `jitterSeconds`
- `mutex: boolean`（同站点互斥）
- `lastRunAt`、`lastResult`、`nextRunAt`

## 15. 错误码/警告码与提示文案（让用户知道该怎么修）

> 原则：UI 文案要“可行动”。同一个错误给出明确下一步。

### 15.1 建议错误码（`errorCode` 可选，但建议日志/导出保留）

| code | 场景 | UI 提示（示例） | 建议动作 |
|---|---|---|---|
| `API_UNAUTHORIZED` | `/api/fs/list` 401 | API 鉴权失败：token 可能已过期 | 重新登录/更新 token 后重试 |
| `API_FORBIDDEN` | `/api/fs/list` 403 | 无权限访问该目录（或目录密码错误） | 检查 token 权限/填写目录密码 |
| `API_NOT_FOUND` | 404 | 起始路径不存在 | 检查 `startPath` 是否正确 |
| `DOWNLOAD_REDIRECT` | 下载出现 3xx | 发现跳转（已继续预热），这可能意味着未走本机代理或域名配置不一致 | 检查 `proxyUrl`、`downloadBaseUrl` 是否为 CDN 域名 |
| `DOWNLOAD_4XX` | 4xx（非 401/403） | 下载失败（4xx） | 检查该文件是否可公开访问/路径编码 |
| `DOWNLOAD_5XX` | 5xx | 回源/网关错误（将自动重试） | 稍后重试或降低并发 |
| `NET_TIMEOUT` | 超时 | 请求超时（将自动重试） | 增大超时/检查网络/降低并发 |
| `NET_RESET` | 连接重置 | 连接被重置（将自动重试） | 检查代理/网络稳定性 |

### 15.2 警告码（WarningItem.code）
- `REDIRECT`：发现 302/301
- `UNEXPECTED_HOST`：最终 host 不等于（`downloadBaseUrl` 的 host；若 `downloadBaseUrl` 为空则用 `apiBaseUrl` 的 host）（特别提示“可能没走 CDN/代理”）
- `PARTIAL_MODE`：用户启用了 partial（提示“可能不提升全量缓存命中”）

## 16. 验收用例（更具体、可重复）

### 16.1 功能正确性
1) **递归遍历**：给定一个包含子目录与文件的目录，最终 `totalFiles` 与 OpenList UI 看到的一致（允许因权限隐藏导致减少）。
2) **路径编码**：包含中文/空格/`#` 等字符的文件，生成的 `/d/...` URL 能成功下载（2xx）。
3) **不落盘**：运行过程中磁盘不会出现被下载的文件（仅允许日志/导出文件）。

### 16.2 302 策略（已确认）
1) 人为制造一个会 302 的样本 URL：
   - 结果：运行不中断
   - 导出中出现 `warningCode=REDIRECT`，包含 `originalUrl`/`finalUrl`/`finalHost`

### 16.3 限速与并发
1) 设置 `rateLimitBytesPerSec = 5MB/s`：滚动速度在合理波动内围绕目标（允许短时 burst）。
2) 设置 `maxConn=2 -> 16` 动态调整：下一采样周期内活跃连接数逐步趋近新值。

### 16.4 定时与互斥
1) 开启 schedule，手动启动同一站点 run：应提示互斥并按设置跳过/排队（默认跳过定时）。
2) schedule 触发时已有 run：`lastResult` 记录为 `skipped` 且包含原因。

## 参考资料（外部文档来源）

### OpenList / AList API 文档来源
- OpenList `llms.txt`（API 文档索引）：https://fox.oplist.org/llms.txt
- Authentication - User login（`POST /api/auth/login`）：https://fox.oplist.org.cn/364155678e0.md
- File System - List directory contents（`POST /api/fs/list`）：https://fox.oplist.org.cn/364155732e0.md
- File System - Get file or directory info（`POST /api/fs/get`）：https://fox.oplist.org.cn/364155733e0.md
- TS 版本接口 - 文件操作接口（`/@file/...`）：https://fox.oplist.org.cn/349162289e0.md

#### OpenList schema（用于字段确认）
- `FsObject`：https://fox.oplist.org.cn/211045434d0.md
- `FsListRequest`：https://fox.oplist.org.cn/211045435d0.md
- `FsListResponse`：https://fox.oplist.org.cn/211045436d0.md
- `LoginRequest`：https://fox.oplist.org.cn/211045430d0.md

### 框架/运行时文档来源
- Tauri 文档（含前置依赖/环境准备）：https://tauri.app/start/prerequisites
- Tailwind CSS 文档：https://tailwindcss.com/docs

#### Nuxt / Nuxt UI：建议优先使用已配置的 MCP 文档工具（而不是手动翻网页）
本仓库环境已接入 Nuxt 与 Nuxt UI 的 MCP 文档工具，优势是：
- 可以直接按“路径”拉取文档正文（更可复现）
- 适合在实现/排障时精确引用某一节内容
- 避免网页结构变化导致链接失效或抓取噪声

**Nuxt MCP（已试用并确认可用）**
- 获取入门页（示例）：`mcp_nuxt_get-getting-started-guide`（version: `4.x`）
  - 返回示例路径：`/docs/4.x/getting-started/introduction`
- 按路径取任意页面：`mcp_nuxt_get-documentation-page`
  - 例如：`/docs/4.x/guide/concepts/rendering`
- 不知道路径时先列目录：`mcp_nuxt_list-documentation-pages`

**Nuxt UI MCP（已试用并确认可用）**
- 列出入门相关页面：`mcp_nuxt-ui_list-getting-started-guides`
  - 例如返回：
    - `/docs/getting-started/installation/nuxt`
    - `/docs/getting-started/migration/v4`
    - `/docs/getting-started/ai/mcp`
- 按路径取页面：`mcp_nuxt-ui_get-documentation-page`
- 查组件：
  - `mcp_nuxt-ui_list-components`
  - `mcp_nuxt-ui_get-component` / `mcp_nuxt-ui_get-component-metadata`

**在对话中如何使用（推荐写法）**
- “请用 MCP 打开 Nuxt 4 的渲染模式文档：`/docs/4.x/guide/concepts/rendering`，并总结与本项目（Tauri 禁用 SSR）相关的要点。”
- “请用 MCP 打开 Nuxt UI v4 的安装文档：`/docs/getting-started/installation/nuxt`，给出适配本模板的检查清单。”

> 仍然保留网页 URL 的场景：分享给不具备 MCP 工具的人类读者时，可引用 MCP 返回结果里的 `url` 字段（例如 `https://nuxt.com/docs/4.x/getting-started/introduction`）。

---

## 附：已确认/待确认项（用于把实现细节写死）

### 已确认
1) 下载直链为稳定形式：`https://<host>/d/<path...>/<file>`
2) 下载侧无需鉴权、无需额外 header（链接本身即最终文件）
3) 预热目标目录应在“启用本机代理”的链路下可直接下载，因此**不存在 302 跳转**（或可视为配置错误）
4) CDN 不把 Header 纳入缓存键

### 策略已定（实现按此执行）
- 若在“连接测试”或实际预热过程中发现 302/跳转：**记录警告并继续**（不因单个异常阻塞全局预热）。
- 警告内容建议包含：原始 URL、最终 URL（若跟随重定向）、以及最终 host，便于用户排查本机代理/域名配置。

### 待确认（建议后续在设置里做成可选项）
- 暂无（后续如要支持“发现 302 即停止”可作为高级选项添加）。
