use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use loom_agent::{AgentCatalog, AgentSpec};
use loom_core::{ActorId, RunStatus, SessionId};
use loom_durable::InMemoryEventStore;
use loom_workflow::{StepOutcome, WorkflowAction, WorkflowExecutor, WorkflowGraph};

pub fn run_cli_with_writer<I, S, W>(args: I, writer: &mut W) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    W: Write,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_owned())
        .collect();
    if has_flag(&args, &["--help", "-h"]) {
        writer.write_all(cli_help_text().as_bytes())?;
        return Ok(());
    }
    if has_flag(&args, &["--version", "-V"]) {
        writeln!(writer, "loom {}", loom_core::LOOM_VERSION)?;
        return Ok(());
    }

    let options = CliOptions::parse(&args)?;

    let command: Vec<&str> = options.command.iter().map(String::as_str).collect();
    match command.as_slice() {
        ["status"] => {
            let response = http_get(
                &options.daemon_url,
                "/status",
                options.daemon_token.as_deref(),
            )?;
            writeln!(writer, "{response}")?;
        }
        ["agents", "list"] => {
            let catalog = load_agent_catalog(&options.examples_dir)?;
            for agent in catalog.effective_agents() {
                writeln!(writer, "{}\t{}", agent.id, agent.name)?;
            }
        }
        ["workflows", "list"] => {
            for graph in load_workflows(&options.examples_dir)? {
                writeln!(writer, "{}", graph.id.as_str())?;
            }
        }
        ["run", workflow_id] => {
            let graph = load_workflows(&options.examples_dir)?
                .into_iter()
                .find(|graph| graph.id.as_str() == *workflow_id)
                .ok_or_else(|| anyhow!("workflow `{workflow_id}` was not found"))?;
            let summary = run_workflow(&graph)?;
            writeln!(
                writer,
                "{}\t{}",
                match summary.status {
                    RunStatus::Succeeded => "succeeded",
                    RunStatus::Failed => "failed",
                    RunStatus::Pending => "pending",
                    RunStatus::Running => "running",
                    RunStatus::Cancelled => "cancelled",
                },
                summary.completed_nodes.join(",")
            )?;
        }
        [] => bail!("missing command"),
        command => bail!("unsupported command `{}`", command.join(" ")),
    }

    Ok(())
}

fn has_flag(args: &[String], flags: &[&str]) -> bool {
    args.iter().skip(1).any(|arg| flags.contains(&arg.as_str()))
}

fn cli_help_text() -> &'static str {
    concat!(
        "Usage: loom <COMMAND> [OPTIONS]\n",
        "\n",
        "Commands:\n",
        "  status                 Read loom-daemon status\n",
        "  agents list            List example agents\n",
        "  workflows list         List example workflows\n",
        "  run <workflow-id>      Run an example workflow locally\n",
        "\n",
        "Options:\n",
        "  --daemon-url <URL>     Daemon URL for status [default: http://127.0.0.1:8765]\n",
        "  --daemon-token <TOKEN> Bearer token for status [default: LOOM_DAEMON_TOKEN]\n",
        "  --examples-dir <DIR>   Examples directory [default: examples]\n",
        "  -h, --help             Print help\n",
        "  -V, --version          Print version\n",
    )
}

#[derive(Debug)]
struct CliOptions {
    command: Vec<String>,
    daemon_url: String,
    daemon_token: Option<String>,
    examples_dir: PathBuf,
}

impl CliOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut command = Vec::new();
        let mut daemon_url = "http://127.0.0.1:8765".to_owned();
        let mut daemon_token = std::env::var("LOOM_DAEMON_TOKEN")
            .ok()
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty());
        let mut examples_dir = PathBuf::from("examples");
        let mut index = 1;

        while index < args.len() {
            match args[index].as_str() {
                "--daemon-url" => {
                    index += 1;
                    daemon_url = args
                        .get(index)
                        .ok_or_else(|| anyhow!("--daemon-url requires a value"))?
                        .clone();
                }
                "--daemon-token" => {
                    index += 1;
                    daemon_token = Some(
                        args.get(index)
                            .ok_or_else(|| anyhow!("--daemon-token requires a value"))?
                            .trim()
                            .to_owned(),
                    )
                    .filter(|token| !token.is_empty());
                }
                "--examples-dir" => {
                    index += 1;
                    examples_dir = PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| anyhow!("--examples-dir requires a value"))?,
                    );
                }
                value => command.push(value.to_owned()),
            }
            index += 1;
        }

        Ok(Self {
            command,
            daemon_url,
            daemon_token,
            examples_dir,
        })
    }
}

