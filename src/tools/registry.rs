//! Tool registry for managing and executing tools.
//!
//! The registry provides:
//! - Dynamic tool registration
//! - Tool lookup by name
//! - Conversion to API Tool format
//! - Filtering by capability

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::client::MiniMaxTextClient;
use crate::duo::SharedDuoSession;
use crate::models::Tool;
use crate::rlm::SharedRlmSession;

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};

// === Types ===

/// Registry that holds all available tools.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolSpec>>,
    context: ToolContext,
}

impl ToolRegistry {
    /// Create a new empty registry with the given context.
    #[must_use]
    pub fn new(context: ToolContext) -> Self {
        Self {
            tools: HashMap::new(),
            context,
        }
    }

    /// Register a tool in the registry.
    pub fn register(&mut self, tool: Arc<dyn ToolSpec>) {
        let name = tool.name().to_string();
        if self.tools.insert(name.clone(), tool).is_some() {
            tracing::warn!("Overwriting existing tool: {}", name);
        }
    }

    /// Register multiple tools at once.
    pub fn register_all(&mut self, tools: Vec<Arc<dyn ToolSpec>>) {
        for tool in tools {
            self.register(tool);
        }
    }

    /// Get a tool by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolSpec>> {
        self.tools.get(name).cloned()
    }

    /// Check if a tool exists.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Check if a tool supports parallel execution.
    #[must_use]
    pub fn tool_supports_parallel(&self, name: &str) -> bool {
        self.get(name)
            .map(|tool| tool.supports_parallel())
            .unwrap_or(false)
    }

    /// Get all registered tool names.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(std::string::String::as_str).collect()
    }

    /// Get the number of registered tools.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Get all registered tools.
    #[must_use]
    pub fn all(&self) -> Vec<Arc<dyn ToolSpec>> {
        self.tools.values().cloned().collect()
    }

    /// Execute a tool by name with the given input.
    pub async fn execute(&self, name: &str, input: Value) -> Result<String, ToolError> {
        let tool = self
            .get(name)
            .ok_or_else(|| ToolError::not_available(format!("tool '{name}' is not registered")))?;

        let result = tool.execute(input, &self.context).await?;
        Ok(result.content)
    }

    /// Execute a tool by name, returning the full `ToolResult`.
    pub async fn execute_full(&self, name: &str, input: Value) -> Result<ToolResult, ToolError> {
        let tool = self
            .get(name)
            .ok_or_else(|| ToolError::not_available(format!("tool '{name}' is not registered")))?;

        tool.execute(input, &self.context).await
    }

    /// Convert all tools to API Tool format for sending to the model.
    #[must_use]
    pub fn to_api_tools(&self) -> Vec<Tool> {
        self.tools
            .values()
            .map(|tool| Tool {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
                cache_control: None,
            })
            .collect()
    }

    /// Convert tools to API Tool format with optional cache control on the last tool.
    #[must_use]
    pub fn to_api_tools_with_cache(&self, enable_cache: bool) -> Vec<Tool> {
        let mut tools = self.to_api_tools();
        if enable_cache && let Some(last) = tools.last_mut() {
            last.cache_control = Some(crate::models::CacheControl {
                cache_type: "ephemeral".to_string(),
            });
        }
        tools
    }

    /// Filter tools by capability.
    #[must_use]
    pub fn filter_by_capability(&self, capability: ToolCapability) -> Vec<Arc<dyn ToolSpec>> {
        self.tools
            .values()
            .filter(|t| t.capabilities().contains(&capability))
            .cloned()
            .collect()
    }

    /// Get read-only tools (for Normal mode).
    #[must_use]
    pub fn read_only_tools(&self) -> Vec<Arc<dyn ToolSpec>> {
        self.tools
            .values()
            .filter(|t| t.is_read_only())
            .cloned()
            .collect()
    }

    /// Get tools that require approval.
    #[must_use]
    pub fn approval_required_tools(&self) -> Vec<Arc<dyn ToolSpec>> {
        self.tools
            .values()
            .filter(|t| t.approval_requirement() == ApprovalRequirement::Required)
            .cloned()
            .collect()
    }

    /// Get tools that suggest approval.
    #[must_use]
    pub fn approval_suggested_tools(&self) -> Vec<Arc<dyn ToolSpec>> {
        self.tools
            .values()
            .filter(|t| {
                matches!(
                    t.approval_requirement(),
                    ApprovalRequirement::Suggest | ApprovalRequirement::Required
                )
            })
            .cloned()
            .collect()
    }

    /// Update the context (e.g., when workspace changes).
    pub fn set_context(&mut self, context: ToolContext) {
        self.context = context;
    }

    /// Get a reference to the current context.
    #[must_use]
    pub fn context(&self) -> &ToolContext {
        &self.context
    }

    /// Get a mutable reference to the current context.
    #[must_use]
    pub fn context_mut(&mut self) -> &mut ToolContext {
        &mut self.context
    }

    /// Remove a tool by name.
    #[must_use]
    pub fn remove(&mut self, name: &str) -> Option<Arc<dyn ToolSpec>> {
        self.tools.remove(name)
    }

    /// Clear all tools from the registry.
    pub fn clear(&mut self) {
        self.tools.clear();
    }
}

