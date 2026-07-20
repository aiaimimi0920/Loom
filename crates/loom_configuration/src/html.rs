use crate::{
    ConfigRegistry, ManagedAppId, ManagedAppSet, ManagedConfigDocument, UiField, UiFieldKind,
};

pub fn render_settings_index(
    registry: &ConfigRegistry,
    managed: &ManagedAppSet,
    documents: &[ManagedConfigDocument],
) -> String {
    let rows = registry
        .apps()
        .into_iter()
        .map(|app| {
            let state = if managed.contains(app) {
                "managed"
            } else {
                "local"
            };
            let revision = documents
                .iter()
                .find(|document| document.app == app)
                .map(|document| document.revision.to_string())
                .unwrap_or_else(|| "-".to_string());
            format!(
                "<li><a href=\"/settings/{app}\">{}</a> <code>{state}</code> <span>revision {revision}</span></li>",
                escape_html(app.display_name())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Loom Settings</title></head><body><h1>Loom Settings</h1><ul>{rows}</ul></body></html>"
    )
}

pub fn render_app_settings_page(
    registry: &ConfigRegistry,
    app: ManagedAppId,
    document: &ManagedConfigDocument,
) -> String {
    let Some(adapter) = registry.get(app) else {
        return "<!doctype html><html><body><h1>Unknown app</h1></body></html>".to_string();
    };
    let sections = adapter
        .ui_sections(&document.config)
        .into_iter()
        .map(|section| {
            let fields = section
                .fields
                .into_iter()
                .map(|field| render_field(&field))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "<section><h2>{}</h2>{fields}</section>",
                escape_html(&section.title)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>{display} Settings</title></head>
<body>
<h1>{display} Settings</h1>
<form id="config-form">
<input type="hidden" name="expected_revision" value="{revision}">
{sections}
<button type="submit">Save</button>
</form>
<pre id="message"></pre>
<script>
const form = document.getElementById('config-form');
const message = document.getElementById('message');
function assignPath(target, path, value) {{
  const parts = path.split('.');
  let node = target;
  for (let i = 0; i < parts.length - 1; i += 1) {{
    node[parts[i]] = node[parts[i]] || {{}};
    node = node[parts[i]];
  }}
  node[parts[parts.length - 1]] = value;
}}
form.addEventListener('submit', async (event) => {{
  event.preventDefault();
  const config = {{}};
  for (const field of form.querySelectorAll('[data-config-path]')) {{
    const path = field.getAttribute('data-config-path');
    const value = field.type === 'checkbox' ? field.checked : field.value;
    assignPath(config, path, value);
  }}
  const response = await fetch('/v1/configuration/apps/{app}', {{
    method: 'PUT',
    headers: {{'content-type': 'application/json'}},
    body: JSON.stringify({{
      expected_revision: Number(form.expected_revision.value),
      config
    }})
  }});
  message.textContent = response.ok ? 'Saved.' : await response.text();
}});
</script>
</body>
</html>"#,
        display = escape_html(adapter.display_name()),
        revision = document.revision,
        sections = sections,
        app = app,
    )
}

fn render_field(field: &UiField) -> String {
    let value = field
        .value
        .as_ref()
        .map(field_value_to_string)
        .unwrap_or_default();
    match &field.kind {
        UiFieldKind::Boolean => {
            let checked = if field
                .value
                .as_ref()
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                " checked"
            } else {
                ""
            };
            format!(
                "<label>{}<input type=\"checkbox\" data-config-path=\"{}\"{}></label>",
                escape_html(&field.label),
                escape_html(&field.path),
                checked
            )
        }
        UiFieldKind::Select => {
            let options = field
                .options
                .iter()
                .map(|option| {
                    let selected = if option.value == value {
                        " selected"
                    } else {
                        ""
                    };
                    format!(
                        "<option value=\"{}\"{}>{}</option>",
                        escape_html(&option.value),
                        selected,
                        escape_html(&option.label)
                    )
                })
                .collect::<Vec<_>>()
                .join("");
            format!(
                "<label>{}<select data-config-path=\"{}\">{options}</select></label>",
                escape_html(&field.label),
                escape_html(&field.path)
            )
        }
        UiFieldKind::Text | UiFieldKind::Number => format!(
            "<label>{}<input data-config-path=\"{}\" value=\"{}\"></label>",
            escape_html(&field.label),
            escape_html(&field.path),
            escape_html(&value)
        ),
    }
}

fn field_value_to_string(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{built_in_registry, ManagedConfigDocument};

    #[test]
    fn settings_index_links_managed_apps() {
        let registry = built_in_registry();
        let managed = ManagedAppSet::parse("tea,hook");
        let html = render_settings_index(&registry, &managed, &[]);
        assert!(html.contains("Loom Settings"));
        assert!(html.contains("href=\"/settings/tea\""));
        assert!(html.contains("href=\"/settings/hook\""));
    }

    #[test]
    fn app_settings_page_renders_fields_and_revision() {
        let registry = built_in_registry();
        let adapter = registry.get(ManagedAppId::Tea).expect("Tea adapter");
        let document = ManagedConfigDocument::new(
            ManagedAppId::Tea,
            adapter.schema_version(),
            adapter.default_config(),
        );
        let html = render_app_settings_page(&registry, ManagedAppId::Tea, &document);
        assert!(html.contains("Tea Settings"));
        assert!(html.contains("name=\"expected_revision\""));
        assert!(html.contains("human_ticket_default_approval_policy"));
    }
}
