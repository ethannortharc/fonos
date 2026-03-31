# Fonos v2 — Meeting Mode Design

## 概述

Meeting mode 是 fonos 的长时间运行模式，用于实时记录会议内容。支持面对面会议（仅麦克风）和远程会议（麦克风 + 系统音频），产出实时 transcript、说话人标注、AI 摘要和行动项提取。

---

## 一、音频捕获架构

### 1.1 两路音频

会议场景有两路音频需要捕获：

**本地音频（你的声音）**：通过麦克风输入捕获。fonos 已有 CoreAudio 麦克风捕获能力，直接复用。

**远端音频（其他参会者的声音）**：通过 WeChat、Teams、Google Meet、Zoom 等会议应用播放，走系统音频输出通道。需要捕获系统音频输出（loopback capture）。

### 1.2 系统音频捕获方案

**主方案：ScreenCaptureKit（macOS 13+）**

ScreenCaptureKit 是 Apple 原生的屏幕和音频捕获 API。fonos 使用其**纯音频模式**（不捕获屏幕画面，只捕获系统音频输出）。

优势：
- 零配置：用户不需要安装任何驱动或虚拟音频设备
- 原生 API：Apple 官方支持，不会被系统更新破坏
- 低延迟：直接从系统音频管道读取
- Rust 支持：screencapturekit-rs crate 提供 Rust 绑定
- 功耗低：不需要额外的音频路由或混合

限制：
- 需要"屏幕录制"权限（macOS 系统弹窗，用户授权一次即可）
- macOS 13 (Ventura) 及以上版本

**备选方案：BlackHole（macOS 12 及以下）**

BlackHole 是开源虚拟音频设备驱动，创建一条"软件音频线"。

使用方式：
- 用户安装 BlackHole 驱动
- 在 Audio MIDI Setup 中创建 Multi-Output Device（扬声器 + BlackHole）
- 将系统音频输出设为 Multi-Output Device
- fonos 从 BlackHole 输入端读取系统音频

限制：
- 需要用户手动安装和配置
- 驱动可能在 macOS 大版本更新后失效
- 只作为 ScreenCaptureKit 不可用时的 fallback

### 1.3 双通道录制

fonos 在 meeting mode 下同时捕获两路音频，合成为双声道 WAV：

- Channel L（左声道）：麦克风输入 = 你的声音
- Channel R（右声道）：系统音频输出 = 远端参会者的声音

这样做的核心好处是**说话人识别几乎免费** — 不需要任何 speaker diarization 模型，声道本身就自带了 "Me" vs "Others" 的标签。

对于面对面会议（只有麦克风，没有系统音频），录制为单声道。如果需要更细粒度的说话人区分（区分多个远端参会者），可以通过 WhisperKit 的 SpeakerKit（基于 pyannote）对右声道做进一步的 speaker diarization。

### 1.4 音频设备选择

Meeting mode 需要在 settings 中或启动时选择音频设备：

**麦克风选择**：列出所有可用的 CoreAudio 输入设备，让用户选择。默认使用系统默认麦克风。

**系统音频来源**：自动检测是否支持 ScreenCaptureKit。如果支持，默认使用；如果不支持，提示用户安装 BlackHole 并选择 BlackHole 设备。

**会议类型快捷选择**：在 meeting panel 的 header 提供两个预设：
- "Remote meeting"：双通道（麦克风 + 系统音频），适用于 WeChat/Teams/Meet/Zoom
- "In-person meeting"：单通道（仅麦克风），适用于面对面

---

## 二、STT 处理管道

### 2.1 分段转写

Meeting mode 的 STT 与 dictation 不同 — 它是**连续流式**的，不是按住说话。音频流被切成固定长度的 chunk（建议 10-15 秒），每个 chunk 独立送入 STT 引擎。

处理流程：
1. 音频流持续捕获，写入环形缓冲区
2. VAD（Voice Activity Detection）检测到语音活动时，标记 chunk 边界
3. 每个 chunk 发送到 STT 引擎（本地 Qwen3-ASR 或 Mac LAN 的 oMLX）
4. STT 返回 transcript text + word-level timestamps
5. 如果是双声道，左右声道分别做 STT，合并时交替按时间排序

### 2.2 Speaker 标注

