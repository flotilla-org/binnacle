#!/usr/bin/env python3
"""Ask Jev (TypeSafe System One) how much users want each grid feature, and how well it fits binnacle.

Usage: score_gaps_jev.py FEATURES.json --raw jev_raw.jsonl --out scored.json
       [--only id,id] [--workers 8] [--model jev-latest] [--envrc PATH]

One request per feature, both questions batched. The state is the feature alone (name, description,
category, kind, which surveyed products have it, the cited demand evidence and notes). binnacle's own
status is never in the state, so the scores stay independent of the audit.

- `demand` (Score, five levels): how much users of data tables and grids want the feature.
- `fit` (Score, five levels): how much it matters for binnacle's uses, described in the instructions.

There is no paid-tier question: the product cells already record which vendors charge for a feature,
and the output carries that as `paid_share`.

Answers are cached in the raw file keyed by (feature id, question-set hash, state hash, model), so
reruns only pay for new or changed questions and states. The published
docs/research/data/grid-feature-gaps.json is a valid FEATURES input (it keeps the fields the state uses). Writes FEATURES with `demand`, `fit`, their
confidences and probabilities, and `demand_x_fit` added.

Needs TYPESAFE_API_KEY in the environment; falls back to the `export TYPESAFE_API_KEY=` line of the
file given by --envrc (default ./.envrc). The key is never printed or written. Stdlib only (urllib).
"""
import argparse
import concurrent.futures as cf
import hashlib
import json
import os
import re
import time
import urllib.error
import urllib.request

API = "https://api.typesafe.ai/v1/systemone"
FALLBACK_MODEL = "jev-1.13.0"

PRODUCT_NAMES = {
    "aggrid": "AG Grid", "muix": "MUI X Data Grid", "glide": "Glide Data Grid", "tanstack": "TanStack Table",
    "handsontable": "Handsontable", "tabulator": "Tabulator", "revogrid": "RevoGrid", "perspective": "FINOS Perspective",
    "rdg": "react-data-grid", "datatables": "DataTables", "excel": "Microsoft Excel", "sheets": "Google Sheets",
    "visidata": "VisiData", "ratatui_table": "ratatui Table widget", "tui_csv": "csvlens", "k9s": "k9s",
    "qt": "Qt QTableView", "egui": "egui table", "datasette": "Datasette", "tad": "Tad",
}
SUPPORT = {"core": "built in", "paid": "paid tier only", "plugin": "official plugin or extension",
           "partial": "partly", "no": "no"}
KINDS = {
    "widget": "the table component itself",
    "source": "a data operation the table's data source performs (sort, filter, group, load)",
    "app": "the application around a table (tabs, saved views, history, file handling)",
}

USES = ("binnacle is a table core with adapters for terminal (ratatui), web (canvas) and native (C API) apps. "
        "It is used for: a keyboard-driven terminal viewer for query and command result sets, read-mostly, "
        "in a developer tool; a terminal CSV viewer that opens large files; a table widget embedded in "
        "terminal, web and native desktop apps, from small forms to tables with millions of rows; and later "
        "a table served to other processes over a protocol. It is not a spreadsheet and not a BI tool.")

QUESTIONS = {
    "demand": {
        "type": "score",
        "instructions": "How much do users of data tables and data grids want `feature`? Judge from "
                        "`feature.products` (how many grids and table tools ship it) and `feature.demand_evidence`.",
        "criteria": [
            "Niche: rarely asked for, and few grids or table tools offer it.",
            "Specialist: a particular group of users wants it, and a minority of products offer it.",
            "Common: many products offer it and users ask for it regularly, though many users never need it.",
            "Expected: most serious grids offer it, and its absence draws repeated requests and complaints.",
            "Table stakes: users reject a grid or table viewer that lacks it.",
        ],
    },
    "fit": {
        "type": "score",
        "instructions": {
            "binnacle": USES,
            "question": "How much does `feature` matter for binnacle's uses?",
        },
        "criteria": [
            "Irrelevant: none of binnacle's uses would need it.",
            "Peripheral: only an unusual embedding app would want it; the terminal viewers would not miss it.",
            "Useful: some of the uses benefit from it, but each works well without it.",
            "Important: at least one of the uses feels clearly incomplete without it.",
            "Essential: a terminal result-set viewer, a CSV viewer or an embedded table cannot do its basic job without it.",
        ],
    },
}


def api_key(envrc):
    k = os.environ.get("TYPESAFE_API_KEY")
    if not k and os.path.exists(envrc):
        m = re.search(r'^\s*export\s+TYPESAFE_API_KEY=["\']?([^"\'\s]+)', open(envrc).read(), re.M)
        k = m and m.group(1)
    if not k:
        raise SystemExit("TYPESAFE_API_KEY not set (and not found in the envrc file)")
    return k


def state(f):
    products = {PRODUCT_NAMES[p]: SUPPORT[v] for p, v in f["products"].items() if v in SUPPORT}
    have = sum(v != "no" for v in products.values())
    v = {
        "name": f["name"],
        "description": f["description"],
        "category": f["category"],
        "kind": f"{f['kind']}: {KINDS[f['kind']]}",
        "products": products,
        "product_summary": f"{have} of {len(products)} surveyed products have it in some form",
        "demand_evidence": [f"{d['kind']}: {d['signal']}" for d in f["evidence"]] or ["none found"],
    }
    if f.get("notes"):
        v["notes"] = f["notes"]
    return {"feature": v}