fn http_get(base_url: &str, path: &str, bearer_token: Option<&str>) -> Result<String> {
    let endpoint = parse_http_url(base_url)?;
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .with_context(|| format!("connect daemon at {base_url}"))?;
    let auth_header = bearer_token
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {}:{}\r\n{auth_header}Connection: close\r\n\r\n",
        endpoint.host, endpoint.port
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow!("malformed daemon response"))?;
    if !head.starts_with("HTTP/1.1 200") {
        bail!("daemon returned non-200 response: {head}");
    }
    Ok(body.to_owned())
}

struct HttpEndpoint {
    host: String,
    port: u16,
}

fn parse_http_url(base_url: &str) -> Result<HttpEndpoint> {
    let authority = base_url
        .strip_prefix("http://")
        .ok_or_else(|| anyhow!("daemon URL must use http://"))?
        .split('/')
        .next()
        .ok_or_else(|| anyhow!("daemon URL is missing authority"))?;
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host.to_owned(), port.parse::<u16>()?),
        None => (authority.to_owned(), 80),
    };
    Ok(HttpEndpoint { host, port })
}

fn load_agent_catalog(examples_dir: &Path) -> Result<AgentCatalog> {
    let mut catalog = AgentCatalog::default();
    let agents_dir = examples_dir.join("agents");
    if !agents_dir.exists() {
        return Ok(catalog);
    }

    for entry in fs::read_dir(&agents_dir).with_context(|| format!("read {agents_dir:?}"))? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        let markdown = fs::read_to_string(&path).with_context(|| format!("read {path:?}"))?;
        let spec =
            AgentSpec::from_markdown(&markdown).with_context(|| format!("parse {path:?}"))?;
        catalog.add(spec).with_context(|| format!("add {path:?}"))?;
    }
    Ok(catalog)
}

fn load_workflows(examples_dir: &Path) -> Result<Vec<WorkflowGraph>> {
    let workflows_dir = examples_dir.join("workflows");
    if !workflows_dir.exists() {
        return Ok(Vec::new());
    }

    let mut workflows = Vec::new();
    for entry in fs::read_dir(&workflows_dir).with_context(|| format!("read {workflows_dir:?}"))? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("yaml") {
            continue;
        }
        let yaml = fs::read_to_string(&path).with_context(|| format!("read {path:?}"))?;
        let graph: WorkflowGraph =
            serde_yaml::from_str(&yaml).with_context(|| format!("parse {path:?}"))?;
        graph
            .validate()
            .with_context(|| format!("validate {path:?}"))?;
        workflows.push(graph);
    }
    workflows.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(workflows)
}

fn run_workflow(graph: &WorkflowGraph) -> Result<loom_workflow::WorkflowRunSummary> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("create CLI workflow runtime")?;
    runtime.block_on(async {
        let store = InMemoryEventStore::default();
        let mut executor = WorkflowExecutor::new(&store);
        for actor_id in workflow_actor_ids(graph) {
            executor = executor.with_actor(actor_id, StepOutcome::succeed("sample complete"));
        }
        executor
            .run(SessionId::new(), graph)
            .await
            .context("execute workflow")
    })
}