**双声道模式（远程会议）**：左声道 transcript 自动标注为 "Me"，右声道 transcript 自动标注为 "Others"（或会议应用中对方的名字，如果能识别的话）。

**单声道模式（面对面会议）**：使用 speaker diarization 模型（WhisperKit SpeakerKit / pyannote）区分不同说话人。标注为 "Speaker 1"、"Speaker 2" 等，用户可以在会后手动给 speaker 命名。

### 2.3 实时展示

Transcript 在 FloatingPanel 的 body 区域流式展示：
- 每个 chunk 转写完成后追加到面板底部
- 自动滚动到最新内容
- 每段 transcript 前面显示 speaker 标签和时间戳
- 新出现的文字有短暂的高亮动画，然后渐变为正常颜色

---

## 三、数据存储

### 3.1 Entry 结构

每个 STT chunk 的转写结果存为一条 entry：

- source_type: "meeting"
- role: "user"（如果是你说的）或 "participant"（如果是远端说的）
- container_id: 指向本次会议的 meeting_session container
- raw_text: STT 原始输出
- processed_text: 与 raw_text 相同（meeting mode 不做 LLM 后处理，保持原始 transcript）
- audio_ref: 对应 audio chunk 文件的路径
- metadata:
  - chunk_index: 在这次会议中的片段序号（0, 1, 2, ...）
  - speaker_hint: "me" / "others" / "speaker_1" 等
  - timestamp_in_session: 在会议中的相对时间（如 "00:05:23"）
  - channel: "L" / "R" / "mono"
  - duration_ms: 这个 chunk 的时长

### 3.2 Container 结构

每次 meeting 自动创建一个 container：

- type: "meeting_session"
- title: 自动生成（日期 + 时间，如 "Meeting 2026-03-27 14:30"），用户可以修改
- metadata:
  - duration_total_ms: 会议总时长
  - audio_source: "screencapturekit" / "blackhole" / "mic_only"
  - channel_mode: "dual" / "mono"
  - participant_count: 检测到的说话人数量
  - summary_generated: 是否已生成 AI 摘要
  - meeting_app: 会议应用名称（如 "Google Meet"、"WeChat"，可选，通过前台 app 检测推断）

### 3.3 音频文件存储

完整的会议音频存为一个连续文件，不按 chunk 切割：

```
~/.fonos/audio/meetings/
└── 2026-03-27-1430-meeting.wav   # 完整的双声道 WAV
```

entry 的 audio_ref 记录文件路径 + chunk 的起止时间偏移量，这样在 UI 中点击某段 transcript 时可以精确定位到音频的对应位置播放。

---

## 四、会议结束后处理

### 4.1 AI 摘要生成

会议结束时（用户按停止），触发 AI 摘要流程：

1. 收集本次 meeting_session 下所有 entry 的 processed_text
2. 按时间排序拼接为完整 transcript，带 speaker 标签
3. 发送到 LLM（本地 Qwen3 或 OpenRouter），prompt 要求生成：
   - 会议摘要（3-5 句话概括主要内容）
   - 关键讨论点（按 topic 分组）
   - 行动项（Action items，格式：谁做什么，什么时候之前）
   - 决策记录（会上达成的决定）
4. 摘要结果存为一条新的 entry：
   - source_type: "meeting"
   - role: "system"（AI 生成的）
   - container_id: 同一个 meeting_session
   - metadata.source_entries: 基于哪些 entry 生成的
   - metadata.generation_model: 使用的 LLM 模型

### 4.2 用户编辑

摘要生成后，用户可以在 Meeting detail view 中：
- 修改摘要内容
- 给 speaker 标签重命名（"Speaker 1" → "张三"）
- 标记重要段落
- 删除不相关的 transcript 段落（如闲聊）
- 手动添加行动项

### 4.3 导出

支持导出整个 meeting session：
- Markdown 格式：摘要 + 行动项 + 完整 transcript（带 speaker 和时间戳）
- 如果开启了 vault sync，自动同步到 Obsidian vault 的 meetings/ 子文件夹

---

## 五、FloatingPanel 配置

Meeting mode 的 FloatingPanel 配置：

- panel_size: large（约 400×300，需要显示滚动的 transcript）
- panel_position: fixed_corner（固定在屏幕右下角，不遮挡会议应用窗口）
- panel_dismiss: manual_toggle（再按 Option+M 停止录制并关闭面板）
- panel_persist_between_entries: true（持续保持打开）
- input_mode: continuous（开启后持续监听，不需要按住说话）

