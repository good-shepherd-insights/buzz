//! Runtime settings for buzz-acp: every tunable lives in `buzz-acp.settings.toml`.
//!
//! The file is required at startup (`BUZZ_ACP_SETTINGS_FILE`, no default). There are no
//! defaults in code: a missing key is a load error that names the file, the table and the
//! key. Unknown keys are errors too. Any key can be overridden per process with
//! `BUZZ_ACP_SET__<TABLE>__<KEY>=<toml value>` (nested tables add another `__` segment,
//! e.g. `BUZZ_ACP_SET__TEXT__PROMPT_LABELS__PARSED_SEPARATOR`). The override value is parsed
//! as a TOML value; if that fails it is taken as a plain string.
//!
//! GENERATED from the source inventory at commit e982f70 (see SETTINGS_SPEC.md); the struct
//! layout mirrors the toml tables one to one.

// Fields are consumed module by module as each module is migrated.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Deserializer};

const ENV_PREFIX: &str = "BUZZ_ACP_SET__";

/// `deserialize_with` helpers: integer toml values become `Duration`s, the key
/// name carrying the unit (`_secs`, `_ms`).
pub(crate) fn secs<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    u64::deserialize(d).map(Duration::from_secs)
}

pub(crate) fn millis<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    u64::deserialize(d).map(Duration::from_millis)
}

pub(crate) fn secs_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Duration>, D::Error> {
    Ok(Vec::<u64>::deserialize(d)?
        .into_iter()
        .map(Duration::from_secs)
        .collect())
}

