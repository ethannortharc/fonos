//! Built-in widget and workflow definitions that ship with Fonos.
//!
//! These are the base layer of [`crate::workflow::engine::effective_widgets`]
//! / [`crate::workflow::engine::effective_workflows`]: a user's config can
//! override a built-in (by re-declaring the same id) or add new entries, but
//! the built-ins themselves are never deletable.
//!
//! The LLM widgets (`llm.polish`, `llm.formal`, `llm.translate`,
//! `llm.summarize`, `llm.listen`) copy their `system` / `user_template` /
//! `temperature` **verbatim** from the matching mode in
//! [`crate::modes::built_in_modes`], so the workflow engine and the legacy
//! mode system stay byte-for-byte in sync.

use crate::workflow::model::{WidgetDef, WidgetRole, WorkflowDef};

/// Build a built-in [`WidgetDef`] (always `builtin: true`).
fn widget(
    id: &str,
    role: WidgetRole,
    type_tag: &str,
    name: &str,
    icon: &str,
    props: serde_json::Value,
) -> WidgetDef {
    WidgetDef {
        id: id.to_string(),
        role,
        type_tag: type_tag.to_string(),
        name: name.to_string(),
        icon: icon.to_string(),
        props,
        builtin: true,
    }
}

/// Build a built-in `llm` processor widget, copying `system` / `user_template`
/// / `temperature` verbatim from the mode `mode_id` in
/// [`crate::modes::built_in_modes`].
fn llm_widget(id: &str, name: &str, icon: &str, mode_id: &str, max_tokens: u32) -> WidgetDef {
    let modes = crate::modes::built_in_modes();
    let mode = modes
        .get(mode_id)
        .unwrap_or_else(|| panic!("built-in mode '{mode_id}' missing"));
    widget(
        id,
        WidgetRole::Processor,
        "llm",
        name,
        icon,
        serde_json::json!({
            "system": mode.system.clone().unwrap_or_default(),
            "user_template": mode.user_template.clone().unwrap_or_default(),
            "model_profile": "",
            "temperature": mode.temperature,
            "max_tokens": max_tokens,
            "output_language": "auto",
            "vocab_books": [],
        }),
    )
}

/// Build a built-in [`WorkflowDef`] (always `builtin: true`, empty hotkey).
fn workflow(id: &str, name: &str, icon: &str, source: &str, processors: &[&str], outputs: &[&str]) -> WorkflowDef {
    WorkflowDef {
        id: id.to_string(),
        name: name.to_string(),
        icon: icon.to_string(),
        hotkey: String::new(),
        source: source.to_string(),
        processors: processors.iter().map(|s| s.to_string()).collect(),
        outputs: outputs.iter().map(|s| s.to_string()).collect(),
        builtin: true,
    }
}

/// The built-in widgets: sources, processors, and outputs that ship with the
/// app. Ids are globally unique and role-prefixed (`src.*`, `stt.*`, `llm.*`,
/// `out.*`).
pub fn built_in_widgets() -> Vec<WidgetDef> {
    use WidgetRole::{Output, Processor, Source};
    vec![
        // ── Sources ──────────────────────────────────────────────────────
        widget("src.selection", Source, "selection", "选区", "🖱", serde_json::json!({})),
        widget(
            "src.mic-hold",
            Source,
            "microphone",
            "麦克风·按住",
            "🎙",
            serde_json::json!({ "capture": "hold" }),
        ),
        widget(
            "src.mic-toggle",
            Source,
            "microphone",
            "麦克风·切换",
            "🎙",
            serde_json::json!({ "capture": "toggle" }),
        ),
        // ── Processors ───────────────────────────────────────────────────
        widget(
            "stt.default",
            Processor,
            "stt",
            "默认转写",
            "✍️",
            serde_json::json!({
                "model_profile": "",
                "stt_prompt": "",
                "vocab_books": [],
                "temperature": 0.0,
            }),
        ),
        llm_widget("llm.polish", "润色", "✨", "polish", 4096),
        llm_widget("llm.formal", "正式", "👔", "formal", 4096),
        llm_widget("llm.translate", "翻译", "🌐", "translate", 4096),
        llm_widget("llm.summarize", "总结", "📌", "summarize", 4096),
        llm_widget("llm.listen", "朗读摘要", "🎧", "listen", 2048),
        // ── Outputs ──────────────────────────────────────────────────────
        widget(
            "out.insert",
            Output,
            "insert",
            "插入",
            "⌨️",
            serde_json::json!({ "strategy": "paste", "press_enter": false }),
        ),
        widget("out.replace", Output, "replace", "替换选区", "🔁", serde_json::json!({})),
        widget("out.clipboard", Output, "clipboard", "剪贴板", "📋", serde_json::json!({})),
        widget(
            "out.panel",
            Output,
            "panel",
            "悬浮板·默认",
            "🪟",
            serde_json::json!({ "markdown": false }),
        ),
        widget(
            "out.speak",
            Output,
            "speak",
            "朗读",
            "🔊",
            serde_json::json!({ "voice_profile": "", "voice": "default" }),
        ),
        widget(
            "out.quicknote",
            Output,
            "notebook",
            "快速笔记",
            "📓",
            serde_json::json!({ "container_id": 0 }),
        ),
    ]
}

