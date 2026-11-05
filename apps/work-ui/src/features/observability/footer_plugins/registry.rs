//! Typed, client-only, read-only providers. No node/plugin loader or executable input.
use super::super::runtime_stats::{percent, shown, tokens};
use super::FooterContext;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Tone {
    Neutral,
    Good,
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum FooterVisual {
    Text,
    Tokens {
        input: String,
        output: String,
    },
    Cache {
        value: String,
        known: bool,
    },
    Context {
        value: String,
        used: Option<f64>,
    },
    Cost {
        value: String,
    },
    Tools {
        calls: Option<usize>,
        failures: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct FooterItem {
    pub id: String,
    pub label: String,
    pub value: String,
    pub visual: FooterVisual,
    pub description: String,
    pub tone: Tone,
}

pub(super) struct ItemDeclaration {
    pub id: &'static str,
    pub label: &'static str,
}
pub(super) struct FooterPlugin {
    pub id: &'static str,
    pub version: u32,
    pub title: &'static str,
    pub items: &'static [ItemDeclaration],
    pub provider: fn(&FooterContext) -> Result<Vec<FooterItem>, &'static str>,
}

pub(super) static BUILTINS: &[FooterPlugin] = &[
    FooterPlugin {
        id: "session-identity",
        version: 1,
        title: "会话身份",
        items: &[ItemDeclaration {
            id: "identity",
            label: "供应商/模型",
        }],
        provider: identity,
    },
    FooterPlugin {
        id: "session-usage",
        version: 1,
        title: "会话用量",
        items: &[
            ItemDeclaration {
                id: "input-output",
                label: "累计输入/输出",
            },
            ItemDeclaration {
                id: "cache",
                label: "缓存命中率",
            },
            ItemDeclaration {
                id: "cost",
                label: "累计费用",
            },
            ItemDeclaration {
                id: "speed",
                label: "生成速度",
            },
            ItemDeclaration {
                id: "requests-retries",
                label: "请求/重试",
            },
        ],
        provider: usage,
    },
    FooterPlugin {
        id: "context-usage",
        version: 1,
        title: "上下文用量",
        items: &[ItemDeclaration {
            id: "context",
            label: "上下文",
        }],
        provider: context_usage,
    },
    FooterPlugin {
        id: "tool-activity",
        version: 1,
        title: "已加载工具活动",
        items: &[ItemDeclaration {
            id: "tools",
            label: "已加载工具/失败",
        }],
        provider: tools,
    },
];

pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn valid_text(text: &str, max: usize) -> bool {
    text.len() <= max && !text.chars().any(char::is_control)
}

/// Fail closed per descriptor/item, not per registry. First valid duplicate wins.
/// IDs must be declared; providers cannot choose markup, classes, actions or URLs.
pub(super) fn collect<'a>(
    plugins: &'a [FooterPlugin],
    context: &FooterContext,
) -> Vec<(&'a FooterPlugin, Vec<FooterItem>)> {
    let mut plugin_ids = BTreeSet::new();
    let mut result = Vec::new();
    let mut total = 0;
    for plugin in plugins {
        if !valid_id(plugin.id)
            || plugin.version != 1
            || !valid_text(plugin.title, 128)
            || plugin.items.len() > 16
            || plugin
                .items
                .iter()
                .any(|i| !valid_id(i.id) || !valid_text(i.label, 128))
            || plugin
                .items
                .iter()
                .map(|i| i.id)
                .collect::<BTreeSet<_>>()
                .len()
                != plugin.items.len()
            || !plugin_ids.insert(plugin.id)
        {
            continue;
        }
        let Ok(Ok(items)) =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (plugin.provider)(context)))
        else {
            continue;
        };
        if items.len() > 16 {
            continue;
        }
        let mut ids = BTreeSet::new();
        let items: Vec<_> = items
            .into_iter()
            .filter(|item| {
                valid_id(&item.id)
                    && plugin.items.iter().any(|i| i.id == item.id)
                    && valid_text(&item.label, 128)
                    && valid_text(&item.value, 512)
                    && valid_text(&item.description, 1024)
                    && ids.insert(item.id.clone())
            })
            .take(64_usize.saturating_sub(total))
            .collect();
        total += items.len();
        if !items.is_empty() {
            result.push((plugin, items));
        }
    }
    result
}