pub(crate) fn millis_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Duration>, D::Error> {
    Ok(Vec::<u64>::deserialize(d)?
        .into_iter()
        .map(Duration::from_millis)
        .collect())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelaySettings {
    pub event_channel_capacity_default: usize,
    pub cmd_channel_capacity: usize,
    pub seen_id_limit: usize,
    #[serde(deserialize_with = "secs")]
    pub ping_interval_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub pong_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub ws_send_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub stable_connection_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub since_skew_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub auth_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub connect_timeout_secs: Duration,
    #[serde(deserialize_with = "secs_list")]
    pub startup_connect_backoffs_secs: Vec<Duration>,
    #[serde(deserialize_with = "secs")]
    pub dns_retry_interval_secs: Duration,
    #[serde(deserialize_with = "millis")]
    pub req_pacing_interval_ms: Duration,
    pub drain_budget_per_iter: usize,
    pub gated_observer_queue_cap: usize,
    #[serde(deserialize_with = "millis_list")]
    pub rest_retry_base_delays_ms: Vec<Duration>,
    pub rest_retryable_statuses: Vec<u16>,
    pub query_page_size: usize,
    pub query_event_bound: usize,
    pub max_dns_flat_retries: usize,
    #[serde(deserialize_with = "secs")]
    pub rest_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub rest_connect_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub shutdown_join_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub rate_limit_fallback_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub rate_limit_threshold_secs: Duration,
    pub backpressure_warn_percent: usize,
    #[serde(deserialize_with = "secs")]
    pub req_retry_penalty_secs: Duration,
    #[serde(deserialize_with = "secs_list")]
    pub reconnect_backoffs_secs: Vec<Duration>,
    #[serde(deserialize_with = "secs")]
    pub reconnect_max_delay_secs: Duration,
    pub ws_nonterminal_statuses: Vec<u16>,
    pub ws_nonterminal_status_range_min: u16,
    pub ws_nonterminal_status_range_max: u16,
    pub nip11_paths: Vec<String>,
    #[serde(deserialize_with = "secs")]
    pub recovery_interval_secs: Duration,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueSettings {
    pub max_pending_per_scope: usize,
    pub max_pending_per_channel: usize,
    pub max_batch_events: usize,
    pub max_retries: u32,
    #[serde(deserialize_with = "secs")]
    pub base_retry_delay_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub max_retry_delay_secs: Duration,
    pub retry_exp_cap: u32,
    #[serde(deserialize_with = "secs")]
    pub in_flight_deadline_buffer_secs: Duration,
    pub max_prompt_label_len: usize,
    pub max_description_len: usize,
    pub max_project_name_len: usize,
    pub project_field_max_len: usize,
    pub project_coordinate_max_len: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolSettings {
    #[serde(deserialize_with = "secs")]
    pub recent_activity_window_secs: Duration,
    pub max_hydrated_thread_roots_per_scope: usize,
    #[serde(deserialize_with = "secs")]
    pub project_info_cache_ttl_secs: Duration,
    #[serde(deserialize_with = "millis")]
    pub context_fetch_timeout_ms: Duration,
    #[serde(deserialize_with = "millis")]
    pub context_count_timeout_ms: Duration,
    #[serde(deserialize_with = "millis")]
    pub context_fetch_retry_delay_ms: Duration,
    pub context_fetch_retries: u32,
    #[serde(deserialize_with = "secs")]
    pub model_switch_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub control_cancel_grace_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub permission_mode_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub hold_busy_owner_timeout_secs: Duration,
    pub unknown_channel_name: String,
    #[serde(deserialize_with = "secs")]
    pub core_fetch_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub canvas_fetch_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub metric_timeout_secs: Duration,
    #[serde(deserialize_with = "millis")]
    pub reaction_timeout_ms: Duration,
    pub reaction_concurrency: usize,
    pub overfetch_factor: u32,
    #[serde(deserialize_with = "secs")]
    pub failure_notice_submit_timeout_secs: Duration,
    #[serde(deserialize_with = "millis")]
    pub reaction_remove_query_timeout_ms: Duration,
    #[serde(deserialize_with = "millis")]
    pub reaction_remove_submit_timeout_ms: Duration,
    pub reaction_seen: String,
    pub reaction_working: String,
    pub claude_agent_acp_name: String,
    pub goose_agent_name: String,
    pub legacy_core_label: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibSettings {
    #[serde(deserialize_with = "secs")]
    pub models_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub authenticate_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub observer_publish_tick_secs: Duration,
    pub observer_pending_queue_max_bytes: usize,
    pub observer_chunk_max_text_bytes: usize,
    pub observer_leaf_retain_bytes: usize,
    #[serde(deserialize_with = "secs")]
    pub observer_control_freshness_secs: Duration,
    pub circuit_breaker_threshold: usize,
    #[serde(deserialize_with = "secs")]
    pub circuit_breaker_window_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub circuit_breaker_cooldown_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub respawn_base_delay_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub respawn_max_delay_secs: Duration,
    pub respawn_exp_cap: u32,
    pub jitter_fraction: f64,
    #[serde(deserialize_with = "secs")]
    pub replay_floor_max_age_secs: Duration,
    #[serde(deserialize_with = "millis")]
    pub author_gate_lookup_timeout_ms: Duration,
    pub sibling_cache_max: usize,
    #[serde(deserialize_with = "secs")]
    pub project_announce_skew_secs: Duration,
    #[serde(deserialize_with = "millis")]
    pub run_shutdown_timeout_ms: Duration,
    #[serde(deserialize_with = "secs")]
    pub presence_interval_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub typing_refresh_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub inactivity_tick_max_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub idle_pool_tick_max_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub maintenance_interval_secs: Duration,
    pub membership_seen_rotate: usize,
    #[serde(deserialize_with = "secs")]
    pub relay_gone_exit_delay_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub shutdown_wake_drain_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub shutdown_grace_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub offline_presence_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub initialize_timeout_secs: Duration,
    pub owner_command_shutdown: String,
    pub owner_command_cancel: String,
    pub owner_command_rotate: String,
    pub mcp_name_fallback: String,
    pub failure_notice_hard_timeout: String,
    pub failure_notice_hard_timeout_retries: String,
    pub failure_notice_model_not_found: String,
    pub failure_notice_auth: String,
    pub failure_notice_generic: String,
    pub failure_reason_idle_timeout: String,
    pub failure_reason_hard_timeout: String,
    pub failure_reason_agent_exited: String,
    pub failure_reason_repeated: String,
    pub default_heartbeat_prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcpSettings {
    pub max_line_size_bytes: usize,
    #[serde(deserialize_with = "secs")]
    pub write_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub request_timeout_secs: Duration,
    pub claude_thinking_display_min_version: [u64; 3],
    pub max_version_bytes: u64,
    #[serde(deserialize_with = "secs")]
    pub shutdown_wait_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub cancel_cleanup_min_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub cancel_cleanup_idle_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub claude_version_probe_secs: Duration,
    pub buzz_pi_acp_name: String,
    pub claude_adapter_commands: Vec<String>,
    pub codex_adapter_commands: Vec<String>,
    pub goose_system_prompt_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IsolatedExecutionSettings {
    #[serde(deserialize_with = "secs")]
    pub outer_cancel_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub cancel_grace_secs: Duration,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunTaskSettings {
    pub max_task_bytes: u64,
    #[serde(deserialize_with = "secs")]
    pub input_timeout_secs: Duration,
    pub task_id_max_len: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterSettings {
    pub max_expr_len: usize,
    #[serde(deserialize_with = "millis")]
    pub eval_timeout_ms: Duration,
    pub max_concurrent_filter_evals: usize,
    pub max_consecutive_timeouts: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitSettings {
    pub max_git_user_name_chars: usize,
    pub git_config_count_cap: usize,
    pub email_fallback_host: String,
    pub email_template: String,
    pub tempdir_prefix: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObserverSettings {
    pub buffer_cap: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolLifecycleSettings {
    #[serde(deserialize_with = "secs")]
    pub initial_retry_delay_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub max_retry_delay_secs: Duration,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditRoutingSettings {
    #[serde(deserialize_with = "millis")]
    pub original_fetch_timeout_ms: Duration,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngramSettings {
    pub core_fetch_limit: u32,
    pub onboarding_nudge: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupModeSettings {
    pub instruction_normalized_field: String,
    pub instruction_env_key: String,
    pub instruction_adapter_missing: String,
    pub instruction_adapter_outdated: String,
    pub instruction_cli_missing: String,
    pub instruction_not_installed: String,
    pub instruction_config_invalid: String,
    pub instruction_git_bash: String,
    pub instruction_missing_binary: String,
    pub harness_fallback: String,
    pub cli_fallback: String,
    pub config_file_template: String,
    pub prose_no_requirements: String,
    pub step_template: String,
    pub prose_with_requirements: String,
    pub footer_git_bash: String,
    pub footer_missing_binary: String,
    pub footer_all_external: String,
    pub footer_mixed: String,
    pub footer_all_managed: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigSettings {
    pub relay_url: String,
    pub agent_command: String,
    pub agent_args: Vec<String>,
    pub mcp_command: String,
    #[serde(deserialize_with = "secs")]
    pub max_turn_duration_secs: Duration,
    pub agents: u32,
    pub agents_max: u32,
    #[serde(deserialize_with = "secs")]
    pub heartbeat_interval_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub turn_liveness_secs: Duration,
    pub subscribe: String,
    pub config_file: String,
    pub dedup: String,
    pub session_policy: String,
    pub multiple_event_handling: String,
    pub context_message_limit: u32,
    pub context_message_limit_max: u32,
    pub max_turns_per_session: u32,
    pub memory: bool,
    pub permission_mode: String,
    pub respond_to: String,
    pub relay_observer: bool,
    #[serde(deserialize_with = "secs")]
    pub exit_after_inactivity_secs: Duration,
    pub lazy_pool: bool,
    #[serde(deserialize_with = "secs")]
    pub idle_pool_sleep_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub default_idle_timeout_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub max_turn_duration_ceiling_secs: Duration,
    pub session_title_max_chars: usize,
    pub session_title_separator: String,
    pub session_title_root_chars: usize,
    pub scope_root_short_chars: usize,
    #[serde(deserialize_with = "secs")]
    pub heartbeat_interval_min_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub turn_liveness_min_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub interval_max_secs: Duration,
    pub base_prompt_max_bytes: usize,
    #[serde(deserialize_with = "secs")]
    pub idle_timeout_floor_secs: Duration,
    #[serde(deserialize_with = "secs")]
    pub max_turn_duration_floor_secs: Duration,
    pub max_rules: usize,
    pub default_mention_kinds: Vec<u32>,
    pub agent_default_display: String,
    pub default_agent_args: BTreeMap<String, Vec<String>>,
    pub zero_arg_agents: Vec<String>,
    pub hermes_commands: Vec<String>,
    pub hermes_env: BTreeMap<String, String>,
    pub codex_commands: Vec<String>,
    pub codex_config_json: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextSettings {
    pub prompt_labels: PromptLabelsSettings,
    pub base_prompt: String,
    pub session_model_channel: String,
    pub session_model_thread: String,
    pub session_model_task: String,
    pub reply_instruction: String,
    pub new_thread_reply_instruction: String,
    pub project_block: String,
    pub project_default_repo: String,
    pub project_default_repo_none: String,
    pub project_home_hint: String,
    pub dm_hint_thread_complete: String,
    pub dm_hint_conversation_complete: String,
    pub dm_hint_thread_included: String,
    pub dm_hint_conversation_included: String,
    pub dm_hint_thread_previous: String,
    pub dm_hint_conversation_previous: String,
    pub dm_hint_thread_fetch: String,
    pub dm_hint_conversation_fetch: String,
    pub thread_hint_complete: String,
    pub thread_hint_included: String,
    pub thread_hint_previous: String,
    pub thread_hint_fetch: String,
    pub channel_hint: String,
    pub edit_note_original_fetched: String,
    pub edit_note_original_missing: String,
    pub merge_steer_prior_tag: String,
    pub merge_steer_new_tag: String,
    pub merge_steer_closing_note: String,
    pub merge_interrupt_prior_tag: String,
    pub merge_interrupt_new_tag: String,
    pub merge_interrupt_closing_note: String,
    pub tag_buzz_event: String,
    pub tag_buzz_events: String,
    pub tag_context: String,
    pub tag_thread_context: String,
    pub tag_conversation_context: String,
    pub tag_base: String,
    pub tag_agent_instructions: String,
    pub tag_team_instructions: String,
    pub tag_core_memory: String,
    pub tag_huddle_instructions: String,
    pub tag_channel_canvas: String,
    pub legacy_label_channel_canvas: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptLabelsSettings {
    pub event_block: String,
    pub event_from_labeled: String,
    pub event_from_unlabeled: String,
    pub actor_labeled: String,
    pub channel_display_named: String,
    pub tags_line: String,
    pub edit_line: String,
    pub parsed_line: String,
    pub parsed_parent: String,
    pub parsed_root: String,
    pub parsed_mentions: String,
    pub parsed_separator: String,
    pub mentions_separator: String,
    pub event_header: String,
    pub event_separator: String,
    pub conversation_message: String,
    pub context_dm: String,
    pub context_thread: String,
    pub context_channel: String,
    pub session_scope_thread: String,
    pub session_scope_channel: String,
    pub thread_root_line: String,
    pub parent_line: String,
    pub hint_line: String,
    pub description_inline: String,
    pub description_block: String,
    pub description_indent: String,
    pub ellipsis: String,
}

/// The full settings document: one sub-struct per toml table.
#[derive(Debug, Clone)]
pub struct Settings {
    pub relay: RelaySettings,
    pub queue: QueueSettings,
    pub pool: PoolSettings,
    pub lib: LibSettings,
    pub acp: AcpSettings,
    pub isolated_execution: IsolatedExecutionSettings,
    pub run_task: RunTaskSettings,
    pub filter: FilterSettings,
    pub git: GitSettings,
    pub observer: ObserverSettings,
    pub pool_lifecycle: PoolLifecycleSettings,
    pub edit_routing: EditRoutingSettings,
    pub engram: EngramSettings,
    pub setup_mode: SetupModeSettings,
    pub config: ConfigSettings,
    pub text: TextSettings,
    /// Goal loop; `None` leaves the feature off.
    pub goal: Option<crate::goal::GoalSettings>,
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();

/// Install the process-wide settings. The first call wins; later calls are ignored.
pub fn init(settings: Settings) {
    let _ = SETTINGS.set(settings);
}

/// Load the settings file named by `--settings-file` / `BUZZ_ACP_SETTINGS_FILE` and install it.
pub fn init_from(path: Option<&Path>) -> Result<()> {
    let path =
        path.ok_or_else(|| anyhow!("BUZZ_ACP_SETTINGS_FILE (--settings-file) is not set"))?;
    init(load(path)?);
    Ok(())
}

/// The process-wide settings, or `None` before [`init`] has run.
pub fn try_get() -> Option<&'static Settings> {
    SETTINGS.get()
}

/// The process-wide settings. Panics if [`init`] has not run.
pub fn get() -> &'static Settings {
    SETTINGS
        .get()
        .unwrap_or_else(|| panic!("settings not initialised: set BUZZ_ACP_SETTINGS_FILE"))
}

/// Test-only: initialise from the shipped toml, once per process.
#[cfg(test)]
pub fn init_for_tests() {
    SETTINGS.get_or_init(|| {
        load_from_str(
            include_str!("../buzz-acp.settings.toml"),
            "buzz-acp.settings.toml (embedded for tests)",
            &[],
        )
        .expect("shipped buzz-acp.settings.toml must load")
    });
}

/// Replace every `{name}` in `template` with the matching value in `vars`.
///
/// Single pass: substituted values are never re-scanned, and unknown `{...}` text is left as is.
pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                if let Some((_, value)) = vars.iter().find(|(n, _)| *n == name) {
                    out.push_str(value);
                    rest = &after[close + 1..];
                } else {
                    out.push('{');
                    rest = after;
                }
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Read, merge env overrides from the process environment, and validate the settings file.
pub fn load(path: &Path) -> Result<Settings> {
    let origin = path.display().to_string();
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read settings file {origin}"))?;
    let env: Vec<(String, String)> = std::env::vars().collect();
    load_from_str(&content, &origin, &env)
}

/// Parse `content` (named `origin` in errors) and apply `BUZZ_ACP_SET__*` overrides from `env`.
pub(crate) fn load_from_str(
    content: &str,
    origin: &str,
    env: &[(String, String)],
) -> Result<Settings> {
    let mut root: toml::Table = content
        .parse()
        .map_err(|e| anyhow!("{origin}: not valid TOML: {e}"))?;
    let applied = apply_env_overrides(&mut root, origin, env)?;
    Settings::from_root(root, origin, &applied)
}

fn parse_env_value(raw: &str) -> toml::Value {
    format!("v = {raw}")
        .parse::<toml::Table>()
        .ok()
        .and_then(|mut table| table.remove("v"))
        .unwrap_or_else(|| toml::Value::String(raw.to_string()))
}

/// Merge `BUZZ_ACP_SET__<TABLE>__<KEY>` variables into the parsed document.
/// Returns the names of the variables applied, for error context.
fn apply_env_overrides(
    root: &mut toml::Table,
    origin: &str,
    env: &[(String, String)],
) -> Result<Vec<String>> {
    let mut vars: Vec<&(String, String)> = env
        .iter()
        .filter(|(k, _)| k.starts_with(ENV_PREFIX))
        .collect();
    vars.sort();
    let mut applied = Vec::new();
    for (name, raw) in vars {
        let path: Vec<String> = name[ENV_PREFIX.len()..]
            .split("__")
            .map(str::to_ascii_lowercase)
            .collect();
        let Some((key, tables)) = path
            .split_last()
            .filter(|(_, tables)| !tables.is_empty() && !path.iter().any(String::is_empty))
        else {
            return Err(anyhow!(
                "{origin}: env override {name} must look like {ENV_PREFIX}<TABLE>__<KEY>"
            ));
        };
        let mut cursor = &mut *root;
        for table in tables {
            let entry = cursor
                .entry(table.clone())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            cursor = entry.as_table_mut().ok_or_else(|| {
                anyhow!("{origin}: env override {name}: `{table}` is not a table")
            })?;
        }
        cursor.insert(key.clone(), parse_env_value(raw));
        applied.push(name.clone());
    }
    Ok(applied)
}

fn table_error(origin: &str, table: &str, message: &str, applied: &[String]) -> anyhow::Error {
    let prefix = format!(
        "BUZZ_ACP_SET__{}__",
        table.to_ascii_uppercase().replace('.', "__")
    );
    let relevant: Vec<&String> = applied.iter().filter(|v| v.starts_with(&prefix)).collect();
    if relevant.is_empty() {
        anyhow!("{origin}: invalid settings table [{table}]: {message}")
    } else {
        anyhow!(
            "{origin}: invalid settings table [{table}]: {message} (env overrides applied to this table: {})",
            relevant.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        )
    }
}

fn take_value<T: serde::de::DeserializeOwned>(
    value: toml::Value,
    origin: &str,
    table: &str,
    applied: &[String],
) -> Result<T> {
    value
        .try_into::<T>()
        .map_err(|e| table_error(origin, table, e.message(), applied))
}

fn take_table<T: serde::de::DeserializeOwned>(
    root: &mut toml::Table,
    origin: &str,
    table: &str,
    applied: &[String],
) -> Result<T> {
    let value = root
        .remove(table)
        .ok_or_else(|| anyhow!("{origin}: missing settings table [{table}]"))?;
    take_value(value, origin, table, applied)
}

impl Settings {
    fn from_root(mut root: toml::Table, origin: &str, applied: &[String]) -> Result<Settings> {
        let relay = take_table::<RelaySettings>(&mut root, origin, "relay", &applied)?;
        let queue = take_table::<QueueSettings>(&mut root, origin, "queue", &applied)?;
        let pool = take_table::<PoolSettings>(&mut root, origin, "pool", &applied)?;
        let lib = take_table::<LibSettings>(&mut root, origin, "lib", &applied)?;
        let acp = take_table::<AcpSettings>(&mut root, origin, "acp", &applied)?;
        let isolated_execution = take_table::<IsolatedExecutionSettings>(
            &mut root,
            origin,
            "isolated_execution",
            &applied,
        )?;
        let run_task = take_table::<RunTaskSettings>(&mut root, origin, "run_task", &applied)?;
        let filter = take_table::<FilterSettings>(&mut root, origin, "filter", &applied)?;
        let git = take_table::<GitSettings>(&mut root, origin, "git", &applied)?;
        let observer = take_table::<ObserverSettings>(&mut root, origin, "observer", &applied)?;
        let pool_lifecycle =
            take_table::<PoolLifecycleSettings>(&mut root, origin, "pool_lifecycle", &applied)?;
        let edit_routing =
            take_table::<EditRoutingSettings>(&mut root, origin, "edit_routing", &applied)?;
        let engram = take_table::<EngramSettings>(&mut root, origin, "engram", &applied)?;
        let setup_mode =
            take_table::<SetupModeSettings>(&mut root, origin, "setup_mode", &applied)?;
        let config = take_table::<ConfigSettings>(&mut root, origin, "config", &applied)?;
        // Deserialize the nested table first so its errors name [text.prompt_labels].
        if let Some(labels) = root
            .get("text")
            .and_then(|t| t.get("prompt_labels"))
            .cloned()
        {
            let _ =
                take_value::<PromptLabelsSettings>(labels, origin, "text.prompt_labels", &applied)?;
        }
        let text = take_table::<TextSettings>(&mut root, origin, "text", &applied)?;
        let goal = root
            .remove("goal")
            .map(|value| take_value::<crate::goal::GoalSettings>(value, origin, "goal", &applied))
            .transpose()?;
        if !root.is_empty() {
            let extra: Vec<&str> = root.keys().map(String::as_str).collect();
            return Err(anyhow!(
                "{origin}: unknown settings table(s): {}",
                extra.join(", ")
            ));
        }
        Ok(Settings {
            relay,
            queue,
            pool,
            lib,
            acp,
            isolated_execution,
            run_task,
            filter,
            git,
            observer,
            pool_lifecycle,
            edit_routing,
            engram,
            setup_mode,
            config,
            text,
            goal,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: &str = include_str!("../buzz-acp.settings.toml");
    const ORIGIN: &str = "test-settings.toml";

    fn without_line(prefix: &str) -> String {
        let kept: Vec<&str> = SHIPPED.lines().filter(|l| !l.starts_with(prefix)).collect();
        assert_ne!(
            kept.len(),
            SHIPPED.lines().count(),
            "line {prefix:?} not found"
        );
        kept.join("\n")
    }

    #[test]
    fn shipped_toml_parses() {
        let s = load_from_str(SHIPPED, ORIGIN, &[]).expect("shipped settings must parse");
        assert_eq!(s.relay.ping_interval_secs, Duration::from_secs(30));
        assert_eq!(s.relay.req_pacing_interval_ms, Duration::from_millis(125));
        assert_eq!(s.relay.startup_connect_backoffs_secs.len(), 5);
        assert_eq!(s.queue.max_pending_per_scope, 500);
        assert_eq!(s.config.default_mention_kinds, vec![9, 40003, 46010, 40007]);
        assert_eq!(
            s.config.default_agent_args.get("goose"),
            Some(&vec!["acp".to_string()])
        );
        assert_eq!(s.pool.reaction_seen, "\u{1F440}");
        assert!(!s.text.base_prompt.is_empty());
        assert!(s.text.prompt_labels.event_block.contains("{event_id}"));
        assert_eq!(s.git.email_template, "{pubkey}@{host}");
    }

    #[test]
    fn init_for_tests_makes_get_available() {
        init_for_tests();
        assert_eq!(get().queue.max_batch_events, 50);
    }

    #[test]
    fn missing_key_fails_naming_file_table_and_key() {
        let content = without_line("max_pending_per_scope = ");
        let err = load_from_str(&content, ORIGIN, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("max_pending_per_scope"), "{err}");
        assert!(err.contains(ORIGIN), "{err}");
        assert!(err.contains("[queue]"), "{err}");
    }

    #[test]
    fn missing_nested_key_names_nested_table() {
        let content = without_line("parsed_separator = ");
        let err = load_from_str(&content, ORIGIN, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("parsed_separator"), "{err}");
        assert!(err.contains("[text.prompt_labels]"), "{err}");
    }

    #[test]
    fn missing_table_fails_naming_it() {
        let content = SHIPPED.replace("[observer]", "[observer_renamed]");
        let err = load_from_str(&content, ORIGIN, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("observer"), "{err}");
        assert!(err.contains(ORIGIN), "{err}");
    }

    #[test]
    fn unknown_key_fails() {
        let content = SHIPPED.replacen(
            "buffer_cap = 1000\n",
            "buffer_cap = 1000\nbogus_key = 1\n",
            1,
        );
        assert_ne!(content, SHIPPED);
        let err = load_from_str(&content, ORIGIN, &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("bogus_key"), "{err}");
        assert!(err.contains("[observer]"), "{err}");
    }

    fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn env_override_applies_typed_and_string_values() {
        let vars = env(&[
            ("BUZZ_ACP_SET__RELAY__PING_INTERVAL_SECS", "45"),
            ("BUZZ_ACP_SET__GIT__EMAIL_FALLBACK_HOST", "example"),
            ("BUZZ_ACP_SET__TEXT__PROMPT_LABELS__PARSED_SEPARATOR", "; "),
            ("BUZZ_ACP_SET__RELAY__RECONNECT_BACKOFFS_SECS", "[1, 2]"),
            ("BUZZ_ACP_SET__CONFIG__MEMORY", "false"),
            ("UNRELATED", "x"),
        ]);
        let s = load_from_str(SHIPPED, ORIGIN, &vars).unwrap();
        assert_eq!(s.relay.ping_interval_secs, Duration::from_secs(45));
        assert_eq!(s.git.email_fallback_host, "example");
        assert_eq!(s.text.prompt_labels.parsed_separator, "; ");
        assert_eq!(
            s.relay.reconnect_backoffs_secs,
            vec![Duration::from_secs(1), Duration::from_secs(2)]
        );
        assert!(!s.config.memory);
    }

    #[test]
    fn env_override_with_wrong_type_names_the_variable() {
        let vars = env(&[("BUZZ_ACP_SET__RELAY__PING_INTERVAL_SECS", "not-a-number")]);
        let err = load_from_str(SHIPPED, ORIGIN, &vars)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("BUZZ_ACP_SET__RELAY__PING_INTERVAL_SECS"),
            "{err}"
        );
        assert!(err.contains("[relay]"), "{err}");
    }

    #[test]
    fn env_override_of_unknown_key_fails() {
        let vars = env(&[("BUZZ_ACP_SET__RELAY__NOPE", "1")]);
        let err = load_from_str(SHIPPED, ORIGIN, &vars)
            .unwrap_err()
            .to_string();
        assert!(err.contains("nope"), "{err}");
    }

    #[test]
    fn render_substitutes_named_placeholders() {
        assert_eq!(
            render("a {x} b {y} c {x}", &[("x", "1"), ("y", "2")]),
            "a 1 b 2 c 1"
        );
        // Values are not re-scanned; unknown placeholders and stray braces survive.
        assert_eq!(
            render("{x} {z} { {", &[("x", "{y}"), ("y", "no")]),
            "{y} {z} { {"
        );
        assert_eq!(render("no placeholders", &[]), "no placeholders");
    }
}
