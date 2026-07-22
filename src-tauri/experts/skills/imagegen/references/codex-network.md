# IYW Fusion Network Access

The bundled image CLI calls the IYW Fusion API over HTTPS. A sandboxed Agent may
need network permission before the command can connect.

## Required Destination

```text
https://gateway.iyw.cn/iyw-fusion-api/v1
```

Do not switch hosts to work around a denial. Do not reveal the token or request
headers while diagnosing connectivity.

## Diagnosis

1. Confirm the user is signed in to iyw-claw without printing account data.
2. Confirm the runtime permits outbound HTTPS to `gateway.iyw.cn`.
3. Run the image command once. Preserve the non-secret status and error body.
4. If the environment requires approval, request access for that exact command
   and destination.
5. Do not automatically retry a request whose charge/completion state is
   uncertain.

`--dry-run` validates parameters without reading credentials or using the
network. It cannot prove that a live request will authenticate successfully.