pub(super) fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|n| n.is_finite() && *n >= 0.0)
}
fn count(value: Option<f64>) -> Option<f64> {
    finite(value).filter(|n| *n <= 1e15)
}
fn item(id: &str, label: &str, value: String, description: &str, tone: Tone) -> FooterItem {
    FooterItem {
        id: id.into(),
        label: label.into(),
        value,
        visual: FooterVisual::Text,
        description: description.into(),
        tone,
    }
}
fn text(value: Option<&str>) -> &str {
    value
        .filter(|s| !s.trim().is_empty() && !matches!(s.trim(), "unknown" | "undefined" | "null"))
        .unwrap_or("—")
}
pub(super) fn confirmed(context: &FooterContext) -> bool {
    context.online && text(Some(&context.scope)) != "—"
}
#[allow(clippy::unnecessary_wraps)] // Matches FooterPlugin's fallible provider function pointer.
fn identity(c: &FooterContext) -> Result<Vec<FooterItem>, &'static str> {
    let value = if confirmed(c) {
        let mut value = format!(
            "{} / {}",
            text(c.provider.as_deref()),
            text(c.model.as_deref())
        );
        if let Some(thinking) = c.thinking.as_deref().filter(|s| text(Some(s)) != "—") {
            value.push_str(" · ");
            value.push_str(thinking);
        }
        value
    } else {
        "— / —".into()
    };
    Ok(vec![item(
        "identity",
        "供应商/模型",
        value,
        "当前会话的供应商、模型与可选思考级别；未确认时不展示旧会话身份",
        Tone::Neutral,
    )])
}
#[allow(clippy::unnecessary_wraps)] // Matches FooterPlugin's fallible provider function pointer.
fn usage(c: &FooterContext) -> Result<Vec<FooterItem>, &'static str> {
    let empty = super::super::runtime_stats::Metrics::default();
    let m = if confirmed(c) { &c.metrics } else { &empty };
    let input = tokens(count(m.input));
    let output = tokens(count(m.output));
    let cache = finite(m.cache_percent).filter(|n| *n <= 100.0);
    let cost = finite(m.cost)
        .filter(|n| *n <= 1e12)
        .map_or_else(|| "—".into(), |n| format!("${n:.4}"));
    let mut items = vec![
        item(
            "input-output",
            "累计输入/输出",
            format!("会话累计 ↑{input} ↓{output}"),
            "累计输入/输出；仅展示节点上报的当前会话用量",
            Tone::Neutral,
        ),
        item(
            "cache",
            "缓存命中率",
            format!("缓存 {}", percent(cache)),
            "缓存读取占累计输入的比例",
            Tone::Good,
        ),
        item(
            "cost",
            "累计费用",
            format!("累计{cost}"),
            "会话累计 USD；节点上报，非账单",
            Tone::Neutral,
        ),
    ];
    items[0].visual = FooterVisual::Tokens { input, output };
    items[1].visual = FooterVisual::Cache {
        value: percent(cache),
        known: cache.is_some(),
    };
    items[2].visual = FooterVisual::Cost { value: cost };
    if let Some(speed) = count(m.speed) {
        items.push(item(
            "speed",
            "生成速度",
            format!("{} tok/s", shown(Some(speed))),
            "节点明确上报的生成速度",
            Tone::Neutral,
        ));
    }
    if count(m.requests).is_some() || count(m.retries).is_some() {
        items.push(item(
            "requests-retries",
            "请求/重试",
            format!(
                "请求 {} ·重试 {}",
                shown(count(m.requests)),
                shown(count(m.retries))
            ),
            "节点明确上报的累计请求与重试；未上报为 —",
            Tone::Neutral,
        ));
    }
    Ok(items)
}
#[allow(clippy::unnecessary_wraps)] // Matches FooterPlugin's fallible provider function pointer.
fn context_usage(c: &FooterContext) -> Result<Vec<FooterItem>, &'static str> {
    let pair = confirmed(c)
        .then(|| count(c.context.tokens).zip(count(c.context.window)))
        .flatten()
        .filter(|(_, w)| *w > 0.0);
    let used = pair.and_then(|(n, w)| finite(Some(n / w * 100.0)).filter(|n| *n <= 100.0));
    let value = pair.map_or_else(
        || "上下文 —".into(),
        |(n, w)| {
            format!(
                "上下文 {}/{} ·{}",
                tokens(Some(n)),
                tokens(Some(w)),
                percent(used)
            )
        },
    );
    let mut result = item(
        "context",
        "上下文",
        value,
        "当前实际 token / 窗口；不会从百分比反推 token",
        Tone::Neutral,
    );
    result.visual = FooterVisual::Context {
        value: percent(used),
        used,
    };
    Ok(vec![result])
}
#[allow(clippy::unnecessary_wraps)] // Matches FooterPlugin's fallible provider function pointer.
fn tools(c: &FooterContext) -> Result<Vec<FooterItem>, &'static str> {
    let known = confirmed(c);
    let value = if known {
        format!("已加载工具 {} ·失败{}", c.tool_calls, c.tool_failures)
    } else {
        "已加载工具 — ·失败—".into()
    };
    let mut result = item(
        "tools",
        "已加载工具/失败",
        value,
        "仅已加载的分页历史与当前投影中的工具调用，不代表完整历史",
        if known && c.tool_failures > 0 {
            Tone::Warning
        } else {
            Tone::Neutral
        },
    );
    result.visual = FooterVisual::Tools {
        calls: known.then_some(c.tool_calls),
        failures: if known { c.tool_failures } else { 0 },
    };
    Ok(vec![result])
}

