#!/bin/sh
# Downloads the benchmark corpora into a directory (default: benches/data next to this script).
#
#   benches/scripts/fetch-corpora.sh [DIR]
#
# Silesia is unpacked as is.
# The Pizza&Chili files are streamed and cut to MAX_BYTES so that a download never needs the full corpus.
# Every file is verified against corpora.sha256 when that file lists it.
# Set UPDATE=1 to rewrite corpora.sha256 from the downloaded files instead of verifying.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
dir=${1:-$here/../data}
MAX_BYTES=${MAX_BYTES:-67108864}
sums=$here/../corpora.sha256

silesia_url=http://sun.aei.polsl.pl/~sdeor/corpus/silesia.zip
pc=http://pizzachili.dcc.uchile.cl

# name|url of the gzip stream
pizza_files="
dna|$pc/texts/dna/dna.50MB.gz
english|$pc/texts/nlang/english.50MB.gz
proteins|$pc/texts/protein/proteins.50MB.gz
sources|$pc/texts/code/sources.50MB.gz
dblp.xml|$pc/texts/xml/dblp.xml.50MB.gz
cere|$pc/repcorpus/real/cere.gz
fib41|$pc/repcorpus/artificial/fib41.gz
tm29|$pc/repcorpus/artificial/tm29.gz
rs.13|$pc/repcorpus/artificial/rs.13.gz
"

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@"; else shasum -a 256 "$@"; fi
}

mkdir -p "$dir"

if [ ! -f "$dir/xml" ]; then
    # The archive is untrusted: unpack it into a fresh directory and move the files out.
    tmp=$(mktemp -d "$dir/.silesia.XXXXXX")
    curl -fL "$silesia_url" -o "$tmp/silesia.zip"
    mkdir "$tmp/out"
    unzip -q "$tmp/silesia.zip" -d "$tmp/out"
    mv "$tmp"/out/* "$dir"/
    rm -rf "$tmp"
fi

echo "$pizza_files" | while IFS='|' read -r name url; do
    [ -n "$name" ] || continue
    [ -f "$dir/$name" ] && continue
    echo "fetching $name"
    # head closes the pipe early on purpose, so a failing curl or gzip is not an error here.
    { curl -fsL "$url" | gzip -dc | head -c "$MAX_BYTES" >"$dir/$name.part"; } 2>/dev/null || true
    [ -s "$dir/$name.part" ] || { echo "download of $name failed" >&2; exit 1; }
    mv "$dir/$name.part" "$dir/$name"
done

names="dickens mozilla mr nci ooffice osdb reymont samba sao webster xml x-ray"
for n in $(echo "$pizza_files" | cut -d'|' -f1); do names="$names $n"; done

if [ "${UPDATE:-0}" = 1 ]; then
    (cd "$dir" && sha256 $names) >"$sums"
    echo "wrote $sums"
else
    (cd "$dir" && sha256 -c "$sums")
fi
