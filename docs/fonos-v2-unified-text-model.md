# Fonos v2 — Unified Text Model, Note Mode & UI Architecture

## 概述

Fonos 的所有功能都围绕 text 的不同生命周期。本文档定义三个层面的统一架构：

1. **存储层**：统一的文本存储模型（Entry + Container），所有功能共享一个 SQLite 数据库
2. **UI 层**：可配置的 FloatingPanel 组件，所有浮窗类交互（dictation、note、meeting、live translate）共享一个窗口组件
3. **快捷键层**：分层快捷键系统，覆盖高频操作（dictation）、中频操作（note）、低频长时间操作（meeting/translate）

加新功能只需要：定义 mode 配置 + 配置 FloatingPanel 插槽 + 写 view 组件。不需要改存储层或创建新的窗口类。

---

## 一、核心数据模型

### 1.1 两个核心实体

**Entry** — 最小文本单位。每次语音产出的一段文字就是一个 entry。无论来自 dictation、agent 对话、voice note 还是 meeting transcript，都是一个 entry。

**Container** — 把相关 entry 组织在一起的上下文。一个 notebook、一次 agent 对话、一个 meeting session、一个"宝宝日记"专题，都是 container。Container 支持嵌套（parent_id），例如"宝宝日记"是一个 notebook container，里面每天的日记是子 container。

### 1.2 Entry 的固定字段

以下字段是所有 entry 共有的，作为数据库表的列存储：

- `id` — 唯一标识（UUID 或 ULID）
- `created_at` — 创建时间
- `source_type` — 来源类型，标识这条 entry 是从哪条管道来的。初始枚举值：dictation、agent、note、meeting。未来可扩展（journal、research 等），扩展时不需要改表结构。
- `role` — 这段文字是谁产出的。枚举值：user（用户说的）、agent（agent 回复的）、system（系统生成的，如会议摘要、周报）
- `mode` — 使用的模式名称（raw、polish、translate_en、agent、note、meeting 等），直接对应 fonos 的 mode 系统
- `raw_text` — STT 的原始输出，不经过任何 LLM 处理
- `processed_text` — 经过 mode processor 处理后的文本（如果 mode 是 raw，则与 raw_text 相同）
- `container_id` — 可为空。指向所属的 container。dictation 类 entry 通常没有 container（值为 null），note 和 meeting 类 entry 有 container。
- `audio_ref` — 可为空。对应音频文件的路径（相对于 ~/.fonos/audio/）
- `metadata` — JSON 格式的扩展字段，存储类型特有的数据（见下方）

### 1.3 Metadata JSON 的扩展约定

metadata 是类型特有数据的扩展口。加新功能时往 metadata 里加字段即可，不需要改表结构。

各类型的 metadata 字段约定：

**dictation 类型**：duration_ms（录音时长）、word_count、language（识别语言）、injected_to（注入到了哪个应用，如 "Cursor"、"Safari"）

**agent 类型**：skill（调用的技能名）、tool_calls（工具调用记录数组）、model（使用的 LLM 模型）、tokens_used

**note 类型**：tags（标签数组，如 ["idea", "fonos"]）、mood（可选的情绪标记）

**meeting 类型**：chunk_index（在这次会议中的片段序号）、speaker_hint（说话人提示）、timestamp_in_session（在会议中的时间戳，如 "00:12:34"）、duration_ms

**system 类型**（AI 生成的摘要等）：source_entries（生成这条摘要所基于的 entry id 列表）、generation_model、generation_prompt_summary

### 1.4 Container 的字段

- `id` — 唯一标识
- `type` — 容器类型。枚举值：notebook、conversation、meeting_session、journal、research。可扩展。
- `title` — 标题（用户命名或自动生成）
- `parent_id` — 可为空。指向父 container，用于嵌套（如"宝宝日记" notebook 下的每日子条目）
- `created_at` — 创建时间
- `updated_at` — 最后更新时间（最后一条 entry 写入时更新）
- `metadata` — JSON 格式的扩展字段

Container metadata 的约定：

**notebook 类型**：description（笔记本描述）、icon（可选的图标标识）、color（可选的颜色标记）

**conversation 类型**：model（对话使用的 LLM）、total_turns

**meeting_session 类型**：duration_total_ms、participant_count、audio_source（mic / blackhole）、summary_generated（是否已生成摘要）