pub(super) fn metric_class(id: &str) -> &'static str {
    match id {
        "identity" => "identity-metric",
        "cache" => "cache-metric",
        "context" => "context-meter",
        "cost" => "cost-metric",
        "speed" => "speed-metric",
        "tools" => "tool-metric",
        _ => "usage-metric",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn online() -> FooterContext {
        FooterContext {
            scope: "session-a".into(),
            online: true,
            ..Default::default()
        }
    }
    #[test]
    fn known_visuals_keep_full_semantic_values() {
        let mut c = online();
        c.metrics.input = Some(32_000.0);
        c.metrics.output = Some(4_800.0);
        c.metrics.cache_percent = Some(75.0);
        c.metrics.cost = Some(0.0184);
        c.metrics.speed = Some(12.0);
        c.metrics.requests = Some(2.0);
        c.context.tokens = Some(38_000.0);
        c.context.window = Some(100_000.0);
        c.context.percent = Some(99.0); // Actual pair, not reported percent, wins.
        c.tool_calls = 7;
        c.tool_failures = 2;
        let items = usage(&c).unwrap();
        assert_eq!(items[0].value, "会话累计 ↑32k ↓4.8k");
        assert_eq!(
            items[0].visual,
            FooterVisual::Tokens {
                input: "32k".into(),
                output: "4.8k".into(),
            }
        );
        assert_eq!(items[1].value, "缓存 75%");
        assert_eq!(
            items[1].visual,
            FooterVisual::Cache {
                value: "75%".into(),
                known: true,
            }
        );
        assert_eq!(items[2].value, "累计$0.0184");
        assert_eq!(
            items[2].visual,
            FooterVisual::Cost {
                value: "$0.0184".into()
            }
        );
        assert_eq!(items[3].visual, FooterVisual::Text);
        assert_eq!(items[4].visual, FooterVisual::Text);
        assert_eq!(identity(&c).unwrap()[0].visual, FooterVisual::Text);
        let context = context_usage(&c).unwrap().remove(0);
        assert_eq!(context.value, "上下文 38k/100k ·38%");
        assert_eq!(
            context.visual,
            FooterVisual::Context {
                value: "38%".into(),
                used: Some(38.0),
            }
        );
        let tool = tools(&c).unwrap().remove(0);
        assert_eq!(tool.value, "已加载工具 7 ·失败2");
        assert_eq!(
            tool.visual,
            FooterVisual::Tools {
                calls: Some(7),
                failures: 2
            }
        );
        assert_eq!(tool.tone, Tone::Warning);
    }
    #[test]
    fn unknown_and_unconfirmed_visuals_never_reuse_stale_metrics() {
        let mut stale = online();
        stale.metrics.input = Some(32_000.0);
        stale.metrics.output = Some(4_800.0);
        stale.metrics.cache_percent = Some(75.0);
        stale.metrics.cost = Some(0.0184);
        stale.context.tokens = Some(38_000.0);
        stale.context.window = Some(100_000.0);
        stale.tool_calls = 7;
        stale.tool_failures = 2;
        let mut offline = stale.clone();
        offline.online = false;
        stale.scope = "unknown".into();
        for c in [online(), offline, stale] {
            let items = usage(&c).unwrap();
            assert_eq!(items.len(), 3);
            assert_eq!(
                items[0].visual,
                FooterVisual::Tokens {
                    input: "—".into(),
                    output: "—".into(),
                }
            );
            assert_eq!(
                items[1].visual,
                FooterVisual::Cache {
                    value: "—".into(),
                    known: false,
                }
            );
            assert_eq!(
                items[2].visual,
                FooterVisual::Cost {
                    value: "—".into()
                }
            );
            assert_eq!(
                context_usage(&c).unwrap()[0].visual,
                FooterVisual::Context {
                    value: "—".into(),
                    used: None,
                }
            );
            let tool = tools(&c).unwrap().remove(0);
            assert_eq!(
                tool.visual,
                FooterVisual::Tools {
                    calls: confirmed(&c).then_some(0),
                    failures: 0,
                }
            );
            assert_eq!(tool.tone, Tone::Neutral);
        }
    }
    #[test]
    fn confirmed_zero_visuals_are_known_not_placeholders() {
        let mut c = online();
        c.metrics.input = Some(0.0);
        c.metrics.output = Some(0.0);
        c.metrics.cache_percent = Some(0.0);
        c.metrics.cost = Some(0.0);
        c.context.tokens = Some(0.0);
        c.context.window = Some(100_000.0);
        let items = usage(&c).unwrap();
        assert_eq!(
            items[0].visual,
            FooterVisual::Tokens {
                input: "0".into(),
                output: "0".into(),
            }
        );
        assert_eq!(
            items[1].visual,
            FooterVisual::Cache {
                value: "0%".into(),
                known: true,
            }
        );
        assert_eq!(
            items[2].visual,
            FooterVisual::Cost {
                value: "$0.0000".into()
            }
        );
        assert_eq!(
            context_usage(&c).unwrap()[0].visual,
            FooterVisual::Context {
                value: "0%".into(),
                used: Some(0.0),
            }
        );
        assert_eq!(
            tools(&c).unwrap()[0].visual,
            FooterVisual::Tools {
                calls: Some(0),
                failures: 0,
            }
        );
    }
    #[test]
    fn invalid_usage_visuals_follow_existing_numeric_bounds() {
        let mut c = online();
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 1e15 + 1.0] {
            c.metrics.input = Some(invalid);
            c.metrics.output = Some(invalid);
            c.metrics.cache_percent = Some(invalid);
            c.metrics.cost = Some(invalid);
            let items = usage(&c).unwrap();
            assert_eq!(
                items[0].visual,
                FooterVisual::Tokens {
                    input: "—".into(),
                    output: "—".into(),
                }
            );
            assert_eq!(
                items[1].visual,
                FooterVisual::Cache {
                    value: "—".into(),
                    known: false,
                }
            );
            assert_eq!(
                items[2].visual,
                FooterVisual::Cost {
                    value: "—".into()
                }
            );
        }
        c.metrics.cache_percent = Some(100.001);
        c.metrics.cost = Some(1e12 + 1.0);
        let items = usage(&c).unwrap();
        assert_eq!(
            items[1].visual,
            FooterVisual::Cache {
                value: "—".into(),
                known: false
            }
        );
        assert_eq!(
            items[2].visual,
            FooterVisual::Cost {
                value: "—".into()
            }
        );
        c.metrics.cache_percent = Some(100.0);
        assert_eq!(
            usage(&c).unwrap()[1].visual,
            FooterVisual::Cache {
                value: "100%".into(),
                known: true,
            }
        );
    }
    #[test]
    fn context_visual_requires_a_valid_actual_pair_and_bounded_ratio() {
        let mut c = online();
        c.context.percent = Some(38.0);
        for (n, w) in [
            (None, None),
            (Some(38_000.0), None),
            (None, Some(100_000.0)),
            (Some(0.0), Some(0.0)),
            (Some(f64::NAN), Some(100_000.0)),
            (Some(38_000.0), Some(f64::INFINITY)),
            (Some(-1.0), Some(100_000.0)),
            (Some(38_000.0), Some(-1.0)),
            (Some(1e15 + 1.0), Some(1e15)),
            (Some(38_000.0), Some(1e15 + 1.0)),
            (Some(101_000.0), Some(100_000.0)),
            (Some(1.0), Some(f64::MIN_POSITIVE)),
        ] {
            c.context.tokens = n;
            c.context.window = w;
            assert_eq!(
                context_usage(&c).unwrap()[0].visual,
                FooterVisual::Context {
                    value: "—".into(),
                    used: None,
                }
            );
        }
        c.context.tokens = Some(101_000.0);
        c.context.window = Some(100_000.0);
        assert_eq!(context_usage(&c).unwrap()[0].value, "上下文 101k/100k ·—");
        c.context.tokens = Some(100_000.0);
        assert_eq!(
            context_usage(&c).unwrap()[0].visual,
            FooterVisual::Context {
                value: "100%".into(),
                used: Some(100.0),
            }
        );
    }
    #[test]
    fn real_zero_optional_and_finite() {
        let mut c = online();
        c.metrics.input = Some(0.0);
        c.metrics.output = Some(f64::NAN);
        c.metrics.cost = Some(f64::INFINITY);
        c.metrics.speed = Some(0.0);
        c.metrics.retries = Some(0.0);
        let items = usage(&c).unwrap();
        assert_eq!(items[0].value, "会话累计 ↑0 ↓—");
        assert_eq!(items[2].value, "累计—");
        assert!(items.iter().any(|i| i.value == "0 tok/s"));
        assert!(items.iter().any(|i| i.value == "请求 — ·重试 0"));
        assert!(
            !items
                .iter()
                .any(|i| i.value.contains("NaN") || i.value.contains("inf"))
        );
    }
    #[test]
    fn context_never_inverts_percent_and_offline_never_leaks() {
        let mut c = online();
        c.context.percent = Some(38.0);
        assert_eq!(context_usage(&c).unwrap()[0].value, "上下文 —");
        c.context.tokens = Some(38_000.0);
        c.context.window = Some(100_000.0);
        assert_eq!(context_usage(&c).unwrap()[0].value, "上下文 38k/100k ·38%");
        c.online = false;
        c.model = Some("OLD MODEL".into());
        c.metrics.input = Some(123.0);
        assert_eq!(identity(&c).unwrap()[0].value, "— / —");
        assert_eq!(usage(&c).unwrap()[0].value, "会话累计 ↑— ↓—");
        assert_eq!(tools(&c).unwrap()[0].value, "已加载工具 — ·失败—");
    }
    fn failure(_: &FooterContext) -> Result<Vec<FooterItem>, &'static str> {
        Err("isolated")
    }
    #[allow(clippy::unnecessary_wraps)] // Test provider must match the fallible function pointer.
    fn bad(_: &FooterContext) -> Result<Vec<FooterItem>, &'static str> {
        Ok(vec![
            item("identity", "x", "ok".into(), "", Tone::Neutral),
            item("identity", "x", "duplicate".into(), "", Tone::Neutral),
            item("undeclared", "x", "bad".into(), "", Tone::Neutral),
        ])
    }
    #[allow(clippy::unnecessary_wraps)] // Test provider must match the fallible function pointer.
    fn sixteen(_: &FooterContext) -> Result<Vec<FooterItem>, &'static str> {
        Ok((0..16)
            .map(|i| item(&format!("i{i}"), "x", "0".into(), "", Tone::Neutral))
            .collect())
    }
    fn seventeen(c: &FooterContext) -> Result<Vec<FooterItem>, &'static str> {
        let mut items = sixteen(c)?;
        items.push(items[0].clone());
        Ok(items)
    }
    fn panic_provider(_: &FooterContext) -> Result<Vec<FooterItem>, &'static str> {
        panic!("isolated provider panic")
    }
    #[allow(clippy::unnecessary_wraps)] // Test provider must match the fallible function pointer.
    fn oversized(_: &FooterContext) -> Result<Vec<FooterItem>, &'static str> {
        Ok(vec![item(
            "identity",
            "x",
            "x".repeat(513),
            "",
            Tone::Neutral,
        )])
    }
    #[test]
    fn hard_item_limits_and_invalid_provider_isolation() {
        let declarations: &'static [ItemDeclaration] = Box::leak(
            (0..16)
                .map(|i| ItemDeclaration {
                    id: Box::leak(format!("i{i}").into_boxed_str()),
                    label: "x",
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let plugins: Vec<_> = ["p0", "p1", "p2", "p3", "p4"]
            .into_iter()
            .map(|id| FooterPlugin {
                id,
                version: 1,
                title: "x",
                items: declarations,
                provider: sixteen,
            })
            .collect();
        let result = collect(&plugins, &online());
        assert_eq!(
            result.iter().map(|(_, items)| items.len()).sum::<usize>(),
            64
        );
        assert_eq!(result.len(), 4);
        let invalid = [
            FooterPlugin {
                id: "overflow",
                version: 1,
                title: "x",
                items: declarations,
                provider: seventeen,
            },
            FooterPlugin {
                id: "panic",
                version: 1,
                title: "x",
                items: BUILTINS[0].items,
                provider: panic_provider,
            },
            FooterPlugin {
                id: "oversized",
                version: 1,
                title: "x",
                items: BUILTINS[0].items,
                provider: oversized,
            },
            FooterPlugin {
                id: "survivor",
                version: 1,
                title: "x",
                items: BUILTINS[0].items,
                provider: identity,
            },
        ];
        let result = collect(&invalid, &online());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.id, "survivor");
    }
    #[test]
    fn unknown_scope_and_invalid_numbers_are_placeholders() {
        let mut c = online();
        c.scope = "unknown".into();
        c.model = Some("not current".into());
        c.tool_calls = 12;
        assert_eq!(identity(&c).unwrap()[0].value, "— / —");
        assert_eq!(tools(&c).unwrap()[0].value, "已加载工具 — ·失败—");
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            assert_eq!(finite(Some(value)), None);
        }
        assert_eq!(finite(Some(0.0)), Some(0.0));
    }
    #[test]
    fn isolation_duplicates_and_bounds() {
        let declarations = BUILTINS[0].items;
        let plugins = [
            FooterPlugin {
                id: "failure",
                version: 1,
                title: "x",
                items: declarations,
                provider: failure,
            },
            FooterPlugin {
                id: "valid",
                version: 1,
                title: "x",
                items: declarations,
                provider: bad,
            },
            FooterPlugin {
                id: "valid",
                version: 1,
                title: "x",
                items: declarations,
                provider: identity,
            },
            FooterPlugin {
                id: "other",
                version: 1,
                title: "x",
                items: declarations,
                provider: identity,
            },
        ];
        let result = collect(&plugins, &online());
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].1.len(), 1);
        assert!(
            collect(BUILTINS, &online())
                .iter()
                .map(|(_, i)| i.len())
                .sum::<usize>()
                <= 64
        );
        assert!(!valid_id(&"a".repeat(41)));
        assert!(!valid_text(&"界".repeat(171), 512));
    }
}
