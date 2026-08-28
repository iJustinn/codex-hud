//! Runtime data adapter between `ChatWidget` state and the bottom-pane HUD.

use super::*;
use crate::hud::HudData;
use crate::hud::HudRateLimit;
use crate::status::RateLimitSnapshotDisplay;
use crate::status::RateLimitWindowDisplay;

impl ChatWidget {
    pub(super) fn refresh_hud(&mut self) {
        if !self.hud_runtime.enabled() {
            return;
        }

        let project = self
            .status_line_value_for_item(StatusLineItem::ProjectRoot)
            .or_else(|| self.status_line_value_for_item(StatusLineItem::CurrentDir));
        let reasoning = Some(Self::status_line_reasoning_effort_label(
            self.effective_reasoning_effort().as_ref(),
        ));
        let usage = (!self.token_usage_pending)
            .then(|| self.token_info.as_ref().map(|info| &info.total_token_usage))
            .flatten();
        let data = HudData {
            model: self.model_display_name().to_string(),
            reasoning,
            project,
            git_branch: self.status_line_branch.clone(),
            git_dirty: self
                .status_line_git_summary
                .as_ref()
                .and_then(|summary| summary.worktree_dirty),
            context_used_percent: self.status_line_context_used_percent(),
            total_tokens: usage.map(|usage| usage.total_tokens),
            input_tokens: usage.map(|usage| usage.input_tokens),
            output_tokens: usage.map(|usage| usage.output_tokens),
            cached_input_tokens: usage.map(|usage| usage.cached_input_tokens),
            estimated_cost_usd: self.hud_runtime.estimated_cost_usd(),
            cost_is_this_run: true,
            rate_limits: self.hud_rate_limits(),
        };
        self.bottom_pane.set_hud_data(data);
    }

    fn hud_rate_limits(&self) -> Vec<HudRateLimit> {
        let mut limits = Vec::new();
        if let Some(snapshot) = self.rate_limit_snapshots_by_limit_id.get("codex") {
            append_snapshot_limits(&mut limits, "codex", snapshot);
        }
        for (id, snapshot) in &self.rate_limit_snapshots_by_limit_id {
            if id != "codex" {
                append_snapshot_limits(&mut limits, id, snapshot);
            }
        }
        limits
    }
}

fn append_snapshot_limits(
    target: &mut Vec<HudRateLimit>,
    id: &str,
    snapshot: &RateLimitSnapshotDisplay,
) {
    let window_count =
        usize::from(snapshot.primary.is_some()) + usize::from(snapshot.secondary.is_some());
    if let Some(window) = &snapshot.primary {
        target.push(hud_rate_limit(
            id,
            snapshot,
            window,
            /*is_secondary*/ false,
            window_count,
        ));
    }
    if let Some(window) = &snapshot.secondary {
        target.push(hud_rate_limit(
            id,
            snapshot,
            window,
            /*is_secondary*/ true,
            window_count,
        ));
    }
}

fn hud_rate_limit(
    id: &str,
    snapshot: &RateLimitSnapshotDisplay,
    window: &RateLimitWindowDisplay,
    is_secondary: bool,
    window_count: usize,
) -> HudRateLimit {
    let window_label = limit_label_for_window(window.window_minutes, is_secondary);
    let window_label = capitalize_limit_label(&window_label);
    let label = if id == "codex" {
        window_label
    } else if window_count == 1 {
        snapshot.limit_name.clone()
    } else {
        format!("{} {window_label}", snapshot.limit_name)
    };
    HudRateLimit {
        id: id.to_string(),
        label,
        used_percent: window.used_percent.round().clamp(0.0, 100.0) as i64,
        resets_at: window.resets_at.clone(),
    }
}

fn capitalize_limit_label(label: &str) -> String {
    let mut chars = label.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().collect::<String>() + chars.as_str()
}
