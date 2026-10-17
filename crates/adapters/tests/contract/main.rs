//! The adapter contract: each case states one behaviour every CLI adapter
//! must have, and runs against every subject. A subject that cannot express
//! a fact fails the case; nothing is skipped.
//!
//! The process cases need a process of their own to kill, interrupt or read
//! the stderr of, so only the real subjects run them.

mod cases;
mod process;
mod subject;
mod wire;

use subject::{ClaudeCodeSubject, CodexSubject, MockSubject};

/// Expands every contract case as a test of one module: `<module>::<case>`.
/// A subject given a process macro runs its process cases too.
macro_rules! adapter_contract {
    ($module:ident, $subject:expr $(, $process:ident)?) => {
        mod $module {
            use super::*;

            #[tokio::test]
            async fn failure_causes_follow_the_classifier_table() {
                cases::failure_causes_follow_the_classifier_table(&$subject).await
            }

            #[tokio::test]
            async fn same_run_tool_call_is_logged_under_one_name() {
                cases::same_run_tool_call_is_logged_under_one_name(&$subject).await
            }

            #[tokio::test]
            async fn profile_gives_the_same_mode_and_coverage() {
                cases::profile_gives_the_same_mode_and_coverage(&$subject).await
            }

            #[tokio::test]
            async fn a_launch_without_a_hook_opens_nothing_and_stages_nothing() {
                cases::a_launch_without_a_hook_opens_nothing_and_stages_nothing(&$subject).await
            }

            #[tokio::test]
            async fn launch_installs_the_hooks_it_needs() {
                cases::launch_installs_the_hooks_it_needs(&$subject).await
            }

            #[tokio::test]
            async fn tokens_are_counted_once_and_absent_stays_absent() {
                cases::tokens_are_counted_once_and_absent_stays_absent(&$subject).await
            }

            #[tokio::test]
            async fn session_reports_the_model_the_cli_reports() {
                cases::session_reports_the_model_the_cli_reports(&$subject).await
            }

            #[tokio::test]
            async fn skills_are_staged_and_taken_back() {
                cases::skills_are_staged_and_taken_back(&$subject).await
            }

            #[tokio::test]
            async fn a_named_agent_reaches_the_cli() {
                cases::a_named_agent_reaches_the_cli(&$subject).await
            }

            #[tokio::test]
            async fn effort_reaches_the_cli_or_is_left_to_it() {
                cases::effort_reaches_the_cli_or_is_left_to_it(&$subject).await
            }

            #[tokio::test]
            async fn isolated_session_loads_no_user_config() {
                cases::isolated_session_loads_no_user_config(&$subject).await
            }

            #[tokio::test]
            async fn network_opens_only_under_workspace_write() {
                cases::network_opens_only_under_workspace_write(&$subject).await
            }

            #[tokio::test]
            async fn fence_level_and_rail_codec_agree() {
                cases::fence_level_and_rail_codec_agree(&$subject).await
            }

            #[tokio::test]
            async fn an_unknown_stream_line_is_tolerated_and_named() {
                cases::an_unknown_stream_line_is_tolerated_and_named(&$subject).await
            }

            #[tokio::test]
            async fn a_line_before_the_session_opens_fails_it() {
                cases::a_line_before_the_session_opens_fails_it(&$subject).await
            }

            #[tokio::test]
            async fn a_crashed_session_ends_the_stream_with_no_terminal_event() {
                cases::a_crashed_session_ends_the_stream_with_no_terminal_event(&$subject).await
            }

            #[tokio::test]
            async fn a_session_never_opens_under_unreadable_settings() {
                cases::a_session_never_opens_under_unreadable_settings(&$subject).await
            }

            #[tokio::test]
            async fn an_unknown_adapter_setting_is_reported_by_probe() {
                cases::an_unknown_adapter_setting_is_reported_by_probe(&$subject).await
            }

            #[tokio::test]
            async fn two_refused_stops_in_a_row_keep_the_session_alive() {
                cases::two_refused_stops_in_a_row_keep_the_session_alive(&$subject).await
            }

            #[tokio::test]
            async fn probe_reports_the_stub_version() {
                cases::probe_reports_the_stub_version(&$subject).await
            }

            #[tokio::test]
            async fn capabilities_are_the_ones_the_cli_declares() {
                cases::capabilities_are_the_ones_the_cli_declares(&$subject).await
            }

            #[tokio::test]
            async fn a_tool_use_never_persists_the_command_it_ran() {
                cases::a_tool_use_never_persists_the_command_it_ran(&$subject).await
            }

            #[tokio::test]
            async fn the_prompt_never_reaches_argv() {
                cases::the_prompt_never_reaches_argv(&$subject).await
            }

            $( $process!($subject); )?
        }
    };
}

/// The cases that need a process of their own, for a subject that runs one.
macro_rules! process_cases {
    ($subject:expr) => {
        #[tokio::test]
        async fn kill_terminates_the_whole_process_tree_including_grandchildren() {
            process::kill_terminates_the_whole_process_tree_including_grandchildren(&$subject).await
        }

        #[tokio::test]
        async fn a_dead_sessions_stderr_tail_never_carries_a_value_from_its_env() {
            process::a_dead_sessions_stderr_tail_never_carries_a_value_from_its_env(&$subject).await
        }

        #[tokio::test]
        async fn prompt_travels_by_stdin_never_argv() {
            process::prompt_travels_by_stdin_never_argv(&$subject).await
        }

        #[tokio::test]
        async fn an_interrupted_session_that_leaves_ends_its_grace_at_once() {
            process::an_interrupted_session_that_leaves_ends_its_grace_at_once(&$subject).await
        }
    };
}

// The mock has no process to kill, so it runs no process case.
adapter_contract!(mock, MockSubject);
adapter_contract!(claude_code, ClaudeCodeSubject, process_cases);
adapter_contract!(codex, CodexSubject, process_cases);