/// Builder for constructing a `ToolRegistry` with common tools.
pub struct ToolRegistryBuilder {
    tools: Vec<Arc<dyn ToolSpec>>,
}

impl ToolRegistryBuilder {
    /// Create a new builder.
    #[must_use]
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Add a custom tool.
    #[must_use]
    pub fn with_tool(mut self, tool: Arc<dyn ToolSpec>) -> Self {
        self.tools.push(tool);
        self
    }

    /// Add multiple tools.
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<Arc<dyn ToolSpec>>) -> Self {
        self.tools.extend(tools);
        self
    }

    /// Include file tools (read, write, edit, list).
    #[must_use]
    pub fn with_file_tools(self) -> Self {
        use super::file::{EditFileTool, ListDirTool, ReadFileTool, WriteFileTool};
        self.with_tool(Arc::new(ReadFileTool))
            .with_tool(Arc::new(WriteFileTool))
            .with_tool(Arc::new(EditFileTool))
            .with_tool(Arc::new(ListDirTool))
    }

    /// Include only read-only file tools (read, list).
    #[must_use]
    pub fn with_read_only_file_tools(self) -> Self {
        use super::file::{ListDirTool, ReadFileTool};
        self.with_tool(Arc::new(ReadFileTool))
            .with_tool(Arc::new(ListDirTool))
    }

    /// Include shell execution tool.
    #[must_use]
    pub fn with_shell_tools(self) -> Self {
        use super::shell::{
            ExecShellCleanTool, ExecShellInteractTool, ExecShellKillTool, ExecShellListTool,
            ExecShellTool, ExecShellWaitTool,
        };
        self.with_tool(Arc::new(ExecShellTool))
            .with_tool(Arc::new(ExecShellWaitTool::new("exec_shell_wait")))
            .with_tool(Arc::new(ExecShellWaitTool::new("exec_wait")))
            .with_tool(Arc::new(ExecShellKillTool::new("exec_shell_kill")))
            .with_tool(Arc::new(ExecShellKillTool::new("exec_kill")))
            .with_tool(Arc::new(ExecShellListTool::new("exec_shell_list")))
            .with_tool(Arc::new(ExecShellListTool::new("exec_list")))
            .with_tool(Arc::new(ExecShellCleanTool::new("exec_shell_clean")))
            .with_tool(Arc::new(ExecShellCleanTool::new("exec_clean")))
            .with_tool(Arc::new(ExecShellInteractTool::new("exec_shell_interact")))
            .with_tool(Arc::new(ExecShellInteractTool::new("exec_interact")))
    }

    /// Include search tools (`grep_files`).
    #[must_use]
    pub fn with_search_tools(self) -> Self {
        use super::search::GrepFilesTool;
        self.with_tool(Arc::new(GrepFilesTool))
    }

    /// Include think tool for step-by-step reasoning.
    #[must_use]
    pub fn with_think_tool(self) -> Self {
        use super::think::ThinkTool;
        self.with_tool(Arc::new(ThinkTool))
    }

    /// Include web search tools.
    #[must_use]
    pub fn with_web_tools(self) -> Self {
        use super::web_search::{WebFetchTool, WebSearchTool};
        self.with_tool(Arc::new(WebSearchTool))
            .with_tool(Arc::new(WebFetchTool))
    }

    /// Include patch tools (`apply_patch`).
    #[must_use]
    pub fn with_patch_tools(self) -> Self {
        use super::patch::ApplyPatchTool;
        self.with_tool(Arc::new(ApplyPatchTool))
    }

    /// Include git tools.
    #[must_use]
    pub fn with_git_tools(self) -> Self {
        use super::git::{GitBranchTool, GitCommitTool, GitDiffTool, GitLogTool, GitStatusTool};
        self.with_tool(Arc::new(GitDiffTool))
            .with_tool(Arc::new(GitStatusTool))
            .with_tool(Arc::new(GitCommitTool))
            .with_tool(Arc::new(GitLogTool))
            .with_tool(Arc::new(GitBranchTool))
    }

    /// Include note tool.
    #[must_use]
    pub fn with_note_tool(self) -> Self {
        use super::shell::NoteTool;
        self.with_tool(Arc::new(NoteTool))
    }

    /// Include memory tools.
    #[must_use]
    pub fn with_memory_tools(self, memory_path: std::path::PathBuf) -> Self {
        use super::memory::{GetMemoryTool, SaveMemoryTool};
        self.with_tool(Arc::new(SaveMemoryTool::new(memory_path.clone())))
            .with_tool(Arc::new(GetMemoryTool::new(memory_path)))
    }

    /// Include artifact tools.
    #[must_use]
    pub fn with_artifact_tools(self) -> Self {
        use super::artifact::{ArtifactCreateTool, ArtifactListTool};
        self.with_tool(Arc::new(ArtifactCreateTool))
            .with_tool(Arc::new(ArtifactListTool))
    }

    /// Include execution tools.
    #[must_use]
    pub fn with_execution_tools(self) -> Self {
        use super::execution::ExecPythonTool;
        self.with_tool(Arc::new(ExecPythonTool))
    }

    /// Include investigator tools.
    #[must_use]
    pub fn with_investigator_tool(
        self,
        manager: super::subagent::SharedSubAgentManager,
        runtime: super::subagent::SubAgentRuntime,
    ) -> Self {
        use super::investigator::CodebaseInvestigatorTool;
        self.with_tool(Arc::new(CodebaseInvestigatorTool::new(manager, runtime)))
    }

    /// Include security tools.
    #[must_use]
    pub fn with_security_tool(
        self,
        manager: super::subagent::SharedSubAgentManager,
        runtime: super::subagent::SubAgentRuntime,
    ) -> Self {
        use super::security::SecurityAnalyzeTool;
        self.with_tool(Arc::new(SecurityAnalyzeTool::new(manager, runtime)))
    }

    /// Include all agent tools (file tools + shell + note + search + think + patch + git).
    #[must_use]
    pub fn with_agent_tools(self, allow_shell: bool) -> Self {
        let builder = self
            .with_file_tools()
            .with_note_tool()
            .with_search_tools()
            .with_think_tool()
            .with_web_tools()
            .with_patch_tools()
            .with_git_tools();

        if allow_shell {
            builder.with_shell_tools()
        } else {
            builder
        }
    }

    /// Include the todo tool with a shared `TodoList`.
    #[must_use]
    pub fn with_todo_tool(self, todo_list: super::todo::SharedTodoList) -> Self {
        use super::todo::{TodoAddTool, TodoListTool, TodoUpdateTool, TodoWriteTool};
        self.with_tool(Arc::new(TodoWriteTool::new(todo_list.clone())))
            .with_tool(Arc::new(TodoAddTool::new(todo_list.clone())))
            .with_tool(Arc::new(TodoUpdateTool::new(todo_list.clone())))
            .with_tool(Arc::new(TodoListTool::new(todo_list)))
    }

    /// Include the plan tool with a shared `PlanState`.
    #[must_use]
    pub fn with_plan_tool(self, plan_state: super::plan::SharedPlanState) -> Self {
        use super::plan::UpdatePlanTool;
        self.with_tool(Arc::new(UpdatePlanTool::new(plan_state)))
    }

    /// Include all agent tools plus todo and plan tools.
    #[must_use]
    pub fn with_full_agent_tools(
        self,
        allow_shell: bool,
        todo_list: super::todo::SharedTodoList,
        plan_state: super::plan::SharedPlanState,
    ) -> Self {
        self.with_agent_tools(allow_shell)
            .with_todo_tool(todo_list)
            .with_plan_tool(plan_state)
            .with_artifact_tools()
            .with_execution_tools()
            .with_runtime_automation_tools()
            .with_minimax_tools()
    }

    /// Include runtime automation and utility tools used by server/agent flows.
    #[must_use]
    pub fn with_runtime_automation_tools(self) -> Self {
        use super::calculator::CalculatorTool;
        use super::finance::FinanceTool;
        use super::parallel::MultiToolUseParallelTool;
        use super::sports::SportsTool;
        use super::time::TimeTool;
        use super::user_input::RequestUserInputTool;
        use super::weather::WeatherTool;
        use super::web_run::WebRunTool;

        self.with_tool(Arc::new(WebRunTool))
            .with_tool(Arc::new(MultiToolUseParallelTool))
            .with_tool(Arc::new(RequestUserInputTool))
            .with_tool(Arc::new(WeatherTool))
            .with_tool(Arc::new(FinanceTool))
            .with_tool(Arc::new(SportsTool))
            .with_tool(Arc::new(TimeTool))
            .with_tool(Arc::new(CalculatorTool))
    }

    /// Include RLM tools for context execution and sub-queries.
    #[must_use]
    pub fn with_rlm_tools(
        self,
        session: SharedRlmSession,
        client: Option<MiniMaxTextClient>,
        model: String,
    ) -> Self {
        self.with_tool(Arc::new(super::rlm::RlmExecTool::new(session.clone())))
            .with_tool(Arc::new(super::rlm::RlmLoadTool::new(session.clone())))
            .with_tool(Arc::new(super::rlm::RlmStatusTool::new(session.clone())))
            .with_tool(Arc::new(super::rlm::RlmQueryTool::new(
                session, client, model,
            )))
    }

    /// Include Duo tools for dialectical autocoding.
    #[must_use]
    pub fn with_duo_tools(self, session: SharedDuoSession) -> Self {
        use super::duo::{DuoAdvanceTool, DuoCoachTool, DuoInitTool, DuoPlayerTool, DuoStatusTool};
        self.with_tool(Arc::new(DuoInitTool::new(session.clone())))
            .with_tool(Arc::new(DuoPlayerTool::new(session.clone())))
            .with_tool(Arc::new(DuoCoachTool::new(session.clone())))
            .with_tool(Arc::new(DuoAdvanceTool::new(session.clone())))
            .with_tool(Arc::new(DuoStatusTool::new(session)))
    }

    /// Include sub-agent management tools.
    #[must_use]
    pub fn with_subagent_tools(
        self,
        manager: super::subagent::SharedSubAgentManager,
        runtime: super::subagent::SubAgentRuntime,
    ) -> Self {
        use super::subagent::{AgentCancelTool, AgentListTool, AgentResultTool, AgentSpawnTool};

        self.with_tool(Arc::new(AgentSpawnTool::new(manager.clone(), runtime)))
            .with_tool(Arc::new(AgentResultTool::new(manager.clone())))
            .with_tool(Arc::new(AgentCancelTool::new(manager.clone())))
            .with_tool(Arc::new(AgentListTool::new(manager)))
    }

    /// Include `MiniMax` tools (tts, `tts_async_create`, `tts_async_query`, `analyze_image`,
    /// `generate_image`, `generate_video`, `generate_music`, `upload_file`, `list_files`, `retrieve_file`,
    /// `download_file`, `delete_file`, `voice_clone`, `voice_list`, `voice_delete`, `voice_design`,
    /// `query_video`, `generate_video_template`, `query_video_template`).
    #[must_use]
    pub fn with_minimax_tools(self) -> Self {
        use super::minimax::{
            AnalyzeImageTool, DeleteFileTool, DownloadFileTool, GenerateImageTool,
            GenerateMusicTool, GenerateVideoTool, ListFilesTool, QueryVideoTool, RetrieveFileTool,
            TtsAsyncCreateTool, TtsAsyncQueryTool, TtsTool, UploadFileTool,
            VideoTemplateCreateTool, VideoTemplateQueryTool, VoiceCloneTool, VoiceDeleteTool,
            VoiceDesignTool, VoiceListTool,
        };
        self.with_tool(Arc::new(TtsTool))
            .with_tool(Arc::new(TtsAsyncCreateTool))
            .with_tool(Arc::new(TtsAsyncQueryTool))
            .with_tool(Arc::new(AnalyzeImageTool))
            .with_tool(Arc::new(GenerateImageTool))
            .with_tool(Arc::new(GenerateVideoTool))
            .with_tool(Arc::new(QueryVideoTool))
            .with_tool(Arc::new(GenerateMusicTool))
            .with_tool(Arc::new(UploadFileTool))
            .with_tool(Arc::new(ListFilesTool))
            .with_tool(Arc::new(RetrieveFileTool))
            .with_tool(Arc::new(DownloadFileTool))
            .with_tool(Arc::new(DeleteFileTool))
            .with_tool(Arc::new(VoiceCloneTool))
            .with_tool(Arc::new(VoiceListTool))
            .with_tool(Arc::new(VoiceDeleteTool))
            .with_tool(Arc::new(VoiceDesignTool))
            .with_tool(Arc::new(VideoTemplateCreateTool))
            .with_tool(Arc::new(VideoTemplateQueryTool))
    }

    /// Build the registry with the given context.
    #[must_use]
    pub fn build(self, context: ToolContext) -> ToolRegistry {
        let mut registry = ToolRegistry::new(context);
        registry.register_all(self.tools);
        registry
    }
}