fn workflow_actor_ids(graph: &WorkflowGraph) -> Vec<ActorId> {
    let mut actor_ids = Vec::new();
    for node in graph.nodes.values() {
        let WorkflowAction::Agent { actor_id } = &node.action;
        if !actor_ids.contains(actor_id) {
            actor_ids.push(actor_id.clone());
        }
    }
    actor_ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn help_and_version_do_not_require_a_daemon_or_examples() {
        let mut help = Vec::new();
        run_cli_with_writer(["loom", "--help"], &mut help).expect("help command");
        let help = String::from_utf8(help).expect("help utf8");
        assert!(help.contains("Usage: loom"));
        assert!(help.contains("agents list"));
        assert!(help.contains("workflows list"));
        assert!(help.contains("run <workflow-id>"));

        let mut version = Vec::new();
        run_cli_with_writer(["loom", "--version"], &mut version).expect("version command");
        let version = String::from_utf8(version).expect("version utf8");
        assert_eq!(version.trim(), format!("loom {}", loom_core::LOOM_VERSION));
    }

    #[test]
    fn status_command_reads_daemon_status_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon");
        let address = listener.local_addr().expect("mock address");
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept status request");
            let request = read_http_request(&mut socket);
            assert!(
                request.starts_with("GET /status HTTP/1.1"),
                "unexpected request: {request:?}"
            );
            let body = r#"{"status":"ready","modules":[{"name":"core","initialized":true}]}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write response");
        });

        let mut output = Vec::new();
        run_cli_with_writer(
            [
                "loom",
                "status",
                "--daemon-url",
                &format!("http://{address}"),
            ],
            &mut output,
        )
        .expect("status command");

        server.join().expect("mock server");
        let output = String::from_utf8(output).expect("utf8 output");
        assert!(output.contains("ready"));
        assert!(output.contains("core"));
    }

    #[test]
    fn status_command_sends_bearer_token_when_configured() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon");
        let address = listener.local_addr().expect("mock address");
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept status request");
            let request = read_http_request(&mut socket);
            assert!(
                request.contains("Authorization: Bearer test-token\r\n"),
                "missing authorization header: {request:?}"
            );
            let body = r#"{"status":"ready","modules":[]}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write response");
        });

        let mut output = Vec::new();
        run_cli_with_writer(
            [
                "loom",
                "status",
                "--daemon-url",
                &format!("http://{address}"),
                "--daemon-token",
                "test-token",
            ],
            &mut output,
        )
        .expect("status command");

        server.join().expect("mock server");
        let output = String::from_utf8(output).expect("utf8 output");
        assert!(output.contains("ready"));
    }

    #[test]
    fn agents_and_workflows_list_and_run_examples() {
        let examples = temp_examples_dir();
        fs::create_dir_all(examples.join("agents")).expect("agents dir");
        fs::create_dir_all(examples.join("workflows")).expect("workflows dir");
        fs::write(
            examples.join("agents").join("planner.md"),
            r#"---
id: planner
name: Planner
scope: project
tools:
  allow: []
  deny: []
---
Plan the work.
"#,
        )
        .expect("write agent");
        fs::write(
            examples.join("workflows").join("three-node-success.yaml"),
            r#"id: sample.three_node
entry_node: start
nodes:
  start:
    id: start
    action:
      type: agent
      actor_id: planner
edges: []
"#,
        )
        .expect("write workflow");

        let examples_arg = examples.to_string_lossy().to_string();

        let mut agents_output = Vec::new();
        run_cli_with_writer(
            ["loom", "agents", "list", "--examples-dir", &examples_arg],
            &mut agents_output,
        )
        .expect("agents list");
        assert!(String::from_utf8(agents_output)
            .expect("agents utf8")
            .contains("planner"));

        let mut workflows_output = Vec::new();
        run_cli_with_writer(
            ["loom", "workflows", "list", "--examples-dir", &examples_arg],
            &mut workflows_output,
        )
        .expect("workflows list");
        assert!(String::from_utf8(workflows_output)
            .expect("workflows utf8")
            .contains("sample.three_node"));

        let mut run_output = Vec::new();
        run_cli_with_writer(
            [
                "loom",
                "run",
                "sample.three_node",
                "--examples-dir",
                &examples_arg,
            ],
            &mut run_output,
        )
        .expect("workflow run");
        let run_output = String::from_utf8(run_output).expect("run utf8");
        assert!(run_output.contains("succeeded"));
        assert!(run_output.contains("start"));

        fs::remove_dir_all(examples).expect("cleanup examples");
    }

    #[test]
    fn workflows_list_reads_repo_example_fixture() {
        let mut output = Vec::new();
        let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let examples_arg = examples.to_string_lossy().to_string();

        run_cli_with_writer(
            ["loom", "workflows", "list", "--examples-dir", &examples_arg],
            &mut output,
        )
        .expect("workflows list from repo examples");

        assert!(String::from_utf8(output)
            .expect("workflow output utf8")
            .contains("sample.three_node"));
    }

    fn temp_examples_dir() -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "loom-cli-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        path
    }

    fn read_http_request(socket: &mut impl Read) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 64];
        loop {
            let bytes = socket.read(&mut buffer).expect("read request");
            if bytes == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..bytes]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8(request).expect("utf8 request")
    }
}
