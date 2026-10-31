# Described-table wire samples from flotillad

> Copied for binnacle#298 from the branch `research/described-table-wire` (folder `docs/research/data/described-tables/`, with this README and `capture.sh`), unchanged. The source's fixture replay, decoding agreement and the `binnacle-tables` example read them. `../fixtures/crews.ndjson` is not a capture: it is made by hand, with the nulls, empty texts and invalid values these lack.

These were captured on 2026-10-28 for binnacle#282, read-only, from feta's live daemon over its Unix socket (`~/.config/flotilla/run/flotilla.sock`). Feta runs generation r909, flotilla `84d0d2f2eff6`. Its table code is the same as main at `d8c6677f6`. `[primitives]` is off on feta.

Each file `X` has a sibling `X.headers` holding the HTTP status and response headers. The capture script is `capture.sh`; the first run omitted `namespace` and got empty rows.

## What is here

| File | Request |
|---|---|
| `listing.json` | `GET /tables` |
| `desc-{hosts,fulfilmentkinds,repositories,convoys,vessels}.{json,cbor}` | `GET /tables/{t}` with `Accept: application/json` or `application/cbor` |
| `desc-hosts.md` | `?format=markdown` |
| `rows-{hosts,fulfilmentkinds,repositories}.{ndjson,cborseq}` | full rows, `application/x-ndjson` or `application/cbor-seq` |
| `rows-hosts-no-namespace.ndjson` | full rows with no `namespace`: expected 200 and no rows. **Not captured yet**: `capture.sh` asks for it since binnacle#299, and the operator runs it on a flotillad host |
| `rows-convoys-cols.ndjson` | `columns=` a projection (all 324 convoys) |
| `rows-convoys-sort-where.{ndjson,cborseq}` | `sort=-metadata.creationTimestamp&where=.status.phase == Running`. This returned 0 rows; see below |
| `rows-vessels-page1.ndjson`, `rows-vessels-page2.ndjson` | `sort=metadata.name&limit=5`, then `&cursor=` from page 1 |
| `rows-convoys-count-{exact,estimate}.ndjson` | `limit=3&count=exact` and `count=estimate` |
| `watch-vessels.ndjson`, `watch-hosts.ndjson` | `?watch` for 75 s each |
| `error-unknown-table.json`, `error-bad-where.json` | a 404 and a 400 body |

Convoy and vessel rows are restricted with `columns=` to metadata and `status.phase`, because their `spec` and `status` carry crew briefs and stall text. Host, FulfilmentKind and Repository rows are complete.

The fake table server (`binnacle_testkit::fake_tables`, binnacle#299) is checked against these: each `rows-*` table it serves has the same message types and keys, in both encodings.

## Notes for a table source

- **Namespace.** Rows are scoped by `?namespace=`, which defaults to `default`. Flotilla's records live in `flotilla`, so a client that omits it sees empty tables with no error. Always pass it.
- **Engine-store tables are missing.** Engine kinds and their `.log` projections are listed only when `[primitives]` is on, so `listing.json` has resource kinds only. Their samples need a daemon with the switch on: the silo rehearsal or a devbox, later.
- **Resource tables share one generic schema.** The columns are `metadata.*` plus `status.phase`, `status.conditions`, and `spec` and `status` as whole `object` columns. Per-kind fields live inside those objects; they are not columns. Named queries as tables (flotilla#3249) will be the richer, typed ones.
- **How values look in JSON.**
  - `datetime`: an RFC 3339 string with nanoseconds, for example `2026-07-27T22:00:39.399401197Z`.
  - `object`: a nested JSON object.
  - `metadata.labels` has `additionalProperties` describing the value type.
  - `status.conditions` is an `array` with `items` described inline.
  - Nullable columns have `constraints.required: false` and appear as `null`.
  - There are no binary or categorical columns among the resource kinds.
- **Field properties seen:** `name`, `type`, `title` (on most columns), `roles` (`key`, `time`, `state`), `sortable`, `whereOperators` (only `==`, and only on some columns), `constraints.required`. Every resource table has `primaryKey: ["metadata.name"]` and `foreignKeys: []`. No units appear.
- **Message order.**
  - A read is `table` (with the schema), then `count` if asked, then `rows` (`cursor`, `partial`), then `synced`.
  - A watch continues with `upsert` (`key` plus row), and with `progress` heartbeats carrying `sequence`.
  - No `delete` or `resync` occurred within the 75 s windows.
  - `incarnation` is `durable`.
- **Cursors** are opaque base64 strings. Decoded, they hold the sort, table, filter, values, version and namespace.

## Odd things worth a look

- **`partial` vs `cursor`.** `rows-vessels-page2.ndjson` has `partial: true` and `cursor: null`. This is by design (`tables.rs:746`): `partial` means "this response is not the whole table" (more rows, an offset, or a cursor was given), and a null `cursor` means there are no further pages. Use the cursor, not `partial`, to decide whether to fetch again.
- **`where` on `status.phase == Running` returned nothing** because no convoy has that phase. The values are `Landed`, `Abandoned`, `Active` and `Failed`. The filter works; my value was wrong.
- **Default page size.** `rows-convoys-cols.ndjson` returned 200 of the 324 convoys, with a cursor and no `limit` given, so the server applies a default page size of 200.
- **Only feta's own Host row** appears in `rows-hosts.ndjson`. Other hosts' records aren't served from this table in this namespace.