### 1.5 Embedding 索引

为 recall / 语义搜索服务。每个 entry 可以有一个对应的 embedding 向量。存储方式：单独的 embeddings 表，entry_id 为主键，vector 为 BLOB。embedding 的生成可以异步进行（entry 写入后，后台 worker 计算 embedding 并写入）。

### 1.6 全文搜索

使用 SQLite 的 FTS5 扩展建立全文索引，覆盖 processed_text 和 raw_text。支持中英文混合搜索。FTS5 索引与 entries 表关联，在 entry 插入时同步更新。

---

## 二、输入管道

### 2.1 统一管道流程

所有功能共享同一条输入管道，差异仅在三个配置点：

1. **Voice input** — 用户说话，音频被捕获
2. **STT engine** — 语音转文字（Qwen3-ASR / Apple Speech，由配置决定）
3. **Mode processor** — 按当前 mode 处理文字（raw 不处理、polish 由 LLM 润色、translate 由 LLM 翻译、agent 走 agent pipeline）
4. **Entry 创建并写入 SQLite** — 这一步永远执行，不管后面的 output target 是什么。即使是 dictation 模式"用完即走"，entry 也已经存下来了。
5. **Output target 执行** — 按 mode 的 output_target 配置执行动作

### 2.2 Mode 定义的扩展

现有的 mode 定义结构需要增加三个字段：

- `output_target` — 这个 mode 产出的文字去往哪里。枚举值：
  - `cursor_injection` — 注入到当前光标位置（dictation 系列模式）
  - `append_to_container` — 追加到指定 container（note、meeting 模式）
  - `conversation` — 作为对话轮次处理（agent 模式）
  - `save_only` — 仅存储，不执行任何外部动作
  - `display_only` — 仅在浮窗上显示，不存储也不注入（live translate 模式；entry 仍然会写入 SQLite 但不追加到 container）
  
- `container_type` — 当 output_target 需要 container 时，自动创建什么类型的 container。仅在 output_target 为 append_to_container 或 conversation 时有意义。

- `auto_container` — 是否自动管理 container 生命周期。值为 `session`（每次进入模式自动创建新 container）、`reuse`（复用最近打开的同类型 container）、`pick`（让用户选择或创建 container）、`none`（不使用 container）。

现有模式的字段值（backward compatible，对已有行为无影响）：

- raw / polish / translate：output_target = cursor_injection, auto_container = none
- agent：output_target = conversation, container_type = conversation, auto_container = session

### 2.3 存储层保证

管道的第 4 步（写入 SQLite）必须在第 5 步（output target）之前完成。这确保即使 output target 执行失败（比如光标注入失败），文字也不会丢失。

音频文件的保存是可选的，由 mode 配置决定。如果 mode 配置了 `save_audio: true`（如 note、meeting 模式），音频文件保存到 `~/.fonos/audio/` 并在 entry 的 audio_ref 中记录路径。dictation 模式默认不保存音频。

---

## 三、Note 模式 — 交互设计

### 3.1 设计原则

笔记模式的交互必须做到两件事：**零思考进入**（想到就说）和**归属可控**（说的话会去到正确的地方）。

### 3.2 进入笔记模式

方式一：在 mode picker 中选择 "Note" pill（与 Raw、Polish、Agent 平级）。
方式二：全局快捷键（可配置，如 Option+N）直接进入 note 模式，跳过 mode picker。

### 3.3 Notebook 选择器

进入 note 模式后，屏幕上出现一个**极简的 notebook 选择器**。这是一个小型浮窗或菜单栏下拉面板中的一行，不是全屏界面。

选择器的内容：

- **默认选项：Quick Note（快速笔记）** — 总是排在第一个，已预选。选择后 entry 的 container_id 为 null，作为独立的灵感记录存储。用户什么都不选直接开始说话，就是 quick note。
- **最近使用的 notebook** — 最多显示 3-5 个最近活跃的 notebook，按最后更新时间排序。
- **所有 notebook** — 一个展开按钮，显示完整的 notebook 列表。
- **新建 notebook** — 一个"+"按钮，输入名称即创建。

交互流程：

1. 用户进入 note 模式 → 选择器出现，Quick Note 已预选
2. 如果用户直接开始说话 → 记录为 quick note（无 container）
3. 如果用户点击/选择一个 notebook → 选择器收起，后续所有说话内容追加到该 notebook
4. 选择器支持键盘导航：数字键 1-5 快速选择对应的 notebook，Enter 确认当前选择开始录音

