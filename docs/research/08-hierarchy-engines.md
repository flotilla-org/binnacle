# Grouping, subtotals and pivots in query engines

How do query engines express grouping, subtotals and pivots, and can a Table Source backed by one hand binnacle a grouped or pivoted view ready-made? This note answers the research ticket [#179](https://github.com/flotilla-org/binnacle/issues/179) for the map [Hierarchical axes (#177)](https://github.com/flotilla-org/binnacle/issues/177). It covers SQL `GROUP BY` with `GROUPING SETS`, `ROLLUP`, `CUBE` and `GROUPING()`, DuckDB `PIVOT` and `UNPIVOT`, Polars, DataFusion, and Perspective (its own engine and its Virtual Server, which pushes a grouped view down to DuckDB, ClickHouse or Postgres).

Terms follow [`CONTEXT.md`](https://github.com/flotilla-org/binnacle/blob/main/CONTEXT.md): the Table Source owns every data operation and answers a Window (a span of Positions) with Keys; Positions are gapless; a Change Notice tells the Grid how an axis changed. Numbering: 06 is on `research/column-presentation`, 07 is left to the sibling grid-side ticket, and this is 08 as the ticket asked.

Everything here is as of 2026-10-21. Claims come from official docs or source pinned to a commit. Every query below was run, and its output is shown as returned. Pinned sources:

| Short name | Pinned source |
|---|---|
| DuckDB docs | [`duckdb/duckdb-web@d51b69e`](https://github.com/duckdb/duckdb-web/tree/d51b69e91dcc1873255226d1ee74b5ceb888842a), the `docs/2.0` tree |
| DuckDB | [`duckdb/duckdb@c15a232`](https://github.com/duckdb/duckdb/tree/c15a232a57040c59d5004ea4a7e54520260b2db2); queries run on the `duckdb` 2.0.0 Python package |
| DataFusion | [`apache/datafusion@6c1727f`](https://github.com/apache/datafusion/tree/6c1727fcd9230dda428f8c3a2397fce235544bd5); queries run on `datafusion` 55.0.0 (Python) |
| Polars | [`pola-rs/polars@8973ffe`](https://github.com/pola-rs/polars/tree/8973ffe9843fbca5196ec88e42ad5a440d7436d3); queries run on `polars` 2.0.0 |
| Perspective | [`perspective-dev/perspective@0857a7d`](https://github.com/perspective-dev/perspective/tree/0857a7d9c1e34f9d82496b8a81f6778944178b85) (the commit doc 06 pinned); queries run on `perspective-python` 5.6.0 |
| PostgreSQL | [Grouping sets, §7.2.4](https://www.postgresql.org/docs/current/queries-table-expressions.html#QUERIES-GROUPING-SETS) and [the `GROUPING` function](https://www.postgresql.org/docs/current/functions-aggregate.html#FUNCTIONS-GROUPING-TABLE), as the reference for the standard SQL forms |

Every query uses this table, run with `uv run --with duckdb python` (or `--with polars`, `--with datafusion`, `--with perspective-python`):

```sql
create table sales as select * from (values
 ('EU','DE','a',10),('EU','DE','b',20),('EU','FR','a',5),
 ('US','CA','a',7),('US','CA','b',1),('US','NY','b',30),('US','NY','b',30)
) t(region, country, product, amount);
```

## Summary

- **One query returns leaves, subtotals and the grand total together, with each row's level marked.** `GROUP BY ROLLUP(...)` plus `GROUPING(...)` does it in DuckDB, DataFusion, PostgreSQL and Polars SQL. The engine merges the levels internally; nothing outside the engine re-aggregates. How it merges differs, and that decides which aggregates are cheap (below).
- **Display order is not free.** Without `ORDER BY` the rows come back in no useful order (Polars returned subtotals first, then leaves). Keeping a subtotal beside its group, and sorting groups by an aggregate, takes an `ORDER BY` built from `GROUPING()` bits plus a window function per level. Perspective's Virtual Server generates exactly that, and it is shown below.
- **The level marker is not optional.** A `NULL` group value in the data and the `NULL` a subtotal row puts in a rolled-up column look the same. Only `GROUPING()` tells them apart (Q5). So a subtotal row's Key must include the grouping id, not just the path values.
- **A pivot needs a first pass over distinct values, or the values declared up front.** SQL needs every output column known before the query runs. DuckDB's `PIVOT` without `IN` becomes two queries (`SELECT DISTINCT` into an `ENUM`, then the pivot). Lazy Polars requires `on_columns`. DataFusion has no `PIVOT`. Perspective's own engine discovers columns as data arrives and adds new ones on update. A changed value set is a new schema: in binnacle terms, a Change Notice on the column Axis.
- **A Window over a grouped, partly collapsed result has three known shapes:** materialise everything and `LIMIT`/`OFFSET` (Perspective Virtual Server, which has no expand or collapse yet); keep a flattened list of the visible tree in the source and slice it (Perspective's engine); or compute a per-level skeleton with child counts, map Positions by prefix sums, and fetch only the expanded groups that meet the Window (Q13). Sorting groups by an aggregate still needs that aggregate for every sibling at each expanded level.
- **Sum, count, min and max recombine from partial results. Distinct count and median do not, unless the partials carry the whole value set.** DuckDB builds coarser `ROLLUP` levels by combining aggregate states of the finest level, and falls back to separate passes over the input when any aggregate is `DISTINCT` or ordered. DataFusion's partial state for `count(distinct)` and `median` is the list of values. Perspective's engine updates `sum` incrementally but recomputes `median` and distinct count from the rows under each node.

## One query or two, and who merges

### The SQL forms

`GROUPING SETS` lists the groupings to compute in one statement. `ROLLUP(a, b)` is sugar for `(a, b), (a), ()`, which is n+1 sets, and `CUBE` gives all 2^n combinations ([DuckDB docs, `grouping_sets.md` L107-L111](https://github.com/duckdb/duckdb-web/blob/d51b69e91dcc1873255226d1ee74b5ceb888842a/docs/2.0/sql/query_syntax/grouping_sets.md#L107-L111)). The result is defined as the `UNION ALL` of one `GROUP BY` per set, with `NULL` in the columns a set does not group on ([L84](https://github.com/duckdb/duckdb-web/blob/d51b69e91dcc1873255226d1ee74b5ceb888842a/docs/2.0/sql/query_syntax/grouping_sets.md#L84)). `GROUPING(cols...)` (also `GROUPING_ID`) returns a bit mask with a 1 for each listed column that was rolled up in this row, so 0 is a leaf ([L113-L117](https://github.com/duckdb/duckdb-web/blob/d51b69e91dcc1873255226d1ee74b5ceb888842a/docs/2.0/sql/query_syntax/grouping_sets.md#L113-L117); [PostgreSQL `GROUPING`](https://www.postgresql.org/docs/current/functions-aggregate.html#FUNCTIONS-GROUPING-TABLE)).

Q1, one statement, every level, with each row's level marked:

```sql
select region, country, sum(amount) total, grouping(region, country) lvl
from sales group by rollup(region, country)
order by grouping(region) desc, region nulls first, grouping(country) desc, country nulls first;
```
```
region  country  total  lvl
NULL    NULL       103    3   -- grand total
EU      NULL        35    1   -- subtotal
EU      DE          30    0
EU      FR           5    0
US      NULL        68    1
US      CA           8    0
US      NY          60    0
```

Arbitrary sets work the same way. Q3, `group by grouping sets ((region,country),(region),(product),())`, returned `gid` 1 for the four (region, country) rows, 3 for the two region rows, 6 for the two product rows and 7 for the total. So the mask says which set a row came from, not just its depth. `CUBE(region, country, product)` on seven rows gave 29 rows (Q4): a cube grows with the product of distinct counts, not with the row count.

Q5, the `NULL` collision. Add a row whose region is `NULL`:

```sql
select region, sum(amount), grouping(region) g
from (select * from sales union all select null, 'XX', 'a', 100)
group by rollup(region) order by all;
```
```
region  sum(amount)  g
EU               35  0
US               68  0
NULL            100  0   -- a real group whose value is NULL
NULL            203  1   -- the grand total
```

DataFusion gives the same two `NULL` rows, told apart only by `grouping(region)`. Perspective's Virtual Server always selects `GROUPING_ID(...) AS "__GROUPING_ID__"` next to the row path for this reason ([`table_make_view.rs` L999-L1004](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-client/src/rust/virtual_server/generic_sql_model/table_make_view.rs#L999-L1004)).

### How each engine merges the levels

The answer to "who merges" is the engine, but the four engines do it four ways:

- **DuckDB: a cascade of combined states.** A grouping-sets optimizer orders the sets so that each is computed from the smallest already-computed superset: "for ROLLUP this forms a chain, for CUBE a lattice rooted in the complete grouping set" ([`grouping_sets_optimizer.cpp` L87-L115](https://github.com/duckdb/duckdb/blob/c15a232a57040c59d5004ea4a7e54520260b2db2/src/optimizer/grouping_sets_optimizer.cpp#L87-L115)). The finest level aggregates the base data and exports aggregate states; coarser levels combine those states ([L284-L323](https://github.com/duckdb/duckdb/blob/c15a232a57040c59d5004ea4a7e54520260b2db2/src/optimizer/grouping_sets_optimizer.cpp#L284-L323)). `EXPLAIN` of Q1 shows it: `Hash Group By ... sum(#2) EXPORT_STATE` once over the scan, then `Hash Group By Groups: #0 Aggregates: combine_aggr(#1)` and `Ungrouped Aggregate combine_aggr(#0)` over the materialised CTE. The rewrite is refused for "DISTINCT / ORDER BY aggregates [which] cannot be computed by combining states of a finer aggregation" ([L56-L60](https://github.com/duckdb/duckdb/blob/c15a232a57040c59d5004ea4a7e54520260b2db2/src/optimizer/grouping_sets_optimizer.cpp#L56-L60)). In that case it falls back to one branch per set over the input ([L265-L271](https://github.com/duckdb/duckdb/blob/c15a232a57040c59d5004ea4a7e54520260b2db2/src/optimizer/grouping_sets_optimizer.cpp#L265-L271)). `EXPLAIN` of the same rollup with `count(distinct product)` shows three separate `Hash Group By`s each reading the input CTE.
- **DataFusion: each input row once per set.** The planner expands the sets into a `PhysicalGroupBy` with a null mask per set ([`aggregates/mod.rs` L417-L434](https://github.com/apache/datafusion/blob/6c1727fcd9230dda428f8c3a2397fce235544bd5/datafusion/physical-plan/src/aggregates/mod.rs#L417-L434)). `EXPLAIN` shows one `AggregateExec: mode=Partial, gby=[(NULL as region, NULL as country), (region@0 as region, NULL as country), (region@0 as region, country@1 as country)]` keyed with a `__grouping_id`, then a `FinalPartitioned` merge. So each input row is aggregated n+1 times, and no level is built from another.
- **Polars: a rewrite to `UNION ALL`.** Polars SQL lowers grouping sets "to a `UNION ALL` of ordinary aggregations over one shared input, plus the `GROUPING()` metadata" ([`grouping_sets.rs` L1-L2](https://github.com/pola-rs/polars/blob/8973ffe9843fbca5196ec88e42ad5a440d7436d3/crates/polars-sql/src/grouping_sets.rs#L1-L2)). The DataFrame API has `group_by` but no rollup: `dir(pl.LazyFrame)` lists `group_by`, `group_by_dynamic` and `rolling`. A Polars source wanting subtotals in Python runs one `group_by` per level and concatenates, merging itself.
- **Perspective's engine: a tree, kept up to date.** Its sparse tree holds one node per group at every level, and each node holds aggregates. `sum` is updated by adding the delta to the stored value (`new_value.set(dst_scalar.add(src_scalar))`), unless a re-aggregate is forced ([`sparse_tree.cpp` L1998-L2033](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-server/cpp/perspective/src/cpp/sparse_tree.cpp#L1998-L2033)). `median` and `distinct count` gather the primary keys under the node and reduce over all their values again ([L1589-L1603](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-server/cpp/perspective/src/cpp/sparse_tree.cpp#L1589-L1603), [L2209-L2231](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-server/cpp/perspective/src/cpp/sparse_tree.cpp#L2209-L2231)). Its `group_rollup_mode` picks `"rollup"` (every subtotal and the grand total, the default), `"flat"` (leaves only) or `"total"` ([`grouping_and_pivots.md` L41-L55](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/docs/md/explanation/view/config/grouping_and_pivots.md#L41-L55)).

So "two queries, merged by a layer above" is never needed for correctness when the engine speaks `ROLLUP`. It is what a source does when its engine lacks `ROLLUP` (Polars' DataFrame API), or when it wants only some levels (see Windows below). A layer doing that merge itself can only recombine the aggregates that recombine (last section).

## Ordering with subtotals

Rows from a grouping-sets query come back in no defined order. Polars SQL returned `US/NULL, EU/NULL, NULL/NULL` before the leaves, and DuckDB's own docs example comes back interleaved ([`grouping_sets.md` L66-L82](https://github.com/duckdb/duckdb-web/blob/d51b69e91dcc1873255226d1ee74b5ceb888842a/docs/2.0/sql/query_syntax/grouping_sets.md#L66-L82)). Display order is the source's job, in `ORDER BY`:

- **Subtotal before (or after) its leaves:** sort level by level on `grouping(level-0 .. level-k) desc` (or `asc` to put totals last), then that level's value. Q1 does it for two levels.
- **Groups sorted by an aggregate, subtotal kept with its group:** the sort key for a group's rows has to be the group's subtotal, which a window function copies onto every row of the group. Q2:

```sql
select region, country, sum(amount) total, grouping(region, country) lvl
from sales group by rollup(region, country)
order by grouping(region) desc,
  sum(sum(amount)) filter (where grouping(country) = 1) over (partition by region) desc,
  region, grouping(country) desc, total desc;
```
```
NULL  NULL  103  3
US    NULL   68  1
US    NY     60  0
US    CA      8  0
EU    NULL   35  1
EU    DE     30  0
EU    FR      5  0
```

Perspective's Virtual Server generates the general form. For each group level it emits `GROUPING_ID(groups up to this level) DESC`, then, for each sort, the aggregate at the leaf level or `first_value(agg) OVER __WINDOW_n__` at an inner level, then the level's path value ([`table_make_view.rs` L1016-L1120](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-client/src/rust/virtual_server/generic_sql_model/table_make_view.rs#L1016-L1120)). Each `__WINDOW_n__` is partitioned by the grouping id and the path up to level n ([L1145-L1185](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-client/src/rust/virtual_server/generic_sql_model/table_make_view.rs#L1145-L1185)). Perspective's own engine sorts siblings within each parent the same way: in the run above, a sort on `amount desc` put `US` before `EU` and `NY` before `CA`, with the grand total first.

## Pivot columns: discovered or declared

SQL needs every output column's name and type fixed before execution. DuckDB says so plainly: "DuckDB, like most SQL engines, requires that all column names and types be known at the start of a query", so a `PIVOT` that detects its columns "must be translated into multiple queries" ([`internals/pivot.md` L18-L20](https://github.com/duckdb/duckdb-web/blob/d51b69e91dcc1873255226d1ee74b5ceb888842a/docs/2.0/internals/pivot.md#L18-L20)). The first query is `CREATE TEMPORARY TYPE __pivot_enum_0_0 AS ENUM (SELECT DISTINCT year::VARCHAR FROM cities ORDER BY year)`, and its values are injected into the pivot's `IN` clause ([L33-L45](https://github.com/duckdb/duckdb-web/blob/d51b69e91dcc1873255226d1ee74b5ceb888842a/docs/2.0/internals/pivot.md#L33-L45)). Up to `pivot_filter_threshold` (20) values it then runs as filtered aggregates, and above that as list aggregates unpacked by a `PhysicalPivot` operator ([L76](https://github.com/duckdb/duckdb-web/blob/d51b69e91dcc1873255226d1ee74b5ceb888842a/docs/2.0/internals/pivot.md#L76)). `EXPLAIN` of Q6 confirms it: the plan already names `'a'` and `'b'` (`sum_no_overflow(#1) FILTER (WHERE product IS NOT DISTINCT FROM 'a')`), so the discovery pass ran before planning.

```sql
pivot sales on product using sum(amount) group by region;                     -- Q6: discovered
pivot sales on product in ('a','b','z') using sum(amount) group by region;    -- Q7: declared
```
```
Q6: region a  b          Q7: region a  b  z
    EU     15 20             US     7  61 NULL
    US     7  61             EU     15 20 NULL
```

`UNPIVOT` is the inverse, written as `unnest` over lists of names and values ([`internals/pivot.md`, `UNPIVOT`](https://github.com/duckdb/duckdb-web/blob/d51b69e91dcc1873255226d1ee74b5ceb888842a/docs/2.0/internals/pivot.md)). Q8 round-trips Q6 back to four (region, product, total) rows.

The other engines:

- **Polars.** Eager `DataFrame.pivot` discovers the values. `LazyFrame.pivot` takes a required `on_columns`, "what value combinations will be considered for the output table" ([`lazyframe/frame.py` L8160, L8182-L8183](https://github.com/pola-rs/polars/blob/8973ffe9843fbca5196ec88e42ad5a440d7436d3/py-polars/src/polars/lazyframe/frame.py#L8160)). Without it the call fails with `missing 1 required positional argument: 'on_columns'`. A declared value with no rows came back as `0` for `sum` where DuckDB gives `NULL`, so an empty pivot cell is engine-specific.
- **DataFusion 55** rejects `PIVOT`: `This feature is not implemented: Unsupported ast node Pivot`. A DataFusion source writes the filtered aggregates itself (Q14's form), so it must know the values first.
- **Perspective's engine** discovers values as data arrives. After `table.update(...)` with a new product `c`, the view's `column_paths()` grew from `a, b` to `a, b, c` without the view being rebuilt. Its Virtual Server builds pivots from DuckDB `PIVOT ... ON ... USING first(...)` over a grouped source ([`table_make_view.rs` L842-L870](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-client/src/rust/virtual_server/generic_sql_model/table_make_view.rs#L842-L870)), so it inherits DuckDB's discovery pass.

**What a changing value set costs.** In SQL a new value means re-running the distinct pass and replanning, because the schema changed. For binnacle the column Axis gains or loses items. That is an insertion or removal on the column axis, or a `Reset` if the source cannot say which. The other axis is untouched. Declared values (Q7, Polars `on_columns`) trade that churn for empty columns, and they suit a source that knows its domain, such as months.

## Filtering before and after aggregation

- **`WHERE`** runs before grouping. It changes every subtotal, because filtered rows never reach any level.
- **`HAVING`** runs on the grouped rows. With `ROLLUP` it applies to every level, and it does not recompute the subtotals of the rows it removes. Q15 keeps leaves over 10 and every subtotal:

```sql
select region, country, sum(amount) total, grouping(region, country) lvl from sales
group by rollup(region, country) having sum(amount) > 10 or grouping(country) = 1
order by grouping(region) desc, region, grouping(country) desc, country;
```
```
NULL  NULL  103  3
EU    NULL   35  1   -- still includes FR (5), which HAVING removed
EU    DE     30  0
US    NULL   68  1   -- still includes CA (8)
US    NY     60  0
```

  Both meanings are legitimate ("subtotal of what is shown" is a `WHERE`, "subtotal of everything" is a `HAVING`). A grouped Filter Spec has to say which one it means.
- **`QUALIFY`** filters on window functions after them, as `HAVING` does for aggregates ([`qualify.md` L14](https://github.com/duckdb/duckdb-web/blob/d51b69e91dcc1873255226d1ee74b5ceb888842a/docs/2.0/sql/query_syntax/qualify.md#L14)). It expresses "top n per group". Q9, top country per region by product `b` sales, returned `EU DE 20` and `US NY 60`. DataFusion 55 accepts `QUALIFY` too.

**Pushing a filter on a pivot down.** There are two cases:

- A Condition on a **pivoted cell column** (column `b > 50`) is a filter on an aggregate. It pushes down as `HAVING` on the filtered aggregate that produces the column. Q14 gives the same single `US` row as filtering after the `PIVOT` (Q14b):

  ```sql
  select region, sum(amount) filter (where product = 'a') a, sum(amount) filter (where product = 'b') b
  from sales group by region having sum(amount) filter (where product = 'b') > 50;
  ```
- A filter on the **split column itself** (`product <> 'a'`) is a `WHERE` before the pivot. It also removes the discovered column: Q14c returns only `region, b`. Perspective behaves the same: a view with `split_by: ["product"]` and `filter: [["product", "==", "a"]]` has `column_paths()` of just `a`. So filtering the row Axis's data can change the column Axis's items.

## A Window over a grouped, collapsed result

The Grid asks for a Window of Positions and gets Keys back. With subtotals and collapse, Position n is "the n-th visible node of a tree in display order". The known shapes:

**1. Materialise the whole result, then `LIMIT`/`OFFSET`.** Perspective's Virtual Server creates the view as `CREATE TABLE {view_id} AS ({query})` ([`generic_sql_model.rs` L350-L361](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-client/src/rust/virtual_server/generic_sql_model.rs#L350-L361)). Each viewport fetch is `SELECT ... FROM {view_id} LIMIT {end - start} OFFSET {start}` ([L460-L476](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-client/src/rust/virtual_server/generic_sql_model.rs#L460-L476)). Expand and collapse are not implemented: the file opens with a TODO that "row expand/collapse in the datagrid needs datamodel support, this is likely a 'collapsed' boolean column in the temp table we `UPDATE`" ([L18-L21](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-client/src/rust/virtual_server/generic_sql_model.rs#L18-L21)). This computes every group at every level up front. Collapse then becomes a filter on the materialised table plus a `row_number()` to renumber Positions. Q11 does that for "EU collapsed, US expanded", Positions 1 to 3:

```sql
with r as (select region, country, sum(amount) total, grouping(region, country) lvl
           from sales group by rollup(region, country)),
vis as (select * from r where lvl = 3 or lvl = 1 or (lvl = 0 and region in ('US'))),
ord as (select *, row_number() over (order by lvl = 3, region, lvl desc, country) - 1 as pos from vis)
select * from ord where pos between 1 and 3 order by pos;
```
```
US  NULL  68  1  pos 1
US  CA     8  0  pos 2
US  NY    60  0  pos 3
```

**2. Keep the visible tree flattened in the source and slice it.** Perspective's engine keeps a `t_traversal`: a vector of the visible nodes in display order, each with its depth and descendant count. `collapse_node` erases the node's descendant span from the vector and adjusts the counts of its ancestors ([`traversal.cpp` L207-L239](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-server/cpp/perspective/src/cpp/traversal.cpp#L207-L239)). A viewport is `get_view_nodes(bidx, eidx)`, a direct slice that also reports each node's depth, expanded flag and whether it has children ([L359-L371](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-server/cpp/perspective/src/cpp/traversal.cpp#L359-L371)). In the run, `collapse(1)` (collapsing `US`) dropped `num_rows()` from 7 to 5, and `to_json(start_row=1, end_row=3)` returned two rows: Windows are half-open. This is memory proportional to the visible rows, which a Table Source may hold even though the Grid may not.

**3. Compute a skeleton per level and fetch only what the Window meets.** For each expanded level, one grouped query returns each group, its sort aggregate and its child count. Prefix sums over the child counts of expanded groups give each group's first Position. Children are then fetched only for the expanded groups that intersect the Window. Q13a, with `US` expanded, groups sorted by total:

```sql
select region, sum(amount) total, count(distinct country) n_children,
  1 + row_number() over (order by sum(amount) desc) - 1
    + coalesce(sum(case when region in ('US') then count(distinct country) else 0 end)
        over (order by sum(amount) desc rows between unbounded preceding and 1 preceding), 0) as first_pos
from sales group by region order by total desc;
```
```
region  total  n_children  first_pos
US         68           2          1   -- grand total is Position 0; US's children are 2 and 3
EU         35           2          4
```

Q13b then fetches `where region in ('US') group by region, country`, only for the groups the Window reaches. This is the only shape that avoids computing every leaf group, but it has two limits. First, sorting siblings by an aggregate needs that aggregate for every sibling at each expanded level, because a group's Position depends on all the groups that sort before it. Second, the row count of the Axis is the sum of the expanded groups' child counts, so the count costs a query per expanded level.

**Keys.** Whichever shape is used, a row's identity is its path plus its grouping id. Q5 shows that the path alone collides when data holds `NULL`. A Key Interner over `(grouping_id, path values)` gives stable Keys for subtotal rows across re-sorts and collapses. Collapsing a group is then a `Refiltered` on the row Axis (the same Keys, a subset), not a `Reset`.

## Which aggregates recombine

Whether a layer above the engine can build a coarser level from a finer one depends on the aggregate's partial state. Q10 re-aggregates per-country partials up to region and compares with the truth (Q10b):

```sql
with leaf as (select region, country, sum(amount) s, count(*) n, min(amount) mn, max(amount) mx,
                     count(distinct product) dp, median(amount) med
              from sales group by all)
select region, sum(s), sum(n), min(mn), max(mx), sum(dp) dp_wrong, median(med) med_wrong
from leaf group by region order by 1;
```
```
            sum  count  min  max  distinct  median
re-agg  EU   35      3    5   20         3    10.0
truth   EU   35      3    5   20         2    10.0
re-agg  US   68      4    1   30         3    17.0
truth   US   68      4    1   30         2    18.5
```

- **Recombine exactly from constant-size partials:** `sum` (sum of sums), `count` (sum of counts), `min`, `max`, and `avg` if carried as (sum, count). DuckDB's cascade relies on this (`EXPORT_STATE`, then `combine_aggr`).
- **Do not recombine from their outputs:** `count(distinct)` double-counts values shared between children (EU has product `a` in DE and FR). `median` of medians is wrong (US: 17.0, not 18.5). DuckDB refuses to cascade these ([`grouping_sets_optimizer.cpp` L56-L60](https://github.com/duckdb/duckdb/blob/c15a232a57040c59d5004ea4a7e54520260b2db2/src/optimizer/grouping_sets_optimizer.cpp#L56-L60)).
- **Recombine only if the partial carries the value set.** DataFusion merges partial states through `Accumulator::state` and `merge_batch` ([`accumulator.rs` L374-L387](https://github.com/apache/datafusion/blob/6c1727fcd9230dda428f8c3a2397fce235544bd5/datafusion/expr-common/src/accumulator.rs#L374-L387)). For `count(distinct)` the state field is a list of the distinct values ([`count.rs` L300-L311](https://github.com/apache/datafusion/blob/6c1727fcd9230dda428f8c3a2397fce235544bd5/datafusion/functions-aggregate/src/count.rs#L300-L311)), and for `median` it is a list of all values ([`percentile_cont.rs` L217](https://github.com/apache/datafusion/blob/6c1727fcd9230dda428f8c3a2397fce235544bd5/datafusion/functions-aggregate/src/percentile_cont.rs#L217)). They merge, but the state is as large as the data under the group. Perspective does the equivalent by re-reading the rows under each node.
- **Recombine approximately from bounded sketches:** `approx_count_distinct` is HyperLogLog in DuckDB ([`aggregates.md` L487](https://github.com/duckdb/duckdb-web/blob/d51b69e91dcc1873255226d1ee74b5ceb888842a/docs/2.0/sql/functions/aggregates.md#L487)). DataFusion ships `approx_distinct`, `approx_median` and `approx_percentile_cont` ([`functions-aggregate/src`](https://github.com/apache/datafusion/tree/6c1727fcd9230dda428f8c3a2397fce235544bd5/datafusion/functions-aggregate/src)).

One more trap: the definition of median differs between engines. Perspective's `median` over the integers `[7, 1, 30, 30]` returned 30, where DuckDB says 18.5. Its `get_aggregate_median` averages the two middle values only for floating-point columns, and otherwise takes the upper one ([`sparse_tree.cpp` L2809-L2831](https://github.com/perspective-dev/perspective/blob/0857a7d9c1e34f9d82496b8a81f6778944178b85/rust/perspective-server/cpp/perspective/src/cpp/sparse_tree.cpp#L2809-L2831)).

## What this means for binnacle

**Who computes subtotals.** The engine, in one query, whenever it speaks `ROLLUP`/`GROUPING SETS`. A Table Source over DuckDB, DataFusion or Postgres can hand binnacle a grouped view with leaves, subtotals and totals together, each row marked by `GROUPING()`, in display order from a generated `ORDER BY`. Perspective's Virtual Server is a working generator to copy, ordering included. Nothing in binnacle needs to re-aggregate. A layer that does (a Polars DataFrame source, or a source caching a finer level) must restrict itself to sum, count, min, max and avg-as-pair, or carry value sets or sketches.

**Where they go.** The engine decides placement only through `ORDER BY`: subtotal before or after its group is a direction on the `GROUPING()` terms, and sort-by-aggregate keeps a subtotal with its group through a window function. Both belong to the source's View Spec, not the Grid. Subtotal rows are ordinary rows on the row Axis with Keys from `(grouping_id, path)`. Their level and collapse state are per-row facts the source reports, as Perspective's `get_view_nodes` reports depth, expanded and has-children.

**Windows.** Materialise-and-slice is the easy path and what the Virtual Server does. A flattened visible tree in the source (Perspective's engine) makes collapse a span removal and a Window a slice. A per-level skeleton with child counts avoids computing unexpanded leaves, at the cost of a query per expanded level. Collapse is `Refiltered`, not `Reset`, if Keys are stable.

**Pivots.** Columns are discovered by a distinct pass or declared up front. A new value is a column-Axis change. A filter on the split column can remove columns, and a Condition on a pivoted column is a `HAVING`.

**Filters.** A grouped Filter Spec has to choose whether subtotals count filtered-out rows (`HAVING`) or not (`WHERE`).
