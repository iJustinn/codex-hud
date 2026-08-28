//! Claude-HUD-inspired status surface for the Codex TUI.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use serde::Deserialize;

use crate::live_wrap::take_prefix_by_width;
use crate::render::renderable::Renderable;
use crate::token_usage::TokenUsage;

const WIDE_LAYOUT_MIN_WIDTH: u16 = 80;
const CONTEXT_BAR_MIN_WIDTH: u16 = 90;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct HudConfig {
    pub(crate) enabled: bool,
    pub(crate) context_bar_width: u16,
    pub(crate) show_cost: bool,
    pub(crate) show_rate_limits: bool,
}

impl Default for HudConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            context_bar_width: 20,
            show_cost: true,
            show_rate_limits: true,
        }
    }
}

impl HudConfig {
    pub(crate) fn parse_or_default(contents: &str) -> Self {
        toml::from_str(contents).unwrap_or_default()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HudPaths {
    pub(crate) config: PathBuf,
    pub(crate) pricing: PathBuf,
}

impl HudPaths {
    pub(crate) fn resolve(
        config_override: Option<PathBuf>,
        pricing_override: Option<PathBuf>,
        executable_dir: &Path,
    ) -> Self {
        Self {
            config: config_override.unwrap_or_else(|| executable_dir.join("codex-hud.toml")),
            pricing: pricing_override.unwrap_or_else(|| executable_dir.join("pricing.toml")),
        }
    }
}

pub(crate) struct HudRuntime {
    config: HudConfig,
    pricing: Option<PricingTable>,
    cost_tracker: CostTracker,
    estimated_cost_usd: Option<f64>,
}

impl HudRuntime {
    pub(crate) fn load() -> Self {
        let executable_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let paths = HudPaths::resolve(
            std::env::var_os("CODEX_HUD_CONFIG").map(PathBuf::from),
            std::env::var_os("CODEX_HUD_PRICING").map(PathBuf::from),
            &executable_dir,
        );
        let config = fs::read_to_string(&paths.config)
            .map(|contents| HudConfig::parse_or_default(&contents))
            .unwrap_or_default();
        #[cfg(test)]
        let config = HudConfig {
            enabled: false,
            ..config
        };
        let pricing = match fs::read_to_string(&paths.pricing) {
            Ok(contents) => match PricingTable::parse(&contents) {
                Ok(pricing) => Some(pricing),
                Err(error) => {
                    tracing::warn!(
                        path = %paths.pricing.display(),
                        %error,
                        "invalid HUD pricing; cost estimate disabled"
                    );
                    None
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Some(PricingTable::built_in())
            }
            Err(error) => {
                tracing::warn!(
                    path = %paths.pricing.display(),
                    %error,
                    "HUD pricing could not be read; cost estimate disabled"
                );
                None
            }
        };
        Self {
            config,
            pricing,
            cost_tracker: CostTracker::default(),
            estimated_cost_usd: None,
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.config.enabled
    }

    pub(crate) fn widget(&self) -> HudWidget {
        HudWidget::new(self.config.clone())
    }

    pub(crate) fn observe_usage(&mut self, model: &str, usage: &TokenUsage) {
        self.estimated_cost_usd = self
            .pricing
            .as_ref()
            .and_then(|pricing| self.cost_tracker.observe(model, usage, pricing));
    }

    pub(crate) fn estimated_cost_usd(&self) -> Option<f64> {
        self.estimated_cost_usd
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelPricing {
    input_per_million: f64,
    cached_input_per_million: f64,
    output_per_million: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct PricingTable {
    models: BTreeMap<String, ModelPricing>,
}

impl PricingTable {
    pub(crate) fn parse(contents: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(contents)
    }

    pub(crate) fn built_in() -> Self {
        match Self::parse(
            r#"
            [models."gpt-5.6"]
            input_per_million = 4.0
            cached_input_per_million = 0.4
            output_per_million = 20.0

            [models."gpt-5.6-sol"]
            input_per_million = 4.0
            cached_input_per_million = 0.4
            output_per_million = 20.0

            [models."gpt-5.6-terra"]
            input_per_million = 2.0
            cached_input_per_million = 0.2
            output_per_million = 12.0

            [models."gpt-5.6-luna"]
            input_per_million = 0.2
            cached_input_per_million = 0.02
            output_per_million = 1.2

            [models."gpt-5.5"]
            input_per_million = 5.0
            cached_input_per_million = 0.5
            output_per_million = 30.0

            [models."gpt-5.4"]
            input_per_million = 2.5
            cached_input_per_million = 0.25
            output_per_million = 15.0

            [models."gpt-5-codex"]
            input_per_million = 1.25
            cached_input_per_million = 0.125
            output_per_million = 10.0
            "#,
        ) {
            Ok(pricing) => pricing,
            Err(error) => {
                tracing::error!(%error, "built-in HUD pricing is invalid");
                Self::default()
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CostTracker {
    previous: Option<TokenUsage>,
    total_usd: f64,
    available: bool,
}

impl CostTracker {
    pub(crate) fn observe(
        &mut self,
        model: &str,
        usage: &TokenUsage,
        pricing: &PricingTable,
    ) -> Option<f64> {
        let price = pricing.models.get(model);
        let previous = self.previous.replace(usage.clone());
        let Some(price) = price else {
            self.available = false;
            return None;
        };
        self.available = true;
        let Some(previous) = previous else {
            return Some(self.total_usd);
        };
        if usage.input_tokens < previous.input_tokens
            || usage.cached_input_tokens < previous.cached_input_tokens
            || usage.output_tokens < previous.output_tokens
        {
            return self.available.then_some(self.total_usd);
        }

        let input_delta = usage.input_tokens - previous.input_tokens;
        let cached_delta = usage.cached_input_tokens - previous.cached_input_tokens;
        let output_delta = usage.output_tokens - previous.output_tokens;
        let uncached_delta = (input_delta - cached_delta).max(0);
        self.total_usd += tokens_cost(uncached_delta, price.input_per_million)
            + tokens_cost(cached_delta, price.cached_input_per_million)
            + tokens_cost(output_delta, price.output_per_million);
        self.total_usd = (self.total_usd * 1_000_000_000.0).round() / 1_000_000_000.0;
        self.available.then_some(self.total_usd)
    }
}

fn tokens_cost(tokens: i64, dollars_per_million: f64) -> f64 {
    tokens.max(0) as f64 * dollars_per_million / 1_000_000.0
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HudRateLimit {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) used_percent: i64,
    pub(crate) resets_at: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HudData {
    pub(crate) model: String,
    pub(crate) reasoning: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) git_branch: Option<String>,
    pub(crate) git_dirty: Option<bool>,
    pub(crate) context_used_percent: Option<i64>,
    pub(crate) total_tokens: Option<i64>,
    pub(crate) input_tokens: Option<i64>,
    pub(crate) output_tokens: Option<i64>,
    pub(crate) cached_input_tokens: Option<i64>,
    pub(crate) estimated_cost_usd: Option<f64>,
    pub(crate) cost_is_this_run: bool,
    pub(crate) rate_limits: Vec<HudRateLimit>,
}

pub(crate) struct HudWidget {
    config: HudConfig,
    data: HudData,
}

impl HudWidget {
    pub(crate) fn new(config: HudConfig) -> Self {
        Self {
            config,
            data: HudData::default(),
        }
    }

    pub(crate) fn set_data(&mut self, data: HudData) -> bool {
        if self.data == data {
            return false;
        }
        self.data = data;
        true
    }

    pub(crate) fn enabled(&self) -> bool {
        self.config.enabled
    }

    fn render_lines(&self, width: u16) -> Vec<Line<'static>> {
        if !self.config.enabled || width < 24 {
            return Vec::new();
        }

        vec![
            self.header_line(width),
            self.context_and_limits_line(width),
            self.tokens_line(width),
        ]
    }

    fn header_line(&self, width: u16) -> Line<'static> {
        let mut spans = vec!["[".into(), self.data.model.clone().cyan()];
        if let Some(reasoning) = &self.data.reasoning {
            spans.extend([" ".into(), reasoning.clone().magenta()]);
        }
        spans.push("]".into());
        if let Some(project) = &self.data.project {
            spans.extend([" | ".dim(), project.clone().green()]);
        }
        if let Some(branch) = &self.data.git_branch {
            let dirty = if self.data.git_dirty == Some(true) {
                "*"
            } else {
                ""
            };
            spans.extend([
                " git:(".dim(),
                format!("{branch}{dirty}").magenta(),
                ")".dim(),
            ]);
        }
        if width >= WIDE_LAYOUT_MIN_WIDTH
            && self.config.show_cost
            && let Some(cost) = self.data.estimated_cost_usd
        {
            let label = if self.data.cost_is_this_run {
                "API est. this run"
            } else {
                "API est."
            };
            spans.extend([" | ".dim(), format!("{label} ${cost:.2}").dim()]);
        }
        truncate_line(Line::from(spans), width)
    }

    fn context_and_limits_line(&self, width: u16) -> Line<'static> {
        let mut spans = Vec::new();
        if let Some(percent) = self.data.context_used_percent {
            spans.push("Context ".dim());
            if width >= CONTEXT_BAR_MIN_WIDTH {
                spans.push(self.context_bar(percent));
                spans.push(" ".into());
            }
            spans.push(percent_span(format!("{percent}%"), percent));
        } else {
            spans.push("Context —".dim());
        }

        if self.config.show_rate_limits && !self.data.rate_limits.is_empty() {
            let remaining = width.saturating_sub(line_width(&Line::from(spans.clone())) + 3);
            let selected = self.selected_limits(
                remaining,
                width >= WIDE_LAYOUT_MIN_WIDTH,
                width >= CONTEXT_BAR_MIN_WIDTH,
            );
            for limit in selected.limits {
                spans.push(" | ".dim());
                spans.push(percent_span(
                    format!("{} {}%", limit.label, limit.used_percent),
                    limit.used_percent,
                ));
                if selected.show_resets
                    && let Some(reset) = &limit.resets_at
                {
                    spans.push(format!(" ({reset})").dim());
                }
            }
            if selected.hidden > 0 {
                spans.extend([" | ".dim(), format!("+{}", selected.hidden).dim()]);
            }
        }
        truncate_line(Line::from(spans), width)
    }

    fn context_bar(&self, percent: i64) -> Span<'static> {
        let width = self.config.context_bar_width.clamp(4, 40) as usize;
        let used = ((percent.clamp(0, 100) as f64 / 100.0) * width as f64).round() as usize;
        format!("[{}{}]", "█".repeat(used), "░".repeat(width - used)).green()
    }

    fn selected_limits(
        &self,
        available_width: u16,
        allow_all_limits: bool,
        prefer_resets: bool,
    ) -> SelectedLimits<'_> {
        let separator_width = self.data.rate_limits.len().saturating_sub(1) * 3;
        let full_width = self
            .data
            .rate_limits
            .iter()
            .map(|limit| rate_limit_width(limit, true))
            .sum::<usize>()
            + separator_width;
        if prefer_resets && full_width <= available_width as usize {
            return SelectedLimits {
                limits: self.data.rate_limits.iter().collect(),
                hidden: 0,
                show_resets: true,
            };
        }

        let compact_width = self
            .data
            .rate_limits
            .iter()
            .map(|limit| rate_limit_width(limit, false))
            .sum::<usize>()
            + separator_width;
        if allow_all_limits && compact_width <= available_width as usize {
            return SelectedLimits {
                limits: self.data.rate_limits.iter().collect(),
                hidden: 0,
                show_resets: false,
            };
        }

        let highest = self
            .data
            .rate_limits
            .iter()
            .max_by_key(|limit| (limit.used_percent, std::cmp::Reverse(&limit.id)));
        SelectedLimits {
            limits: highest.into_iter().collect(),
            hidden: self.data.rate_limits.len().saturating_sub(1),
            show_resets: false,
        }
    }

    fn tokens_line(&self, width: u16) -> Line<'static> {
        let total = self
            .data
            .total_tokens
            .map(format_tokens)
            .unwrap_or_else(|| "—".to_string());
        let input = self.data.input_tokens.map(format_tokens);
        let output = self.data.output_tokens.map(format_tokens);
        let cached = self.data.cached_input_tokens.map(format_tokens);
        let details = if width >= WIDE_LAYOUT_MIN_WIDTH {
            format!(
                "in: {}, out: {}, cache: {}",
                input.as_deref().unwrap_or("—"),
                output.as_deref().unwrap_or("—"),
                cached.as_deref().unwrap_or("—")
            )
        } else {
            format!(
                "in {} · out {} · cache {}",
                input.as_deref().unwrap_or("—"),
                output.as_deref().unwrap_or("—"),
                cached.as_deref().unwrap_or("—")
            )
        };
        truncate_line(
            Line::from(vec![
                "Tokens ".dim(),
                total.cyan(),
                format!(" ({details})").dim(),
            ]),
            width,
        )
    }
}

struct SelectedLimits<'a> {
    limits: Vec<&'a HudRateLimit>,
    hidden: usize,
    show_resets: bool,
}

impl Renderable for HudWidget {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        Paragraph::new(self.render_lines(area.width)).render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.render_lines(width).len() as u16
    }
}

fn percent_span(text: String, percent: i64) -> Span<'static> {
    match percent {
        90.. => text.red(),
        75..=89 => text.magenta(),
        _ => text.green(),
    }
}

fn rate_limit_width(limit: &HudRateLimit, show_reset: bool) -> usize {
    limit.label.len()
        + 2
        + limit.used_percent.to_string().len()
        + usize::from(show_reset) * limit.resets_at.as_ref().map_or(0, |reset| reset.len() + 3)
}

fn format_tokens(tokens: i64) -> String {
    let tokens = tokens.max(0);
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{}k", (tokens as f64 / 1_000.0).round() as i64)
    } else {
        tokens.to_string()
    }
}

fn truncate_line(line: Line<'static>, width: u16) -> Line<'static> {
    let mut remaining = width as usize;
    let mut spans = Vec::new();
    for span in line.spans {
        if remaining == 0 {
            break;
        }
        let (prefix, _, used) = take_prefix_by_width(span.content.as_ref(), remaining);
        if !prefix.is_empty() {
            spans.push(Span::styled(prefix, span.style));
            remaining = remaining.saturating_sub(used);
        }
    }
    Line::from(spans)
}

fn line_width(line: &Line<'_>) -> u16 {
    line.width().min(u16::MAX as usize) as u16
}

#[allow(dead_code)]
pub(crate) const fn is_codex_hud_build() -> bool {
    true
}

#[cfg(test)]
#[path = "hud_tests.rs"]
mod tests;