### 3.4 Notebook 快捷键

用户可以为常用 notebook 绑定快捷键。在 settings 中配置。例如：

- Option+B → 直接进入 note 模式 + 选择"宝宝日记" notebook
- Option+N → 直接进入 note 模式 + 默认 Quick Note

这样高频使用的 notebook 是一键直达，零思考。

### 3.5 Note 模式下的录音行为

与 dictation 模式类似，按下快捷键说话，松开结束。但有一个区别：在 notebook 被选中的状态下，**可以连续多次按键说话**，每次都追加到同一个 notebook 的新 entry。不需要每次重新选择 notebook。

退出 note 模式的方式：切换到其他 mode（Raw、Agent 等），或者按 Escape。

### 3.6 Note 的处理方式

Note 模式的 processor 有两种可选行为（在 mode 配置中设定）：

- `raw` — 不做任何处理，直接存储 STT 输出。适合快速灵感记录，保留口语化表达。
- `light_polish` — 轻度润色，去除口头禅和重复，整理成书面化段落，但不改变内容含义。适合需要回顾阅读的笔记。

默认为 `light_polish`。用户可以在 settings 中修改默认值，也可以在进入 note 模式时临时切换。

---

## 四、显示层

### 4.1 设计原则

存储统一，view 不同。每种 view 本质上是对同一个 SQLite 数据库的不同 query + 不同的 UI 组件。

### 4.2 导航结构

主界面的导航栏包含以下 tab（可以是侧边栏 tab 或顶部 tab，取决于整体布局）：

- **Recent** — 时间线视图，全部 entry 按时间倒序
- **Notes** — 笔记视图，按 notebook 分组
- **Conversations** — Agent 对话视图
- **Meetings** — 会议记录视图（Phase 2）
- **Search** — 全局搜索（FTS + 语义搜索）

### 4.3 Recent View — 时间线

这是现有 history 的升级版。显示所有 entry，按时间倒序排列。

每条 entry 显示：
- 时间戳
- Source type 标签（小的彩色 badge：Dictation / Agent / Note / Meeting）
- processed_text 的预览（前 2-3 行）
- 如果属于某个 container，显示 container 名称作为副标题

点击某条 entry 展开查看完整内容。如果 entry 属于一个 container，提供"查看完整上下文"链接跳转到对应 view。

这个 view 的查询本质上就是 `SELECT * FROM entries ORDER BY created_at DESC LIMIT N`，加分页。

### 4.4 Notes View — 笔记

两级结构：

**第一级：Notebook 列表**
- 显示所有 type=notebook 的 container，按 updated_at 倒序
- 每个 notebook 卡片显示：标题、entry 数量、最后更新时间、预览（最近一条 entry 的前几句）
- 顶部有一个 "Quick Notes" 区域，显示 container_id 为 null 的 note 类 entry（独立灵感）
- 支持新建 notebook、重命名、删除、导出

**第二级：Notebook 内容**
- 点击某个 notebook 进入内容页
- 按时间排序显示该 notebook 下的所有 entry
- 每条 entry 显示时间、processed_text、可选的 audio 播放按钮
- 支持编辑 entry 的 processed_text（文字修正）
- 支持删除单条 entry
- 顶部有导出按钮（导出整个 notebook）

### 4.5 Conversations View — Agent 对话

显示所有 type=conversation 的 container，按 updated_at 倒序。

每个对话卡片显示：标题（可以是第一条消息的摘要或用户命名）、轮数、最后活动时间。

点击进入对话详情，UI 采用聊天气泡布局：
- role=user 的 entry 显示在右侧（用户说的）
- role=agent 的 entry 显示在左侧（agent 回复的）
- agent entry 如果有 tool_calls metadata，在气泡中显示工具调用的折叠面板

支持"继续对话"功能 — 在对话详情页直接说话，追加新的 entry 到这个 conversation container。

### 4.6 Meetings View（Phase 2）

显示所有 type=meeting_session 的 container。

这是最 rich 的 view，包含：
- 会议时间线（左侧时间轴，右侧 transcript 文字）
- 说话人标签（如果有 speaker_hint metadata）
- AI 生成的摘要（role=system 的 entry，显示在顶部）
- 关键信息提取：action items、决策点（由 AI 在会议结束时生成）
- 音频播放：点击任何一段 transcript 可以跳到对应的音频位置播放
- 导出：支持导出为 markdown 或 PDF（包含摘要 + 完整 transcript）

