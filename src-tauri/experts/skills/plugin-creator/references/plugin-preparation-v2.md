# v2 connector preparation

HostGateway v2 connectors may declare `preparation` inside `.iyw-plugin.json`. This is part of the signed version content; do not edit an installed manifest or add a parallel runtime. The v1 native/portable validator does not validate v2 packages.

```json
{
  "preparation": {
    "parameters": [
      {"key":"SERVICE_ENDPOINT","label":"Service endpoint","required":true},
      {"key":"SERVICE_TOKEN","label":"Access token","required":true,"secret":true}
    ],
    "authentication": {
      "status":{"toolName":"account_status","arguments":{}},
      "authenticatedPointer":"/structuredContent/authenticated",
      "login":{"toolName":"begin_login","arguments":{}},
      "authorizationUrlPointer":"/structuredContent/authorizationUrl",
      "logout":{"toolName":"disconnect_account","arguments":{}}
    }
  }
}
```

The names are examples. Use the plugin's actual documented environment and MCP tools. Each action must reference a capability declared by the same connector. Status must have `readOnlyHint: true` both in the capability declaration and in the actual MCP tool annotations. Login/logout are explicit user operations; never infer them from tool names or invoke them during import/validation.

Parameters are strings, at most32, with unique environment keys. Supported fields: key, label, description, required, secret, defaultValue, placeholder. Credentials must be secret with no default. Reserved host credential prefixes, runtime injection variables, HOME/APPDATA/XDG/PATH and private *_DATA_DIR variables are prohibited. Actual secrets belong in the installed client's secure configuration, never in the package, default, auth arguments or logs.

Auth arguments are explicit objects and may not contain credentials or templates. JSON Pointers address the entire serialized MCP CallToolResult: status must return a boolean, login may return a validated HTTPS or local HTTP URL. The host does not parse arbitrary text to guess authentication. Missing login/logout actions leave those operations to the plugin UI.

Run the focused author validator before uploading a v2 package:

```sh
node scripts/validate_plugin_preparation.mjs <plugin-directory>
```

The script only validates preparation declarations and does not perform login or certify tool execution. The original package validator and publication service still validate components, schemas, permissions and artifacts. Native import also calls this preparation validator before writing the generated manifest; it never invents provider authentication settings.

The Fusion plugin upload editor supports preparation objects keyed by connector key. It transforms only the uploaded manifest copy and recalculates its digest. Published changes require a new version. Existing manifests without preparation keep their previous digest.