class ModelRejected(Exception):
    pass


def call(key, payload, tries=6):
    req = urllib.request.Request(API, data=json.dumps(payload).encode(), method="POST",
                                 headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"})
    for t in range(tries):
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                return json.load(resp)
        except urllib.error.HTTPError as e:
            if e.code in (429, 529, 500, 502, 503) and t < tries - 1:
                time.sleep(float(e.headers.get("retry-after") or 2 ** t))
                continue
            body = e.read().decode(errors="replace")[:500]
            if e.code in (400, 404, 422) and "model" in body.lower():
                raise ModelRejected(body)
            raise SystemExit(f"HTTP {e.code}: {body}")
        except (urllib.error.URLError, TimeoutError):
            if t == tries - 1:
                raise
            time.sleep(2 ** t)


def digest(x):
    return hashlib.sha256(json.dumps(x, sort_keys=True).encode()).hexdigest()[:12]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("features")
    ap.add_argument("--raw", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--only")
    ap.add_argument("--workers", type=int, default=8)
    ap.add_argument("--model", default="jev-latest")
    ap.add_argument("--envrc", default=".envrc")
    a = ap.parse_args()
    feats = json.load(open(a.features))
    if a.only:
        keep = a.only.split(",")
        feats = [f for f in feats if f["id"] in keep]
    qhash = digest(QUESTIONS)
    cache = {}
    if os.path.exists(a.raw):
        for l in open(a.raw):
            x = json.loads(l)
            cache[(x["id"], x["qhash"], x["shash"], x["model"])] = x

    def cached(f, model):
        return cache.get((f["id"], qhash, digest(state(f)), model))

    model = a.model
    todo = [f for f in feats if not cached(f, model)]
    print(f"{len(feats)} features; {len(todo)} to ask (question set {qhash}, model {model})")
    key = api_key(a.envrc) if todo else None
    t0, tin, tout = time.time(), 0, 0

    def ask(f):
        return call(key, {"model": model, "state": state(f), "questions": QUESTIONS})

    def record(f, resp, raw):
        nonlocal tin, tout
        x = {"id": f["id"], "qhash": qhash, "shash": digest(state(f)), "model": model,
             "served_by": resp["model"], "usage": resp["usage"],
             "answers": {q: {k: v[k] for k in ("score", "confidence", "probabilities")}
                         for q, v in resp["answers"].items()}}
        cache[(x["id"], qhash, x["shash"], model)] = x
        raw.write(json.dumps(x) + "\n")
        raw.flush()
        tin += resp["usage"]["input_tokens"]
        tout += resp["usage"].get("output_tokens", 0)

    with open(a.raw, "a") as raw:
        if todo:  # the first request also tells us whether the model name is accepted
            try:
                first = ask(todo[0])
            except ModelRejected as e:
                print(f"model {model} rejected ({e}); using {FALLBACK_MODEL}")
                model = FALLBACK_MODEL
                todo = [f for f in feats if not cached(f, model)]
                first = ask(todo[0]) if todo else None
            if first:
                record(todo[0], first, raw)
                todo = todo[1:]
        with cf.ThreadPoolExecutor(a.workers) as ex:
            futs = {ex.submit(ask, f): f for f in todo}
            for i, fut in enumerate(cf.as_completed(futs), 1):
                record(futs[fut], fut.result(), raw)
                if i % 25 == 0 or i == len(todo):
                    print(f"  {i}/{len(todo)}  {time.time() - t0:.0f}s  {tin:,} input, {tout:,} output tokens")
    for f in feats:
        x = cached(f, model)
        for q in QUESTIONS:
            ans = x["answers"][q]
            f[q] = round(ans["score"], 3)
            f[f"{q}_confidence"] = round(ans["confidence"], 3)
            f[f"{q}_probabilities"] = {k: round(v, 4) for k, v in ans["probabilities"].items()}
        f["demand_x_fit"] = round(f["demand"] * f["fit"], 3)
        f["jev_model"] = x["served_by"]
        surveyed = [v for v in f["products"].values() if v != "unknown"]
        f["paid_share"] = round(sum(v == "paid" for v in surveyed) / len(surveyed), 3) if surveyed else None
    json.dump(feats, open(a.out, "w"), indent=1)
    total_in = sum(x["usage"]["input_tokens"] for x in cache.values())
    total_out = sum(x["usage"].get("output_tokens", 0) for x in cache.values())
    print(f"this run: {tin:,} input, {tout:,} output tokens; whole cache: {total_in:,} input, {total_out:,} output")
    if a.only:
        for f in sorted(feats, key=lambda f: -f["demand_x_fit"]):
            print(f"  {f['id']:28} demand {f['demand']:.2f} ({f['demand_confidence']:.2f})  fit {f['fit']:.2f} ({f['fit_confidence']:.2f})")


if __name__ == "__main__":
    main()