### 4.7 Search View — 全局搜索

搜索入口，覆盖所有 entry。

搜索方式：
- 关键词搜索 — 使用 FTS5 全文索引
- 语义搜索 — 使用 embedding 向量做余弦相似度匹配（Phase 2）
- 过滤器 — 按 source_type、时间范围、container 过滤

搜索结果显示：
- entry 预览 + 匹配关键词高亮
- source_type badge
- 所属 container 名称（如果有）
- 时间戳
- 点击跳转到 entry 在其 view 中的位置

这个 view 也是 recall 功能的 UI 表现 — 在 agent 模式下通过语音触发的 recall，本质上是在这个 search 能力上的语音接口。

---

## 五、导出功能

### 5.1 导出的通用模型

导出是一个通用函数，对所有 container type 都适用：

输入：container_id + 导出格式

流程：
1. 查询 container 下所有 entry，按 created_at 排序
2. 按格式渲染（见下方）
3. 如果 entry 有 audio_ref，可选地包含音频文件
4. 打包为文件夹或压缩包

### 5.2 支持的导出格式

**Markdown 文件夹** — 人类可读，最通用
- 一个 README.md 包含 container 的 title、描述、统计信息
- entry 按日期分组输出（每天一个 .md 文件，或所有 entry 在一个文件中按时间排列）
- 如果有音频文件，放在 audio/ 子目录

**JSON 文件** — 机器可读，完整保留所有 metadata
- 整个 container 的 entries 数组，每条 entry 包含所有字段

**独立 SQLite 文件** — 完全自包含
- 导出一个小的 .sqlite 文件，包含 entries 和 container 表（仅限于该 container 的数据）
- 可以直接被其他工具读取

### 5.3 导出入口

- 在 Notebook 内容页的顶部操作栏
- 在 Meeting 详情页的顶部操作栏
- 在 Conversation 详情页（导出对话记录）
- 在 Search 结果页（导出搜索结果集）

---

## 六、文件系统布局

```
~/.fonos/
├── fonos.db              # 单一 SQLite：entries, containers, embeddings, fts_index
├── audio/                # 音频文件，按日期组织
│   ├── 2026/
│   │   ├── 03/
│   │   │   ├── 26/
│   │   │   │   ├── {entry-id}.wav
│   │   │   │   └── ...
│   │   │   └── ...
│   │   └── ...
│   └── ...
├── exports/              # 导出产出物（用户手动触发）
│   ├── baby-diary-2026-03/
│   │   ├── README.md
│   │   ├── entries.md
│   │   └── audio/
│   └── ...
├── modes.json            # 自定义模式定义（已有）
├── skills/               # Agent skills 配置（已有）
└── model_caps.json       # 模型能力缓存（已有）
```

---

## 七、Mode 配置示例

以下是各模式的完整配置，展示 output_target 等新字段如何与现有字段配合。

**现有模式（补充新字段即可）：**

raw 模式：processor = none, output_target = cursor_injection, auto_container = none, save_audio = false

polish 模式：processor = llm_polish, output_target = cursor_injection, auto_container = none, save_audio = false

translate_en 模式：processor = llm_translate, output_target = cursor_injection, auto_container = none, save_audio = false

agent 模式：processor = agent_pipeline, output_target = conversation, container_type = conversation, auto_container = session, save_audio = false

**新增模式：**

note 模式：processor = llm_light_polish（可配置为 none）, output_target = append_to_container, container_type = notebook, auto_container = pick（弹出选择器）, save_audio = true（可配置）

meeting 模式（Phase 2）：processor = none（raw transcript）, output_target = append_to_container, container_type = meeting_session, auto_container = session（每次自动创建）, save_audio = true, audio_source = blackhole

journal 模式（Phase 2）：processor = llm_light_polish, output_target = append_to_container, container_type = journal, auto_container = reuse（复用今天的 journal entry；如果今天没有则自动创建）, save_audio = false

live_translate 模式（Phase 2）：processor = llm_translate, output_target = display_only（仅在浮窗上显示翻译结果，不注入光标也不存储到 container）, auto_container = none, save_audio = false, input_mode = continuous（连续监听麦克风，不需要按住热键）, panel_size = medium-large, panel_font_size = large（翻译文字需要大字号，对方要从对面看屏幕）

