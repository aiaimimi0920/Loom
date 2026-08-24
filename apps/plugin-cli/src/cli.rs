// CLI argument dispatch, help text and embedded schema selection.
pub fn run<I, S, W>(args: I, writer: &mut W) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    W: Write,
{
    let args = args
        .into_iter()
        .map(|value| value.as_ref().to_owned())
        .collect::<Vec<_>>();
    if args.len() <= 1 || has_flag(&args, "--help") || has_flag(&args, "-h") {
        writer.write_all(help_text().as_bytes())?;
        return Ok(());
    }
    if has_flag(&args, "--version") || has_flag(&args, "-V") {
        writeln!(writer, "loom-plugin {}", env!("CARGO_PKG_VERSION"))?;
        return Ok(());
    }

    match args[1..]
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["validate", path] => {
            let report = validate_path_with_trust_store(Path::new(path), None)?;
            writeln!(writer, "{report}")?;
        }
        ["validate", path, "--trust-store", store] => {
            let report = validate_path_with_trust_store(Path::new(path), Some(Path::new(store)))?;
            writeln!(writer, "{report}")?;
        }
        ["pack", source, output] => {
            let report = pack_directory(Path::new(source), Path::new(output))?;
            writeln!(writer, "{report}")?;
        }
        ["schema", name] => {
            writer.write_all(schema(name)?.as_bytes())?;
            writer.write_all(b"\n")?;
        }
        ["keygen", path, key_id] => {
            let key = generate_signing_key(*key_id);
            write_signing_key_document(Path::new(path), &key)?;
            writeln!(writer, "generated Ed25519 key `{key_id}` at {path}")?;
        }
        ["sign", directory, key_path, publisher_id] => {
            let status =
                sign_plugin_package(Path::new(directory), Path::new(key_path), publisher_id)?;
            writeln!(writer, "{status}")?;
        }
        ["trust", "add", store_path, publisher_id, key_path] => {
            trust_publisher(Path::new(store_path), publisher_id, Path::new(key_path))?;
            writeln!(writer, "trusted publisher `{publisher_id}`")?;
        }
        ["trust", "revoke", store_path, publisher_id, key_id] => {
            revoke_publisher(Path::new(store_path), publisher_id, key_id)?;
            writeln!(writer, "revoked publisher `{publisher_id}` key `{key_id}`")?;
        }
        ["init", "framework", directory, id, publisher] => {
            init_framework(Path::new(directory), id, publisher)?;
            writeln!(
                writer,
                "initialized framework `{publisher}/{id}` at {directory}"
            )?;
        }
        ["init", "art", directory, id, framework, publisher] => {
            init_art(Path::new(directory), id, framework, publisher)?;
            writeln!(writer, "initialized Art `{publisher}/{id}` at {directory}")?;
        }
        ["conformance", executable, framework_id, art_dir] => {
            let report = run_conformance(Path::new(executable), framework_id, Path::new(art_dir))?;
            writeln!(writer, "{report}")?;
        }
        command => bail!(
            "unsupported command `{}`\n\n{}",
            command.join(" "),
            help_text()
        ),
    }
    Ok(())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().skip(1).any(|value| value == flag)
}

fn help_text() -> &'static str {
    concat!(
        "Usage: loom-plugin <COMMAND>\n",
        "\n",
        "Commands:\n",
        "  init framework <DIR> <ID> <PUBLISHER>  Create a framework package skeleton\n",
        "  init art <DIR> <ID> <FRAMEWORK> <PUBLISHER> Create an Art package skeleton\n",
        "  validate <PATH> [--trust-store <STORE>] Validate a package directory or manifest\n",
        "  pack <SOURCE_DIR> <OUTPUT_ZIP>          Validate and build a deterministic package ZIP\n",
        "  conformance <EXE> <FRAMEWORK> <ART_DIR> Run the v1 process contract against a runtime\n",
        "  schema <NAME>                           Print an embedded public JSON Schema\n",
        "  keygen <KEY_FILE> <KEY_ID>              Generate an Ed25519 signing key\n",
        "  sign <PACKAGE_DIR> <KEY_FILE> <PUBLISHER> Sign a framework or Art package\n",
        "  trust add <STORE> <PUBLISHER> <KEY_FILE> Trust a publisher key\n",
        "  trust revoke <STORE> <PUBLISHER> <KEY_ID> Revoke a publisher key\n",
        "\n",
        "Schema names: framework-manifest, execute-request, execute-response, authoring, art-runtime, surface-manifest, surface-message, surface-scene, surface-stream, device-session, hook-message\n",
    )
}

fn schema(name: &str) -> Result<&'static str> {
    match name {
        "framework-manifest" => Ok(schemas::FRAMEWORK_MANIFEST_V1),
        "execute-request" => Ok(schemas::FRAMEWORK_EXECUTE_REQUEST_V1),
        "execute-response" => Ok(schemas::FRAMEWORK_EXECUTE_RESPONSE_V1),
        "authoring" => Ok(schemas::FRAMEWORK_AUTHORING_V1),
        "art-runtime" => Ok(schemas::ART_RUNTIME_V1),
        "surface-manifest" => Ok(schemas::SURFACE_MANIFEST_V1),
        "surface-message" => Ok(schemas::SURFACE_MESSAGE_V1),
        "surface-scene" => Ok(schemas::SURFACE_SCENE_V1),
        "surface-stream" => Ok(schemas::SURFACE_STREAM_V1),
        "device-session" => Ok(schemas::DEVICE_SESSION_V1),
        "hook-message" => Ok(schemas::HOOK_MESSAGE_V1),
        _ => bail!("unknown schema `{name}`"),
    }
}
