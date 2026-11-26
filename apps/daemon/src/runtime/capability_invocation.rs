// Core in-process capability handlers for Global AI Gateway relay and hub runtime probes.
fn invoke_ai_chat(
    request: InvokeCapabilityRequest,
    run_store: &SharedRunStore,
    ai_gateway: &SharedAiGateway,
) -> Result<(u16, String)> {
    let InvokeCapabilityRequest {
        request_id, input, ..
    } = request;
    let chat_payload = match build_ai_chat_payload(&input) {
        Ok(payload) => payload,
        Err(message) => {
            return invoke_error(
                400,
                Some(&request_id),
                "invalid_input",
                &message,
                json!({ "capability": CAPABILITY_AI_CHAT }),
            )
        }
    };

    let run_id = loom_core::RunId::new().to_string();
    let session_id = loom_core::SessionId::new().to_string();
    let mut run = json!({
        "id": run_id,
        "capability": CAPABILITY_AI_CHAT,
        "loom_session_id": session_id,
        "status": "running",
        "input": input,
    });

    let started = match RunEventDraft::new(
        "run_started",
        json!({
            "capability": CAPABILITY_AI_CHAT,
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

    match ai_gateway.chat_completion(chat_payload) {
        Ok(outcome) => {
            let gateway_meta = json!({
                "source": "gateway",
                "model": outcome.model,
                "requestId": outcome.request_id,
            });
            let output = json!({
                "content": outcome.content,
                "model": outcome.model,
                "gateway": gateway_meta,
                "response": outcome.response,
            });
            run["status"] = json!("succeeded");
            run["output"] = output.clone();

            let completed = match RunEventDraft::new(
                "capability_completed",
                json!({
                    "capability": CAPABILITY_AI_CHAT,
                    "status": "succeeded",
                    "gateway": gateway_meta,
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
                        "content": output["content"].clone(),
                        "model": output["model"].clone(),
                        "gateway": output["gateway"].clone(),
                        "response": output["response"].clone(),
                    }
                }))?,
            ))
        }
        Err(error) => {
            let status_snapshot = ai_gateway.status();
            let (http_status, code, public_message) = match &error {
                AiGatewayRelayError::NotConfigured => (
                    503,
                    "ai_gateway_not_configured",
                    "AI Gateway is not configured",
                ),
                AiGatewayRelayError::InvalidRequest(_) => (
                    400,
                    "invalid_input",
                    "AI Gateway rejected the chat input",
                ),
                _ => (502, "ai_gateway_failed", "AI Gateway chat relay failed"),
            };
            let gateway_meta = json!({
                "source": status_snapshot.mode,
                "model": status_snapshot.model,
            });
            let run_error = json!({
                "code": code,
                "message": public_message,
                "diagnostic": truncate_diagnostic(error.to_string(), 512),
            });
            run["status"] = json!("failed");
            run["gateway"] = gateway_meta.clone();
            run["error"] = run_error;

            let failed = match RunEventDraft::new(
                "capability_failed",
                json!({
                    "capability": CAPABILITY_AI_CHAT,
                    "status": "failed",
                    "gateway": gateway_meta,
                    "error": { "code": code },
                }),
            ) {
                Ok(event) => event,
                Err(store_err) => return run_store_failed(store_err),
            };
            let mut store = match lock_run_store(run_store) {
                Ok(store) => store,
                Err(store_err) => return run_store_failed(store_err),
            };
            if let Err(store_err) = store.transition_run(run, failed) {
                return run_store_failed(store_err);
            }
            invoke_error(
                http_status,
                Some(&request_id),
                code,
                public_message,
                json!({
                    "capability": CAPABILITY_AI_CHAT,
                    "runId": run_id,
                }),
            )
        }
    }
}

fn build_ai_chat_payload(input: &Value) -> std::result::Result<Value, String> {
    let Some(object) = input.as_object() else {
        return Err("ai.chat.v1 input must be a JSON object".to_owned());
    };
    let mut payload = serde_json::Map::new();
    if let Some(model) = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("model".to_owned(), json!(model));
    }
    if let Some(messages) = object.get("messages").and_then(Value::as_array) {
        if messages.is_empty() {
            return Err("ai.chat.v1 input.messages must not be empty".to_owned());
        }
        payload.insert("messages".to_owned(), Value::Array(messages.clone()));
    } else if let Some(prompt) = object
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let system = object
            .get("system")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("You are a helpful assistant connected through Loom.");
        payload.insert(
            "messages".to_owned(),
            json!([
                { "role": "system", "content": system },
                { "role": "user", "content": prompt }
            ]),
        );
    } else {
        return Err("ai.chat.v1 requires input.prompt or input.messages".to_owned());
    }
    if let Some(temperature) = object.get("temperature") {
        payload.insert("temperature".to_owned(), temperature.clone());
    }
    if let Some(response_format) = object.get("response_format") {
        payload.insert("response_format".to_owned(), response_format.clone());
    }
    Ok(Value::Object(payload))
}

fn invoke_runtime_probe(
    request: InvokeCapabilityRequest,
    run_store: &SharedRunStore,
) -> Result<(u16, String)> {
    let label = request
        .input
        .get("label")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(label) = label else {
        return invoke_error(
            400,
            Some(&request.request_id),
            "invalid_input",
            "loom.runtime.probe.v1 input.label is required",
            json!({ "capability": CAPABILITY_RUNTIME_PROBE }),
        );
    };
    let run_id = loom_core::RunId::new().to_string();
    let output = json!({
        "summary": format!("Loom runtime probe completed for {label}"),
        "label": label,
    });
    let run = json!({
        "id": run_id,
        "capability": CAPABILITY_RUNTIME_PROBE,
        "loom_session_id": loom_core::SessionId::new().to_string(),
        "status": "succeeded",
        "input": request.input,
        "output": output.clone(),
    });
    let events = match (
        RunEventDraft::new(
            "run_started",
            json!({
                "capability": CAPABILITY_RUNTIME_PROBE,
                "status": "running",
            }),
        ),
        RunEventDraft::new(
            "capability_completed",
            json!({
                "capability": CAPABILITY_RUNTIME_PROBE,
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
                "summary": output["summary"].clone(),
                "label": output["label"].clone(),
            }
        }))?,
    ))
}