---

## 八、FloatingPanel UI 架构

### 8.1 设计原则

Fonos 的所有浮窗类交互不是 5 个独立的窗口，而是**同一个 FloatingPanel 组件的不同配置**。FloatingPanel 是一个 NSPanel（非激活型浮窗，不抢焦点），有 3 个可插拔的插槽区域。

每个 mode 通过配置决定 FloatingPanel 的行为和外观，不需要创建新的窗口类。

### 8.2 FloatingPanel 的 3 个插槽

**Header 插槽**（可选）：显示在面板顶部的上下文信息。

- Dictation 模式：无 header（面板尽可能小）
- Note 模式：Notebook 选择器（Quick Note / 最近 notebook 列表）
- Live translate 模式：语言方向标签（如 "ZH → EN"）
- Meeting 模式：会议名称 + 计时器
- Agent 模式：Session 标题

**Body 插槽**（必须）：面板的主体内容区域。

- Dictation / Note 模式：波形动画 + 实时转写文字
- Live translate 模式：大字号的翻译文字流式显示（需要对方能从对面看到）
- Meeting 模式：滚动式 transcript 列表，新的 chunk 不断追加在底部
- Agent 模式：聊天气泡布局

**Footer 插槽**（可选）：底部的状态信息或操作按钮。

- Dictation 模式：无 footer
- Note 模式：当前 notebook 名称 + entry 计数
- Meeting 模式：停止录制按钮 + 已录制时长
- Agent 模式：输入状态指示

### 8.3 FloatingPanel 的配置维度

每个 mode 在 mode 定义中增加以下 panel 配置字段：

- `panel_size`：small（Dictation，约 300×80）、medium（Note，约 300×160）、medium-large（Live translate，约 400×120）、large（Meeting/Agent，约 400×300+）
- `panel_position`：near_cursor（Dictation，跟随光标位置）、fixed_corner（Note/Meeting/Agent，固定在屏幕某角，可拖拽）、fixed_visible（Live translate，固定在屏幕中下方显眼位置）
- `panel_dismiss`：auto_on_release（Dictation，松开热键自动消失）、manual_esc（Note/Agent，按 Esc 或切换 mode 退出）、manual_toggle（Meeting/Live translate，再按一次热键停止）
- `panel_persist_between_entries`：false（Dictation，每次说完就消失）、true（Note，说完一条后面板保留，可以继续说下一条）
- `input_mode`：hold_to_talk（Dictation/Note/Agent，按住热键说话）、continuous（Meeting/Live translate，开启后持续监听）
- `panel_font_size`：normal（默认）、large（Live translate，文字放大方便对方阅读）

### 8.4 不同模式下 FloatingPanel 的具体行为

**Dictation 模式**：按住热键 → 面板出现在光标附近 → 显示波形和实时转写 → 松开热键 → 文字注入光标 → 面板淡出消失。整个过程 < 1 秒。

**Note 模式**：按下 note 热键 → 面板出现在固定位置 → header 显示 notebook 选择器（Quick Note 已预选） → 用户可选择 notebook 或直接开始说话 → 按住说话热键 → body 显示波形和实时转写 → 松开 → entry 存入 notebook → 面板保留（footer 显示当前 notebook + "1 entry saved"） → 可以继续按住说下一条 → 按 Esc 退出 note 模式，面板消失。

**Live translate 模式**：按下 translate 热键 → 面板出现在屏幕中下方 → header 显示 "ZH → EN" → 麦克风开始连续监听 → body 流式显示翻译后的文字，字号较大 → 用户持续说话，翻译持续更新 → 再按一次热键停止，面板消失。这个模式下 entry 可以选择是否存储（默认不存储到 container，因为这是即时翻译场景）。

**Meeting 模式**：按下 meeting 热键 → 面板出现在固定位置 → header 显示"Meeting Recording" + 计时器开始 → BlackHole 音频源开始连续捕获 → body 流式显示 transcript，新 chunk 不断追加滚动 → footer 显示停止按钮 → 按停止或再按热键 → 触发 AI 摘要生成 → 摘要作为 system role 的 entry 存入 meeting_session container → 面板消失。

