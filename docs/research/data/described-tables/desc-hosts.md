## hosts

| Field | Type | Required | Categories | Reference | Properties |
|---|---|---|---|---|---|
| `metadata.name` | Text | true |  |  | {"roles":["key"],"sortable":true,"title":"Name","whereOperators":["=="]} |
| `metadata.namespace` | Text | true |  |  | {"sortable":true,"title":"Namespace","whereOperators":[]} |
| `metadata.resourceVersion` | Text | true |  |  | {"sortable":false,"whereOperators":[]} |
| `metadata.creationTimestamp` | Timestamp | true |  |  | {"roles":["time"],"sortable":true,"title":"Created","whereOperators":[]} |
| `metadata.deletionTimestamp` | Timestamp | false |  |  | {"roles":["time"],"sortable":true,"title":"Deleted","whereOperators":[]} |
| `metadata.labels` | Cbor | true |  |  | {"additionalProperties":{"sortable":false,"type":"string","whereOperators":["=="]},"sortable":false,"whereOperators":[]} |
| `metadata.annotations` | Cbor | true |  |  | {"sortable":false,"whereOperators":[]} |
| `status.phase` | Text | false |  |  | {"roles":["state"],"sortable":true,"title":"Phase","whereOperators":[]} |
| `status.conditions` | Cbor | false |  |  | {"items":{"properties":{"lastTransitionTime":{"format":"date-time","type":"string"},"message":{"type":"string"},"reason":{"type":"string"},"status":{"type":"string"},"type":{"type":"string"}},"type":"object"},"sortable":false,"title":"Conditions","type":"array","whereOperators":[]} |
| `spec` | Cbor | true |  |  | {"sortable":false,"title":"Spec","whereOperators":[]} |
| `status` | Cbor | false |  |  | {"sortable":false,"title":"Status","whereOperators":[]} |
