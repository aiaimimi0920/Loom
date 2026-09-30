// Unified ownership lookup for core and dynamically installed capability commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CoreCapabilityHandler {
    BrainPlan,
    TeaTicketDecompose,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CapabilityCommandOwner {
    Core(CoreCapabilityHandler),
    Plugin(String),
}

struct CapabilityDispatchRegistry {
    core_commands: HashMap<String, CoreCapabilityHandler>,
    plugin_runtime: SharedCapabilityRuntime,
}

impl CapabilityDispatchRegistry {
    fn with_core_commands(plugin_runtime: SharedCapabilityRuntime) -> Self {
        let mut registry = Self {
            core_commands: HashMap::new(),
            plugin_runtime,
        };
        registry.register_core(CAPABILITY_BRAIN_PLAN, CoreCapabilityHandler::BrainPlan);
        registry.register_core(
            CAPABILITY_TEA_TICKET_DECOMPOSE,
            CoreCapabilityHandler::TeaTicketDecompose,
        );
        registry
    }

    fn register_core(&mut self, command_id: &str, handler: CoreCapabilityHandler) {
        let previous = self.core_commands.insert(command_id.to_owned(), handler);
        debug_assert!(previous.is_none(), "duplicate core capability registration");
    }

    fn resolve(
        &self,
        command_id: &str,
    ) -> std::result::Result<Option<CapabilityCommandOwner>, loom_capability_runtime::CapabilityHostError>
    {
        if let Some(handler) = self.core_commands.get(command_id) {
            return Ok(Some(CapabilityCommandOwner::Core(*handler)));
        }
        Ok(self
            .plugin_runtime
            .command_owner(command_id)?
            .map(CapabilityCommandOwner::Plugin))
    }

    fn plugin_runtime(&self) -> &SharedCapabilityRuntime {
        &self.plugin_runtime
    }
}

type SharedCapabilityDispatchRegistry = Arc<CapabilityDispatchRegistry>;
