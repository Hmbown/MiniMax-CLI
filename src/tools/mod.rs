//! Tool system modules and re-exports.

#![allow(dead_code, unused_imports)]

// === Modules ===

pub mod artifact;
pub mod calculator;
pub mod coding;
pub mod duo;
pub mod execution;
pub mod file;
pub mod finance;
pub mod git;
pub mod investigator;
pub mod memory;
pub mod minimax;
pub mod parallel;
pub mod patch;
pub mod plan;
pub mod registry;
pub mod rlm;
pub mod search;
pub mod security;
pub mod shell;
pub mod spec;
pub mod sports;
pub mod subagent;
pub mod think;
pub mod time;
pub mod todo;
pub mod user_input;
pub mod weather;
pub mod web_run;
pub mod web_search;

// === Re-exports ===

// Re-export commonly used types from spec
pub use spec::ToolContext;

// Re-export coding tools
pub use calculator::CalculatorTool;
pub use coding::{CodingCompleteTool, CodingReviewTool};

// Re-export git tools
pub use git::{GitBranchTool, GitCommitTool, GitDiffTool, GitLogTool, GitStatusTool};

// Re-export memory tools
pub use finance::FinanceTool;
pub use memory::{GetMemoryTool, SaveMemoryTool};
pub use parallel::MultiToolUseParallelTool;

// Re-export artifact tools
pub use artifact::{ArtifactCreateTool, ArtifactListTool};

// Re-export execution tools
pub use execution::ExecPythonTool;

// Re-export investigator tools
pub use investigator::CodebaseInvestigatorTool;

// Re-export minimax tools
pub use minimax::{
    AnalyzeImageTool, DeleteFileTool, DownloadFileTool, GenerateImageTool, GenerateMusicTool,
    GenerateVideoTool, ListFilesTool, QueryVideoTool, RetrieveFileTool, TtsAsyncCreateTool,
    TtsAsyncQueryTool, TtsTool, UploadFileTool, VideoTemplateCreateTool, VideoTemplateQueryTool,
    VoiceCloneTool, VoiceDeleteTool, VoiceDesignTool, VoiceListTool,
};

// Re-export registry types
pub use registry::{ToolRegistry, ToolRegistryBuilder};

// Re-export search tools
pub use search::GrepFilesTool;
pub use sports::SportsTool;
pub use time::TimeTool;

// Re-export web search tools
pub use weather::WeatherTool;
pub use web_run::WebRunTool;
pub use web_search::{WebFetchTool, WebSearchTool};

// Re-export patch tools
pub use patch::ApplyPatchTool;

// Re-export file tools
pub use file::{EditFileTool, ListDirTool, ReadFileTool, WriteFileTool};

// Re-export shell types
pub use shell::{ExecShellInteractTool, ExecShellKillTool, ExecShellTool, ExecShellWaitTool};

// Re-export subagent types
pub use subagent::SubAgent;

// Re-export todo types
pub use todo::TodoWriteTool;

// Re-export plan types
pub use plan::UpdatePlanTool;
pub use user_input::RequestUserInputTool;

// Re-export RLM tools
pub use rlm::{RlmExecTool, RlmLoadTool, RlmQueryTool, RlmStatusTool};

// Re-export think tool
pub use think::ThinkTool;

#[cfg(test)]
mod runtime_tool_schema_tests {
    use super::{
        CalculatorTool, FinanceTool, MultiToolUseParallelTool, RequestUserInputTool, SportsTool,
        TimeTool, WeatherTool, WebRunTool,
    };
    use crate::tools::spec::ToolSpec;
    use serde_json::Value;

    fn assert_schema_has_properties(schema: Value) {
        let obj = schema
            .as_object()
            .expect("tool schema should be a JSON object");
        assert_eq!(
            obj.get("type").and_then(Value::as_str),
            Some("object"),
            "tool schema should declare type=object"
        );
        assert!(
            obj.get("properties")
                .and_then(Value::as_object)
                .is_some_and(|props| !props.is_empty()),
            "tool schema should expose properties"
        );
    }

    #[test]
    fn runtime_parity_tool_schemas_are_valid_objects() {
        assert_schema_has_properties(WebRunTool.input_schema());
        assert_schema_has_properties(MultiToolUseParallelTool.input_schema());
        assert_schema_has_properties(RequestUserInputTool.input_schema());
        assert_schema_has_properties(WeatherTool.input_schema());
        assert_schema_has_properties(FinanceTool.input_schema());
        assert_schema_has_properties(SportsTool.input_schema());
        assert_schema_has_properties(TimeTool.input_schema());
        assert_schema_has_properties(CalculatorTool.input_schema());
    }
}
