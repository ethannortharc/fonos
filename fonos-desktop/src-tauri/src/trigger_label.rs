//! `workflow-{id}@{trigger_index}` hotkey labels — one binding per Hotkey
//! chip so each key carries its own capture behavior.

pub fn hotkey_label(wf_id: &str, trigger_idx: usize) -> String {
    format!("workflow-{wf_id}@{trigger_idx}")
}

/// Splits a dispatch label into the base `workflow-{id}` label (what
/// `resolve_trigger_target` expects) and the trigger index. Suffix-less
/// legacy labels map to index 0.
pub fn parse_hotkey_label(label: &str) -> (&str, usize) {
    match label.rsplit_once('@') {
        Some((base, idx)) => match idx.parse::<usize>() {
            Ok(i) => (base, i),
            Err(_) => (label, 0),
        },
        None => (label, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_roundtrip() {
        let l = hotkey_label("wf.dictation", 2);
        assert_eq!(l, "workflow-wf.dictation@2");
        assert_eq!(parse_hotkey_label(&l), ("workflow-wf.dictation", 2));
    }

    #[test]
    fn legacy_label_without_suffix() {
        assert_eq!(parse_hotkey_label("workflow-wf.custom-123"), ("workflow-wf.custom-123", 0));
    }

    #[test]
    fn non_numeric_suffix_is_not_an_index() {
        assert_eq!(parse_hotkey_label("workflow-wf.x@abc"), ("workflow-wf.x@abc", 0));
    }
}