**Agent 模式**：按下 agent 热键 → 面板出现在固定位置 → body 显示聊天界面 → 按住说话 → agent 处理 → 回复显示 → 面板保留等待下一轮 → 按 Esc 退出。

### 8.5 实现建议

FloatingPanel 在 Rust 层是一个 NSPanel wrapper，提供以下 API：

- show(config) — 按配置显示面板
- update_header(component) — 更新 header 区域
- update_body(component) — 更新 body 区域
- append_to_body(text) — 追加文字到 body（流式显示用）
- update_footer(component) — 更新 footer 区域
- dismiss(animation) — 关闭面板
- resize(size) — 调整面板大小
- reposition(position) — 调整面板位置

React 前端通过 Tauri command 调用这些 API。每个 mode 的 React 组件负责决定往插槽里放什么内容，FloatingPanel 只负责窗口管理。

---

## 九、快捷键架构

### 9.1 三层快捷键

快捷键分三层，对应不同的使用频率和交互模式：

**Layer 1 — Mode 快捷键（内置，最高频）**

这些是每天使用几十次的核心操作，交互方式是 hold-to-talk（按住说话）。

- 主热键（Fn / CapsLock / 可配置）：按住 → 当前 dictation mode 开始录音 → 松开 → 处理并注入
- Option+A（可配置）：Agent 模式面板开关

当前 dictation mode 的切换在菜单栏 popover 中的 mode picker 里进行（Raw / Polish / Translate），切换后主热键自动使用新 mode。

**Layer 2 — Note 快捷键（用户可配置，中频）**

这些是每天使用几次的笔记操作，交互方式是 tap-to-enter-mode（按一次进入 note 模式，然后 hold-to-talk 说话）。

- Option+N（默认）：进入 note 模式 + Quick Note（无 container）
- Option+1 ~ Option+5（用户在 settings 中配置）：进入 note 模式 + 直接选中指定 notebook

在 settings 中，用户可以把任意 notebook 绑定到 Option+1 ~ Option+5。绑定后，这个快捷键一键直达：不需要先进入 note 模式再选择 notebook。

如果用户按了 Option+N（未绑定特定 notebook），则显示 notebook 选择器让用户选择。

**Layer 3 — Session 快捷键（toggle 式，低频但长时间运行）**

这些是不常启动但运行时间长的功能，交互方式是 toggle（按一次开始，再按一次停止）。

- Option+M（可配置）：开始/停止 meeting 录制
- Option+T（可配置）：开始/停止 live translate 显示

### 9.2 快捷键冲突处理

所有快捷键在 settings 中统一管理，显示当前绑定和冲突检测。如果新绑定的快捷键与系统快捷键或其他应用冲突，提示用户并建议替代方案。

### 9.3 快捷键配置持久化

快捷键配置存储在 `~/.fonos/config.json` 的 hotkeys 字段中（已有的配置文件），与 mode 配置和其他设置放在一起。

---

## 十、数据迁移

现有的 history 数据需要迁移到新的 entries 表。迁移策略：

1. 现有 history 表中的每条记录变成一条 entry
2. source_type 根据原始 mode 字段推断：如果 mode 是 agent 相关则 source_type = agent，否则 source_type = dictation
3. role 默认为 user（dictation 都是用户说的）；agent 对话中的 AI 回复 role = agent
4. container_id 默认为 null（现有的 dictation 记录本来就是独立的）
5. agent 对话按 session 分组，每组创建一个 type=conversation 的 container，对应 entry 的 container_id 指向它
6. metadata 从现有字段中提取能提取的信息
7. 迁移后保留旧表为备份（重命名为 history_backup），确认一切正常后再删除

---

## 十一、实施建议

### Phase 1（核心改动，最小可用）

1. 创建 entries 表和 containers 表
2. 修改现有的数据写入逻辑：所有 entry 统一写入 entries 表
3. 给 mode 定义加上 output_target、container_type、auto_container 字段以及 panel 配置字段
4. 现有 dictation 和 agent 功能不受影响（字段值与当前行为一致）
5. 迁移现有 history 数据
6. 改造现有 history UI 为 Recent view（加上 source_type badge）
7. 重构现有 dictation 浮窗为可配置的 FloatingPanel 组件（header/body/footer 插槽化），现有 dictation 行为不变

### Phase 2（笔记功能）