/// The built-in workflows wiring the built-in widgets into runnable pipelines.
/// All ship with an empty hotkey — user key bindings are supplied by migration
/// or the settings UI.
pub fn built_in_workflows() -> Vec<WorkflowDef> {
    vec![
        workflow("wf.dictation", "听写", "🎤", "src.mic-hold", &["stt.default"], &["out.insert"]),
        workflow(
            "wf.translate-pop",
            "翻译弹框",
            "🌐",
            "src.selection",
            &["llm.translate"],
            &["out.panel"],
        ),
        workflow(
            "wf.summarize-pop",
            "总结弹框",
            "📌",
            "src.selection",
            &["llm.summarize"],
            &["out.panel"],
        ),
        workflow("wf.listen", "朗读", "🎧", "src.selection", &["llm.summarize"], &["out.speak"]),
        workflow("wf.note", "记笔记", "📓", "src.mic-hold", &["stt.default"], &["out.quicknote"]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::workflow::engine::effective_workflows;

    #[test]
    fn built_ins_are_internally_consistent() {
        let widgets = built_in_widgets();
        let ids: std::collections::HashSet<_> = widgets.iter().map(|w| w.id.clone()).collect();
        assert_eq!(ids.len(), widgets.len(), "duplicate widget ids");
        for wf in built_in_workflows() {
            assert!(ids.contains(&wf.source), "{} source missing", wf.id);
            for p in &wf.processors {
                assert!(ids.contains(p));
            }
            for o in &wf.outputs {
                assert!(ids.contains(o));
            }
            assert!(wf.builtin && wf.hotkey.is_empty());
        }
        // llm.polish 的 prompt 与 built_in_modes 逐字一致
        let modes = crate::modes::built_in_modes();
        let polish = widgets.iter().find(|w| w.id == "llm.polish").unwrap();
        assert_eq!(
            polish.props["system"].as_str().unwrap(),
            modes["polish"].system.as_deref().unwrap()
        );
    }

    #[test]
    fn effective_overlays_builtin_by_id_and_appends_custom() {
        let mut cfg = AppConfig::default();
        cfg.workflows.push(WorkflowDef {
            id: "wf.dictation".into(),
            name: "听写".into(),
            hotkey: "cmd+shift+space".into(),
            source: "src.mic-hold".into(),
            processors: vec!["stt.default".into(), "llm.polish".into()],
            outputs: vec!["out.insert".into()],
            icon: String::new(),
            builtin: true,
        });
        cfg.workflows.push(WorkflowDef {
            id: "wf.custom-1".into(),
            name: "我的".into(),
            hotkey: String::new(),
            source: "src.selection".into(),
            processors: vec![],
            outputs: vec!["out.insert".into()],
            icon: String::new(),
            builtin: false,
        });
        let eff = effective_workflows(&cfg);
        let d = eff.iter().find(|w| w.id == "wf.dictation").unwrap();
        assert_eq!(d.hotkey, "cmd+shift+space");
        assert_eq!(d.processors.len(), 2);
        assert!(eff.iter().any(|w| w.id == "wf.custom-1"));
        assert_eq!(eff.iter().filter(|w| w.id == "wf.dictation").count(), 1);
    }
}
