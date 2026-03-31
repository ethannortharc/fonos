/// Built-in desktop skills for the Fonos agent on macOS.
///
/// This module provides five platform-specific skills:
/// - [`ShellSkill`] — execute shell commands with safety filtering
/// - [`AppleScriptSkill`] — run AppleScript via `osascript`
/// - [`AppControlSkill`] — open, switch, and list applications
/// - [`ClipboardSkill`] — read/write the macOS clipboard
/// - [`SystemInfoSkill`] — query hostname, memory, disk, OS, uptime, CPU

pub mod applescript;
pub mod app_control;
pub mod clipboard;
pub mod shell;
pub mod system_info;

pub use applescript::AppleScriptSkill;
pub use app_control::AppControlSkill;
pub use clipboard::ClipboardSkill;
pub use shell::ShellSkill;
pub use system_info::SystemInfoSkill;

use std::sync::Arc;
use fonos_core::agent::registry::SkillRegistry;
use fonos_core::agent::safety::CommandSafetyFilter;

/// Register all built-in desktop skills into `registry`.
///
/// The [`CommandSafetyFilter`] is shared between the registry and the
/// [`ShellSkill`] so the same safety rules apply to both direct calls and
/// calls routed through the registry.
pub fn register_desktop_skills(
    registry: &mut SkillRegistry,
    safety: Arc<CommandSafetyFilter>,
) {
    registry.register(Box::new(ShellSkill::new(Arc::clone(&safety))));
    registry.register(Box::new(AppleScriptSkill));
    registry.register(Box::new(AppControlSkill));
    registry.register(Box::new(ClipboardSkill));
    registry.register(Box::new(SystemInfoSkill));
}
