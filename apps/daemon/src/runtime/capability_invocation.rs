// Workflow error mapping and Brain and Tea capability invocation.
fn workflow_runtime_error_response(error: WorkflowRuntimeError) -> Result<(u16, String)> {
    structured_error(
        500,
        json!({
            "code": "workflow_runtime_error",
            "message": error.to_string(),
        }),
    )
}

fn workflow_store_error_response(error: WorkflowStoreError) -> Result<(u16, String)> {
    match error {
        WorkflowStoreError::InvalidWorkflowId(id) => structured_error(
            400,
            json!({
                "code": "invalid_workflow_id",
                "message": format!("invalid workflow id `{id}`"),
                "workflow_id": id,
            }),
        ),
        WorkflowStoreError::InvalidWorkflowYaml(message) => structured_error(
            400,
            json!({
                "code": "invalid_workflow",
                "message": message,
            }),
        ),
        WorkflowStoreError::InvalidWorkflowGraph(message) => structured_error(
            400,
            json!({
                "code": "invalid_workflow_graph",
                "message": message,
            }),
        ),
        WorkflowStoreError::NotFound(id) => structured_error(
            404,
            json!({
                "code": "workflow_not_found",
                "message": format!("workflow `{id}` was not found"),
                "workflow_id": id,
            }),
        ),
        WorkflowStoreError::Io(error) => structured_error(
            500,
            json!({
                "code": "workflow_store_error",
                "message": error.to_string(),
            }),
        ),
        WorkflowStoreError::Json(error) => structured_error(
            500,
            json!({
                "code": "workflow_store_error",
                "message": error.to_string(),
            }),
        ),
        WorkflowStoreError::Yaml(error) => structured_error(
            500,
            json!({
                "code": "workflow_store_error",
                "message": error.to_string(),
            }),
        ),
    }
}

fn invoke_capability(
    body: &str,
    run_store: &SharedRunStore,
    brain_planner: &SharedBrainPlanner,
) -> Result<(u16, String)> {
    let Ok(request) = serde_json::from_str::<InvokeCapabilityRequest>(body) else {
        return bad_request("invalid invoke request");
    };
    if request.request_id.trim().is_empty() {
        return bad_request("invalid invoke request: requestId is required");
    }
    if request.caller.trim().is_empty() {
        return invoke_error(
            400,
            Some(&request.request_id),
            "invalid_request",
            "caller is required",
            json!({}),
        );
    }
    match request.capability.as_str() {
        CAPABILITY_BRAIN_PLAN => invoke_brain_plan(request, run_store, brain_planner),
        CAPABILITY_TEA_TICKET_DECOMPOSE => invoke_tea_ticket_decompose(request, run_store),
        _ => invoke_error(
            404,
            Some(&request.request_id),
            "unknown_capability",
            &format!("unknown capability `{}`", request.capability),
            json!({
                "capability": request.capability,
            }),
        ),
    }
}

fn invoke_brain_plan(
    request: InvokeCapabilityRequest,
    run_store: &SharedRunStore,
    brain_planner: &SharedBrainPlanner,
) -> Result<(u16, String)> {
    let InvokeCapabilityRequest {
        request_id, input, ..
    } = request;
    let run_id = loom_core::RunId::new().to_string();
    let goal = input
        .get("goal")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(goal) = goal else {
        return invoke_error(
            400,
            Some(&request_id),
            "invalid_input",
            "brain.plan input.goal is required",
            json!({
                "capability": CAPABILITY_BRAIN_PLAN,
            }),
        );
    };

    let constraints = input
        .get("constraints")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let context = input.get("context").cloned();
    let session_id = loom_core::SessionId::new().to_string();
    let mut run = json!({
        "id": run_id,
        "capability": CAPABILITY_BRAIN_PLAN,
        "loom_session_id": session_id,
        "status": "running",
        "input": input,
    });

    let started = match RunEventDraft::new(
        "run_started",
        json!({
            "capability": CAPABILITY_BRAIN_PLAN,
            "status": "running",
        }),
    ) {
        Ok(event) => event,
        Err(error) => return run_store_failed(error),
    };
    {
        let mut store = match lock_run_store(run_store) {
            Ok(store) => store,
            Err(error) => return run_store_failed(error),
        };
        if let Err(error) = store.insert_run(run.clone(), vec![started]) {
            return run_store_failed(error);
        }
    }

    let planning = brain_planner.plan(BrainPlanRequest {
        goal: goal.to_owned(),
        constraints,
        context,
    });
    match planning {
        Ok(result) => {
            let planner = json!({
                "source": result.source.as_str(),
                "model": result.model,
            });
            let output = json!({
                "summary": result.summary,
                "steps": result.steps,
                "planner": planner,
            });
            run["status"] = json!("succeeded");
            run["output"] = output.clone();

            let completed = match RunEventDraft::new(
                "capability_completed",
                json!({
                    "capability": CAPABILITY_BRAIN_PLAN,
                    "status": "succeeded",
                    "planner": planner,
                }),
            ) {
                Ok(event) => event,
                Err(error) => return run_store_failed(error),
            };
            let mut store = match lock_run_store(run_store) {
                Ok(store) => store,
                Err(error) => return run_store_failed(error),
            };
            if let Err(error) = store.transition_run(run.clone(), completed) {
                return run_store_failed(error);
            }
            Ok((
                200,
                serde_json::to_string(&json!({
                    "requestId": request_id,
                    "status": "succeeded",
                    "output": {
                        "runId": run_id,
                        "run": run,
                        "summary": output["summary"].clone(),
                        "steps": output["steps"].clone(),
                        "planner": output["planner"].clone(),
                    }
                }))?,
            ))
        }
        Err(error) => {
            let planner_status: BrainPlannerStatus = brain_planner.status();
            let planner = json!({
                "source": planner_status.mode,
                "model": planner_status.model,
            });
            let run_error = json!({
                "code": "gateway_planner_failed",
                "message": "Gateway-backed planning failed",
                "diagnostic": truncate_diagnostic(error.to_string(), 512),
            });
            run["status"] = json!("failed");
            run["planner"] = planner.clone();
            run["error"] = run_error.clone();

            let failed = match RunEventDraft::new(
                "capability_failed",
                json!({
                    "capability": CAPABILITY_BRAIN_PLAN,
                    "status": "failed",
                    "planner": planner,
                    "error": {
                        "code": "gateway_planner_failed",
                    },
                }),
            ) {
                Ok(event) => event,
                Err(error) => return run_store_failed(error),
            };
            let mut store = match lock_run_store(run_store) {
                Ok(store) => store,
                Err(error) => return run_store_failed(error),
            };
            if let Err(error) = store.transition_run(run, failed) {
                return run_store_failed(error);
            }
            invoke_error(
                502,
                Some(&request_id),
                "gateway_planner_failed",
                "Gateway-backed planning failed",
                json!({
                    "capability": CAPABILITY_BRAIN_PLAN,
                    "runId": run_id,
                }),
            )
        }
    }
}