Header 插槽内容：
- 会议名称（可编辑）
- 录制时长计时器（实时更新）
- 录制状态指示（红点闪烁）
- 会议类型标签（Remote / In-person）

Body 插槽内容：
- 滚动 transcript 列表
- 每段前面有 speaker 标签（彩色 badge）和时间戳
- 自动滚到底部，但用户可以手动上滚查看历史（此时暂停自动滚动，新内容不强制拉回底部）

Footer 插槽内容：
- 停止录制按钮
- 实时字数统计
- 音频电平指示（可视化当前是否有人在说话）

---

## 六、Mode 定义

meeting mode 的 mode 配置：

- name: "meeting"
- processor: none（raw transcript，不做 LLM 后处理）
- output_target: append_to_container
- container_type: meeting_session
- auto_container: session（每次启动自动创建新 container）
- save_audio: true
- audio_source: auto（优先 ScreenCaptureKit，fallback BlackHole）
- input_mode: continuous
- channel_mode: auto（检测到系统音频可用时用 dual，否则 mono）
- post_session_actions: ["generate_summary"]（会话结束时触发的动作）

---

## 七、快捷键

Layer 3 快捷键：Option+M（toggle）

- 第一次按下：启动 meeting mode → FloatingPanel 出现 → 开始录制
- 再次按下：停止录制 → 触发 AI 摘要 → FloatingPanel 显示摘要预览 → 几秒后关闭（或用户手动关闭）

---

## 八、Meeting Detail View

在主 app 的 Meetings tab 中，每个 meeting session 有一个详情页，这是最 rich 的 view：

**头部区域**：
- 会议标题（可编辑）
- 日期、时长、参与人数
- AI 摘要（可折叠/展开）
- 行动项列表（可勾选完成状态）

**时间线区域**：
- 左侧时间轴，右侧 transcript 文字
- Speaker 标签用不同颜色区分
- 点击任何一段 transcript → 播放对应时间段的音频
- 支持搜索 transcript 中的关键词
- 可以给任何段落添加备注或标记为"重要"

**底部操作栏**：
- 导出按钮（Markdown / JSON）
- 重新生成摘要（如果修改了 speaker 标签后想重新生成）
- 删除会议记录

---

## 九、权限和首次使用

### 9.1 需要的权限

- 麦克风权限（fonos 已有）
- 屏幕录制权限（ScreenCaptureKit 需要，仅用于系统音频捕获，不录制屏幕画面）

### 9.2 首次使用引导

用户第一次按 Option+M 进入 meeting mode 时：

1. 如果麦克风权限未授权 → 弹出系统权限请求
2. 如果屏幕录制权限未授权 → 弹出系统权限请求，并在 fonos 内显示提示说明"此权限仅用于捕获会议应用的音频，fonos 不会录制屏幕画面"
3. 如果两个权限都已授权 → 直接进入 meeting mode

如果用户拒绝屏幕录制权限 → meeting mode 回退到 mic-only 模式（单通道，仅录你的声音），并在 header 显示提示"仅录制麦克风，远端参会者的声音不会被录制"

---

## 十、实施优先级

### Phase 1（最小可用）
1. 麦克风单通道录制（面对面会议）
2. 连续 STT 转写 + 流式展示
3. Meeting session container 自动创建
4. Entry 按 chunk 存储
5. FloatingPanel meeting 配置
6. Option+M toggle 快捷键

### Phase 2（远程会议）
1. ScreenCaptureKit 系统音频捕获
2. 双通道录制（mic L + system R）
3. 双声道分别 STT + speaker 标注
4. BlackHole fallback

### Phase 3（后处理）
1. 会议结束后 AI 摘要生成
2. 行动项和决策提取
3. Meeting detail view（时间线 + 音频播放）
4. Vault sync（会议记录同步到 Obsidian）
5. 导出功能

### Phase 4（高级）
1. SpeakerKit / pyannote 多说话人 diarization（面对面会议多人区分）
2. Speaker 声纹学习（记住常见参会者的声音）
3. 实时翻译叠加（会议中实时把中文翻译成英文或反过来）
4. 会议模板（standup / brainstorm / 1-on-1 有不同的摘要 prompt）
