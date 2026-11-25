//! 宿主确认固定实际命令登记；确认后的调用仍走原schema、gesture、容量及取消门。
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandScope {
    pub plugin_id: String,
    pub command_id: String,
    pub package_digest: String,
    pub scope_id: String,
    pub requires_user_gesture: bool,
}
impl CommandScope {
    pub(super) fn capture(active: &ActivePackage, command_id: &str) -> HostResult<Self> {
        let command = active
            .effective_contributions
            .commands
            .iter()
            .find(|command| command.id == command_id)
            .ok_or_else(|| CapabilityHostError::NotFound(command_id.to_owned()))?;
        Ok(Self {
            plugin_id: active.package.manifest.qualified_id(),
            command_id: command_id.to_owned(),
            package_digest: active.package.digest.clone(),
            scope_id: active.scope_id.clone(),
            requires_user_gesture: command.requires_user_gesture,
        })
    }
}
impl CapabilityRuntimeHost {
    pub fn command_scope(&self, command_id: &str) -> HostResult<CommandScope> {
        let commands = lock(&self.commands)?;
        let plugin = commands
            .get(command_id)
            .ok_or_else(|| CapabilityHostError::NotFound(command_id.to_owned()))?;
        let packages = lock(&self.packages)?;
        let active = packages
            .get(plugin)
            .ok_or_else(|| CapabilityHostError::NotFound(plugin.clone()))?;
        let active = lock(active)?;
        CommandScope::capture(&active, command_id)
    }

    /// 只能由可信宿主在用户批准后调用。没有外发token，也不接受脚本布尔值作为批准。
    /// 登记解析是准入线性化点：此前换代拒绝；此后换代由调用owner取消/拒收迟到结果。
    pub fn invoke_confirmed_controlled(
        &self,
        invocation: CapabilityInvocation,
        control: Arc<crate::InvocationControl>,
        confirmed: &CommandScope,
    ) -> HostResult<CapabilityInvocationOutput> {
        let _guard = control.begin()?;
        let result = self.invoke_inner(invocation, Some(&control), Some(confirmed));
        control.complete()?;
        result
    }

    pub(super) fn invocation_package(
        &self,
        command: &str,
        confirmed: Option<&CommandScope>,
    ) -> HostResult<(String, Arc<Mutex<ActivePackage>>)> {
        let commands = lock(&self.commands)?;
        let plugin = commands
            .get(command)
            .ok_or_else(|| CapabilityHostError::NotFound(command.to_owned()))?;
        let packages = lock(&self.packages)?;
        let active = packages
            .get(plugin)
            .ok_or_else(|| CapabilityHostError::Unavailable(plugin.clone()))?;
        if let Some(expected) = confirmed {
            if &CommandScope::capture(&*lock(active)?, command)? != expected {
                return Err(CapabilityHostError::Protocol(
                    "confirmed command changed".into(),
                ));
            }
        }
        Ok((plugin.clone(), active.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_gesture_insertion_after_replacement_cannot_authorize_the_new_scope() {
        let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
        let old = CommandScope {
            plugin_id: "publisher/plugin".into(),
            command_id: "publisher/plugin.run".into(),
            package_digest: "a".repeat(64),
            scope_id: "scope:old".into(),
            requires_user_gesture: true,
        };
        // 精确模拟旧登记已读取、replacement已清理、旧issue才插入的交错，无sleep竞态。
        let token = host.issue_scoped_gesture(&old, None).unwrap();
        let call = CapabilityInvocation {
            request_id: "request".into(),
            command_id: old.command_id,
            input: Value::Null,
            target: None,
            resource_refs: vec![],
            unit_attachments: vec![],
            staged_resources: vec![],
            user_gesture_token: Some(token),
            timeout: None,
        };
        assert!(host
            .consume_user_gesture(&old.plugin_id, "scope:new", &call)
            .is_err());
        assert!(host
            .consume_user_gesture(&old.plugin_id, "scope:old", &call)
            .is_err());
    }
}