fn invoke_tea_ticket_decompose(
    request: InvokeCapabilityRequest,
    run_store: &SharedRunStore,
) -> Result<(u16, String)> {
    let Some(ticket) = request.input.get("ticket").and_then(Value::as_object) else {
        return invoke_error(
            400,
            Some(&request.request_id),
            "invalid_input",
            "tea.ticket.decompose.v1 input.ticket is required",
            json!({
                "capability": CAPABILITY_TEA_TICKET_DECOMPOSE,
            }),
        );
    };
    let ticket_title = ticket
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Tea ticket");
    let ticket_description = ticket
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let risk = if ticket_description.len() > 240 {
        "high"
    } else if ticket_description.len() < 12 {
        "medium"
    } else {
        "medium"
    };
    let proposal_id = loom_core::RunId::new().to_string();
    let proposal = json!({
        "schema_version": 1,
        "proposal_id": proposal_id,
        "analysis": {
            "intent": "engineering_work_order",
            "target_components": ["Tea"],
            "target_paths": [],
            "constraints": [
                "Loom must not mutate Tea ticket state directly",
                "Tea validates, stores, and governs decomposition records"
            ],
            "acceptance_criteria": [
                "analysis and plan are returned as a proposal",
                "Tea commits accepted records into its own timeline",
                "verification evidence is attached before human acceptance"
            ],
            "missing_context": if ticket_description.is_empty() {
                json!(["ticket description is empty"])
            } else {
                json!([])
            },
            "risk_assessment": risk,
            "confidence": 0.82,
            "recommended_policy": "human_before_execute",
            "recommended_workflow": "loom.tea_ticket_decompose.v1"
        },
        "plan": {
            "summary": format!("Decompose Tea work order: {ticket_title}"),
            "steps": [
                {
                    "id": "inspect-context",
                    "title": "Inspect context",
                    "description": "Read the Tea ticket snapshot, comments, policy, and available workspace context."
                },
                {
                    "id": "propose-plan",
                    "title": "Propose plan",
                    "description": "Generate a bounded plan that Tea can validate and store before execution."
                },
                {
                    "id": "validate",
                    "title": "Validate",
                    "description": "Define concrete verification commands and evidence requirements for human review."
                }
            ],
            "required_tools": ["loom.run"],
            "expected_artifacts": ["Tea analysis record", "Tea plan record", "Loom run evidence"],
            "validation_strategy": ["Tea stores the returned analysis and plan", "Loom does not write Tea state directly"],
            "rollback_strategy": ["leave the Tea ticket blocked with proposal evidence"],
            "requires_approval_before_execute": true
        },
        "requires_human_review": true,
        "notes": []
    });
    let run_id = loom_core::RunId::new().to_string();
    let output = json!({
        "proposal": proposal,
        "summary": format!("Tea decomposition proposal prepared for {ticket_title}")
    });
    let run = json!({
        "id": run_id,
        "capability": CAPABILITY_TEA_TICKET_DECOMPOSE,
        "loom_session_id": loom_core::SessionId::new().to_string(),
        "status": "succeeded",
        "input": request.input,
        "output": output.clone()
    });

    let events = match (
        RunEventDraft::new(
            "run_started",
            json!({
                "capability": CAPABILITY_TEA_TICKET_DECOMPOSE,
                "status": "running",
            }),
        ),
        RunEventDraft::new(
            "capability_completed",
            json!({
                "capability": CAPABILITY_TEA_TICKET_DECOMPOSE,
                "status": "succeeded",
            }),
        ),
    ) {
        (Ok(started), Ok(completed)) => vec![started, completed],
        (Err(error), _) | (_, Err(error)) => return run_store_failed(error),
    };
    let mut store = match lock_run_store(run_store) {
        Ok(store) => store,
        Err(error) => return run_store_failed(error),
    };
    if let Err(error) = store.insert_run(run.clone(), events) {
        return run_store_failed(error);
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "requestId": request.request_id,
            "status": "succeeded",
            "output": {
                "runId": run_id,
                "run": run,
                "proposal": output["proposal"].clone(),
                "summary": output["summary"].clone(),
            }
        }))?,
    ))
}