impl Default for ToolRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// === Unit Tests ===

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Value, json};
    use tempfile::tempdir;

    use crate::tools::ToolRegistryBuilder;
    use crate::tools::spec::{
        ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, required_str,
    };

    use super::ToolRegistry;

    /// A simple test tool for unit testing
    struct TestTool {
        name: String,
        description: String,
    }

    #[async_trait::async_trait]
    impl ToolSpec for TestTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn input_schema(&self) -> Value {
            json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                },
                "required": ["message"]
            })
        }

        fn capabilities(&self) -> Vec<ToolCapability> {
            vec![ToolCapability::ReadOnly]
        }

        async fn execute(
            &self,
            input: Value,
            _context: &ToolContext,
        ) -> Result<ToolResult, ToolError> {
            let message = required_str(&input, "message")?;
            Ok(ToolResult::success(format!("Echo: {message}")))
        }
    }

    fn make_test_tool(name: &str) -> Arc<TestTool> {
        Arc::new(TestTool {
            name: name.to_string(),
            description: "A test tool".to_string(),
        })
    }

    #[test]
    fn test_registry_register_and_get() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let mut registry = ToolRegistry::new(ctx);

        let tool = make_test_tool("test_tool");
        registry.register(tool);

        assert!(registry.contains("test_tool"));
        assert!(!registry.contains("nonexistent"));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_registry_names() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let mut registry = ToolRegistry::new(ctx);

        registry.register(make_test_tool("tool_a"));
        registry.register(make_test_tool("tool_b"));

        let names = registry.names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"tool_a"));
        assert!(names.contains(&"tool_b"));
    }

    #[test]
    fn test_registry_to_api_tools() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let mut registry = ToolRegistry::new(ctx);

        registry.register(make_test_tool("my_tool"));

        let api_tools = registry.to_api_tools();
        assert_eq!(api_tools.len(), 1);
        assert_eq!(api_tools[0].name, "my_tool");
        assert_eq!(api_tools[0].description, "A test tool");
    }

    #[test]
    fn test_registry_remove() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let mut registry = ToolRegistry::new(ctx);

        registry.register(make_test_tool("removable"));
        assert!(registry.contains("removable"));

        let _ = registry.remove("removable");
        assert!(!registry.contains("removable"));
    }

    #[test]
    fn test_registry_clear() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let mut registry = ToolRegistry::new(ctx);

        registry.register(make_test_tool("tool1"));
        registry.register(make_test_tool("tool2"));
        assert_eq!(registry.len(), 2);

        registry.clear();
        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn test_registry_execute() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let mut registry = ToolRegistry::new(ctx);

        registry.register(make_test_tool("echo"));

        let result = registry
            .execute("echo", json!({"message": "hello"}))
            .await
            .expect("execute");

        assert_eq!(result, "Echo: hello");
    }

    #[tokio::test]
    async fn test_registry_execute_unknown_tool() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let registry = ToolRegistry::new(ctx);

        let result = registry.execute("nonexistent", json!({})).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_builder_basic() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let registry = ToolRegistryBuilder::new()
            .with_tool(make_test_tool("custom"))
            .build(ctx);

        assert!(registry.contains("custom"));
    }

    #[test]
    fn test_filter_by_capability() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let mut registry = ToolRegistry::new(ctx);

        registry.register(make_test_tool("readonly_tool"));

        let readonly = registry.filter_by_capability(ToolCapability::ReadOnly);
        assert_eq!(readonly.len(), 1);

        let writes = registry.filter_by_capability(ToolCapability::WritesFiles);
        assert_eq!(writes.len(), 0);
    }

    #[test]
    fn test_read_only_tools() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let mut registry = ToolRegistry::new(ctx);

        registry.register(make_test_tool("reader"));

        let readonly = registry.read_only_tools();
        assert_eq!(readonly.len(), 1);
        assert_eq!(readonly[0].name(), "reader");
    }
}
