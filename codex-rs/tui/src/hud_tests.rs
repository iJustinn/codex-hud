use std::path::Path;

use insta::assert_snapshot;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::CostTracker;
use super::HudConfig;
use super::HudData;
use super::HudPaths;
use super::HudRateLimit;
use super::HudWidget;
use super::PricingTable;
use crate::render::renderable::Renderable;
use crate::token_usage::TokenUsage;

fn render(widget: &HudWidget, width: u16) -> String {
    let height = widget.desired_height(width);
    let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
    widget.render(Rect::new(0, 0, width, height), &mut buf);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sample_data() -> HudData {
    HudData {
        model: "gpt-5.6-codex".to_string(),
        reasoning: Some("medium".to_string()),
        project: Some("codex-hud".to_string()),
        git_branch: Some("main".to_string()),
        git_dirty: Some(true),
        context_used_percent: Some(42),
        total_tokens: Some(1_923_000),
        input_tokens: Some(50_000),
        output_tokens: Some(22_000),
        cached_input_tokens: Some(1_851_000),
        estimated_cost_usd: Some(3.27),
        cost_is_this_run: true,
        rate_limits: vec![
            HudRateLimit {
                id: "codex".to_string(),
                label: "5h".to_string(),
                used_percent: 93,
                resets_at: Some("58m".to_string()),
            },
            HudRateLimit {
                id: "codex".to_string(),
                label: "Weekly".to_string(),
                used_percent: 80,
                resets_at: Some("16h 58m".to_string()),
            },
            HudRateLimit {
                id: "image_gen".to_string(),
                label: "Images".to_string(),
                used_percent: 94,
                resets_at: None,
            },
        ],
    }
}

#[test]
fn test_hud_paths_env_overrides_adjacent_files() {
    let paths = HudPaths::resolve(
        Some("/tmp/custom-hud.toml".into()),
        Some("/tmp/custom-pricing.toml".into()),
        Path::new("/Applications/codex-hud/bin"),
    );

    assert_eq!(
        paths,
        HudPaths {
            config: "/tmp/custom-hud.toml".into(),
            pricing: "/tmp/custom-pricing.toml".into(),
        }
    );
}

#[test]
fn test_hud_config_invalid_toml_uses_defaults() {
    assert_eq!(
        HudConfig::parse_or_default("enabled = ["),
        HudConfig::default()
    );
}

#[test]
fn test_cost_tracker_charges_deltas_and_rebases_regressions() {
    let pricing = PricingTable::parse(
        r#"
        [models."gpt-5.6-codex"]
        input_per_million = 2.0
        cached_input_per_million = 0.2
        output_per_million = 10.0
        "#,
    )
    .expect("pricing should parse");
    let mut tracker = CostTracker::default();
    let baseline = TokenUsage {
        input_tokens: 100_000,
        cached_input_tokens: 80_000,
        output_tokens: 10_000,
        reasoning_output_tokens: 4_000,
        total_tokens: 110_000,
    };
    let next = TokenUsage {
        input_tokens: 160_000,
        cached_input_tokens: 120_000,
        output_tokens: 20_000,
        reasoning_output_tokens: 8_000,
        total_tokens: 180_000,
    };

    assert_eq!(
        tracker.observe("gpt-5.6-codex", &baseline, &pricing),
        Some(0.0)
    );
    assert_eq!(
        tracker.observe("gpt-5.6-codex", &next, &pricing),
        Some(0.148)
    );
    assert_eq!(
        tracker.observe("gpt-5.6-codex", &baseline, &pricing),
        Some(0.148)
    );
}

#[test]
fn test_cost_tracker_unknown_model_is_unavailable() {
    let pricing = PricingTable::parse("").expect("empty pricing should parse");
    let mut tracker = CostTracker::default();

    assert_eq!(
        tracker.observe("unknown", &TokenUsage::default(), &pricing),
        None
    );
}

#[test]
fn test_cost_tracker_recovers_after_unknown_model() {
    let pricing = PricingTable::parse(
        r#"
        [models."known"]
        input_per_million = 1.0
        cached_input_per_million = 0.1
        output_per_million = 2.0
        "#,
    )
    .expect("pricing should parse");
    let mut tracker = CostTracker::default();
    let baseline = TokenUsage {
        input_tokens: 1_000,
        ..TokenUsage::default()
    };
    let next = TokenUsage {
        input_tokens: 2_000,
        output_tokens: 1_000,
        total_tokens: 3_000,
        ..TokenUsage::default()
    };

    assert_eq!(tracker.observe("unknown", &baseline, &pricing), None);
    assert_eq!(tracker.observe("known", &next, &pricing), Some(0.003));
}

#[test]
fn test_hud_widget_wide_snapshot() {
    let mut widget = HudWidget::new(HudConfig::default());
    widget.set_data(sample_data());

    assert_snapshot!(render(&widget, /*width*/ 150), @r"
    [gpt-5.6-codex medium] | codex-hud git:(main*) | API est. this run $3.27
    Context [████████░░░░░░░░░░░░] 42% | 5h 93% (58m) | Weekly 80% (16h 58m) | Images 94%
    Tokens 1.9M (in: 50k, out: 22k, cache: 1.9M)
    ");
}

#[test]
fn test_hud_widget_narrow_snapshot_compacts_limits() {
    let mut widget = HudWidget::new(HudConfig::default());
    widget.set_data(sample_data());

    assert_snapshot!(render(&widget, /*width*/ 54), @r"
    [gpt-5.6-codex medium] | codex-hud git:(main*)
    Context 42% | Images 94% | +2
    Tokens 1.9M (in 50k · out 22k · cache 1.9M)
    ");
}

#[test]
fn test_hud_widget_medium_snapshot_drops_resets_before_limits() {
    let mut widget = HudWidget::new(HudConfig::default());
    widget.set_data(sample_data());

    assert_snapshot!(render(&widget, /*width*/ 86), @r"
    [gpt-5.6-codex medium] | codex-hud git:(main*) | API est. this run $3.27
    Context 42% | 5h 93% | Weekly 80% | Images 94%
    Tokens 1.9M (in: 50k, out: 22k, cache: 1.9M)
    ");
}