1. 实现 note 模式：FloatingPanel 配置（header = notebook 选择器，body = 波形+文字，footer = notebook 名称），notebook 选择器 UI + 笔记录入流程
2. 实现 Notes view：notebook 列表 + 内容详情
3. 实现导出功能
4. 实现三层快捷键系统：Layer 1（dictation/agent 热键，已有）、Layer 2（Option+N 及 Option+1~5 notebook 快捷键）
5. Settings 中增加快捷键管理和 notebook 绑定配置

### Phase 3（丰富视图 + 长时间模式）

1. Search view（FTS5 全文搜索 + 过滤器）
2. Conversations view（聊天气泡布局）
3. Meeting 模式：FloatingPanel 配置（header = 计时器，body = 滚动 transcript，footer = 停止按钮）、BlackHole 音频源 + 连续 transcript + 结束时 AI 摘要
4. Meeting view（时间线 + 摘要 + 音频播放）
5. Live translate 模式：FloatingPanel 配置（header = 语言方向，body = 大字号翻译文字，dismiss = toggle）、连续麦克风监听 + 流式翻译显示
6. Layer 3 快捷键（Option+M meeting toggle, Option+T translate toggle）
7. Embedding 索引 + 语义搜索

### Phase 4（高级功能）

1. Journal 模式（每日自动创建 + 连续性提示）
2. Research 模式（agent + web search + 结果存储）
3. 周报自动生成（聚合一周 entries + LLM 总结）
4. Recall 语音接口（agent 模式下的语义搜索触发）
5. DuckDB 分析层（如果分析需求增长）

---

## 十二、UI 设计要求

### 12.1 整体风格

- macOS native 风格，与系统 UI 保持一致
- 轻量、快速、不打断工作流
- 菜单栏 popover 保持紧凑，不做成全功能 app 窗口
- 暗色模式必须支持
- 动画和过渡要流畅但克制

### 12.2 Notebook 选择器的 UI 要求

选择器应该是极简的，不能打断说话的冲动。参考交互：

- 出现方式：在进入 note 模式时，在 dictation 浮窗附近出现一个小面板
- 宽度与 dictation 浮窗一致或稍宽
- 高度自适应，最多显示 5 个选项（Quick Note + 最近 4 个 notebook）
- 每个选项一行：notebook 名称 + 最后更新时间 + 快捷键提示（如果有）
- Quick Note 选项有明显的"默认已选中"视觉状态
- 支持键盘导航（↑↓ 选择，Enter 确认，数字键直选）
- 选择后选择器收起，不再占用屏幕空间
- 如果用户不做任何选择直接开始说话，默认使用 Quick Note

### 12.3 Recent View 的 UI 要求

- 列表式布局，每条 entry 是一个卡片
- 卡片左侧有 source_type 的彩色竖条或 badge（用颜色区分类型，如 dictation 灰色、agent 紫色、note 绿色、meeting 橙色）
- 卡片内容：时间戳（次要文字）、processed_text 预览（2-3 行后截断）、container 名称（如果有，显示为链接）
- 下拉加载更多（infinite scroll 或 load more 按钮）
- 支持按 source_type 过滤（顶部的 filter pill 或 tab）

### 12.4 Notes View 的 UI 要求

**Notebook 列表页：**
- 网格或列表布局，每个 notebook 是一个卡片
- 卡片显示：标题、entry 数量、最后更新时间、最近一条 entry 的文字预览
- 顶部有 "Quick Notes" 入口（独立灵感的集合）
- 右上角有"新建 notebook"按钮
- 支持长按/右键菜单：重命名、删除、导出、设置快捷键

**Notebook 内容页：**
- 顶部：notebook 标题 + 操作栏（导出、设置、返回）
- 内容区：entry 按时间排序显示，每条 entry 之间有清晰的分隔
- 每条 entry 显示：时间戳、processed_text、如果有 audio_ref 则显示播放按钮
- 支持点击 entry 展开编辑 processed_text
- 支持向下持续滚动查看历史 entry
- 在 note 模式下且当前 notebook 被选中时，新的 entry 实时出现在列表顶部

### 12.5 各 view 之间的跳转

- 在 Recent view 中点击带 container 的 entry → 跳转到对应 view 的对应位置
- 在 Search 结果中点击 entry → 跳转到 entry 在其原始 view 中的位置
- 所有 view 之间的跳转应该有平滑的过渡动画
- 支持后退导航（返回上一个 view）
