#!/bin/bash
set -u
S=$HOME/.config/flotilla/run/flotilla.sock
O=/tmp/described-tables; rm -rf $O; mkdir -p $O
c(){ out=$1; shift; curl -s --max-time 20 --unix-socket $S -D $O/$out.headers "$@" -o $O/$out; echo "$out $(head -1 $O/$out.headers | tr -d '\r') $(wc -c <$O/$out)"; }
c listing.json -H 'Accept: application/json' http://localhost/tables
for t in hosts fulfilmentkinds repositories convoys vessels; do
  c desc-$t.json -H 'Accept: application/json' http://localhost/tables/$t
  c desc-$t.cbor -H 'Accept: application/cbor' http://localhost/tables/$t
done
c desc-hosts.md 'http://localhost/tables/hosts?format=markdown'
# full rows only for config-like kinds
for t in hosts fulfilmentkinds repositories; do
  c rows-$t.ndjson -H 'Accept: application/x-ndjson' http://localhost/tables/$t/rows?namespace=flotilla
  c rows-$t.cborseq -H 'Accept: application/cbor-seq' http://localhost/tables/$t/rows?namespace=flotilla
done
# convoys/vessels: no spec/status objects (they carry briefs and stall text)
COLS='columns=metadata.name,metadata.namespace,metadata.creationTimestamp,metadata.deletionTimestamp,metadata.labels,status.phase'
c rows-convoys-cols.ndjson -H 'Accept: application/x-ndjson' "http://localhost/tables/convoys/rows?namespace=flotilla&$COLS"
c rows-convoys-sort-where.ndjson -H 'Accept: application/x-ndjson' "http://localhost/tables/convoys/rows?namespace=flotilla&$COLS&sort=-metadata.creationTimestamp&where=.status.phase%20%3D%3D%20Running"
c rows-convoys-sort-where.cborseq -H 'Accept: application/cbor-seq' "http://localhost/tables/convoys/rows?namespace=flotilla&$COLS&sort=-metadata.creationTimestamp&where=.status.phase%20%3D%3D%20Running"
c rows-vessels-page1.ndjson -H 'Accept: application/x-ndjson' "http://localhost/tables/vessels/rows?namespace=flotilla&$COLS&sort=metadata.name&limit=5"
cur=$(grep -o '"cursor":"[^"]*"' $O/rows-vessels-page1.ndjson | tail -1 | cut -d'"' -f4)
c rows-vessels-page2.ndjson -H 'Accept: application/x-ndjson' "http://localhost/tables/vessels/rows?namespace=flotilla&$COLS&sort=metadata.name&limit=5&cursor=$cur"
c rows-convoys-count-exact.ndjson -H 'Accept: application/x-ndjson' "http://localhost/tables/convoys/rows?namespace=flotilla&$COLS&limit=3&count=exact"
c rows-convoys-count-estimate.ndjson -H 'Accept: application/x-ndjson' "http://localhost/tables/convoys/rows?namespace=flotilla&$COLS&limit=3&count=estimate"
c error-unknown-table.json -H 'Accept: application/json' http://localhost/tables/nosuchtable
c error-bad-where.json -H 'Accept: application/x-ndjson' "http://localhost/tables/hosts/rows?namespace=flotilla&where=garbage"
# watch: hosts heartbeat and vessels change; 90s each
curl -s --max-time 75 --unix-socket $S -H 'Accept: application/x-ndjson' "http://localhost/tables/vessels/rows?namespace=flotilla&$COLS&watch" -o $O/watch-vessels.ndjson; echo "watch-vessels $(wc -l <$O/watch-vessels.ndjson) lines"
curl -s --max-time 75 --unix-socket $S -H 'Accept: application/x-ndjson' "http://localhost/tables/hosts/rows?namespace=flotilla&watch" -o $O/watch-hosts.ndjson; echo "watch-hosts $(wc -l <$O/watch-hosts.ndjson) lines"
